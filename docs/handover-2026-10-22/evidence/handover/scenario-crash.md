# Handover: scenario-navigation crash/corruption, busy cursor, slow start

Repo: /home/sirati/devel/rust/civ3-renderer (uncommitted, shared tree). Nothing committed.
Helper notes: preview smear/lag fix -> /mnt/datasets/c3r-build/handover/preview-smear.md (separate file, by the preview helper).

## Task
User on the Mac (TS Neo, ARM64 Wine/FEX, separate Metal viewer) reported:
1. After going back and forth between scenarios, backgrounds vanished, text became garbage letters, then the game hung ("New Unit" cursor, backdrop only, music stopped later).
2. The busy/loading cursor is a "New Unit" arrow, not a wait cursor.
3. The game takes 20-30 s to come up.
Later added: the scenario preview is smeared vertically and runs at 15 fps (preview helper, see preview-smear.md).

## DONE

### 1a. Garbage text / missing backgrounds: ROOT CAUSE FOUND AND FIXED
- Cause: the bridge sends a whole art description to the viewer (`prepare_assets`) before building it itself. When a newer scenario selection stops that build, the bridge's art thread drops its state (`art.rs`, state = None). The viewer still builds the stopped whole and then grows it with the next addition. The bridge got an EMPTY growth for that addition and adopted its generation anyway, so scenes named the viewer's scenario table while using the bridge's base-table sprite ids.
- Evidence (logs under /mnt/datasets/c3r-build/crash-20261020/):
  - Session 41117 (build 4d29): `bridge.log` 37.069 "whole description ... stopped"; 39.927 "art: grown by 0 sprites ... 0 sprites". Viewer at 22:01:40 shows "new assets through generation 8 (12766 sprites)" while the bridge had 7152.
  - Session 47149 (build 3562), `s47149/`: the same pattern. Stop at 72.509, empty growth at 72.608, then the viewer's "generation 7 (13594 sprites)" against the bridge's 7980.
- Fix: explicit table generations in `crates/civ3-bridge/src/presenter.rs`.
  - New `Tables` tracks which whole description each prepared generation belongs to.
  - `adopt_assets` and `adopt_growth` now return bool and refuse any generation that isn't of the latest prepared whole, of an abandoned whole, or a growth of a non-adopted table.
  - New `abandon_assets(gen)`.
  - `prepare_assets` no longer sends additions to an abandoned whole.
- `crates/civ3-bridge/src/live.rs`:
  - A refused whole is not adopted, and `compiled` / `compiled_folders` stay unchanged; tokens are still done.
  - A STOPPED whole calls `presenter.abandon_assets`.
  - Pending growths are applied to `compiled` only if adopted.
  - The viewer side already refuses scenes outside `[whole..=through]` (`tile-viewer uses_art`), so with this nothing can name a superseded table.
- Tests (presenter.rs `table_tests`, all pass): `nothing_of_a_whole_description_given_up_is_adopted_or_sent`, `a_newer_whole_description_supersedes_the_older_and_its_growths`, `a_cancelled_rebuild_leaves_scenes_on_the_table_they_had` (real ArtThread with a stopped whole plus an addition).

### 1b. Hang: MITIGATED + INSTRUMENTED, root cause NOT proven
- Evidence: both Mac sessions show the same sequence.
  1. A worker thread of ours stops taking jobs right after it went idle. In 41117 the preview thread at 61.2 s (no "made" lines afterwards, though the game kept loading scenarios). In 47149 the art thread: the job sent at 78.4 s was never done, though the viewer built its copy in 595 ms.
  2. 5-12 s later the whole process goes silent. The bridge log stops, and the viewer reports "host appears busy/stuck".
  3. The socket stays open until the process is killed.
  - Neither wine.log has a CS timeout or an unhandled-exception message, and there are no macOS crash reports.
- Interpretation: step 1 matches a lost wake-up of a parked receiver (the problem `docs/concurrency.md` and `inproc/alloc.rs` already describe on Wine/FEX macOS). Step 2 freezes even the log writer, which only polls and frees strings. That points at the private-heap spin guard (`inproc/src/alloc.rs`) being held forever, or a Wine/FEX-internal lock. Not proven.
- Not reproducible on papa: Linux Wine 11 old-WoW, separate viewer via the launcher, 230 scripted Up/Down presses over scenarios 6-9. No hang, so it is likely FEX/macOS-specific.
- Mitigation, all workers now use bounded waits, so a lost wake-up costs at most 5-20 ms instead of forever:
  - `crates/civ3-bridge/src/lib.rs`: new `BOUNDED_WAIT` (20 ms) and `recv_bounded()` (try_recv plus sleep).
  - art.rs: art thread uses recv_timeout.
  - text.rs: fonts thread uses recv_timeout.
  - preview.rs: preview thread uses `recv_bounded` with 5 ms.
  - replay_thread.rs: `recv_bounded` with 10 ms.
  - ui_opacity.rs: `recv_bounded`.
  - `crates/viewer-transport/src/lib.rs`: the writer's `park()` became `park_timeout(10 ms)`.
- Diagnostics, `crates/inproc/src/alloc.rs`:
  - The guard records holder thread id and time (HOLDER / HELD_SINCE).
  - New `watchdog(path, watched)` thread allocates nothing and writes straight to `civ3-bridge-watchdog.log` in the game dir. It notes when the allocator guard is held for 2 s or more (with the thread id), and when the publisher stops making rounds (`live::PUBLISHER_ROUNDS`).
  - It is started in `lib.rs init()`.
  - Test: `alloc::tests::the_watchdog_notes_a_held_guard_and_a_stopped_counter`. It passes under Wine: `cargo test -p inproc --target x86_64-pc-windows-gnu --lib alloc`.
- Checked on papa: the DLL works, the watchdog log stays empty in normal use, and preview latency is unchanged (median 32 ms).

### 2. Busy cursor (cursor helper, done; report relayed)
- Cell 12 of Art/cursor.pcx is the "NEW UNIT" arrow.
- The game has no wait art. Its exes and DLLs have no cursor resources, and on busy it calls SetCursor(IDC_WAIT=32514) (bridge log: "system pointer Some(32514)"). Under Wine, IDC_WAIT is the hourglass in Wine's user32.dll (syswow64, group cursor 32514).
- Changes:
  - `crates/civ3-bridge/src/cursor.rs`: IDC_WAIT and IDC_APPSTARTING map to the system Wait/Progress pointer again.
  - New `crates/civ3-assets/src/syscursor.rs` (plus lib.rs and loader.rs): image source `system-cursor:<IDC>/<size>`, a safe PE resource parser that reads user32.dll from windir, WINEPREFIX or the drive_c above the game roots. If the cursor is missing the image is clear.
  - `assets/civ3/civ3.toml`: viewer_loading and viewer_busy use `system-cursor:32514/32`.
  - preview.rs: `LOADING_SET` is "Viewer@loading/0".
  - `tile-viewer host_status.rs`: ICON_SPRITE is "Viewer@busy/0".
- Tests: civ3-assets, the bridge cursor/preview/assets tests and tile-viewer all pass.
- Limit: on real Windows, or on a host without a Wine prefix, the panel and preview icons are blank.

### 3. Slow start (startup helper)
- The helper finished, but its final report never reached me. It may have gone to the orchestrator; check its messages and `git diff` of crates/civ3r-launcher and civ3r-viewer startup code for its changes.
- Raw data: the crash session went from app start 22:00:23Z to viewer listening 22:00:54Z to bridge loaded 22:01:00.8Z to menu art 22:01:04Z, so about 31 s passed before the viewer started (launcher, registry, Wine prefix).

## IN PROGRESS / tree state
- My changes are complete and compile: `cargo check -p civ3-bridge -p viewer-transport -p inproc -p civ3r-viewer` passes on the host.
- The i686 DLL build currently FAILS on someone else's in-progress edit: `crates/civ3-bridge/src/trace.rs:636`, a non-exhaustive match on the new `ScreenDraw::Indexed`. That isn't mine. The DLL built cleanly with my changes before that edit appeared.
- Bridge host tests: 372 pass. Two failures in `units::tests` (army recolour) come from another agent's work, not mine.

## NOT done / next steps
- Root cause of the total freeze is unproven. After deploying, if it hangs again, read `civ3-bridge-watchdog.log` next to civ3-bridge.log on the Mac (runtime/game/Civilization III Complete/Conquests/).
  - "allocator's guard held by thread X": the private-heap spin guard is the single point of failure. Consider a different allocator design, e.g. thread-local arenas, or VirtualAlloc'd pools with lock-free frees.
  - "the publisher has not moved": take a `sample <wine pid>` on the Mac during the hang. The release agent's `release-diag/thread-busy.py` summarizes it.
- The repro is not automated on the Mac: there is no accessibility permission for synthetic keys, and the user was active. A test-only approach that worked on papa: inject Up/Down `scene::Input::Key` in the publisher after `stale_inputs.filter(...)` (live.rs about line 990), gated by an env var, plus clicks to open the Conquests list (title Conquests button at title-relative (262,456) in the centred 1024x768 title).
- The viewer still compiles an abandoned whole (wasted 0.6-1.5 s of CPU). A cancel message would need a protocol bump.

## Evidence locations
- /mnt/datasets/c3r-build/crash-20261020/: session 41117 logs (bridge.log, viewer, wine, registry, app), `s47149/` (second hang on build 3562, bridge.log and viewer log), `diag/` (release agent's old samples).
- Mac: /Users/sirati/civ3r-tsneo-20261019/runtime/logs/ (session logs), runtime/logs/release-diag/ (samples, `thread-busy.py`).

## Gotchas
- NEVER compile on the Mac (user hard rule). Cross-build on papa and copy binaries only.
- On papa, the Linux launcher (`civ3-play-macos --separate-viewer`) needs `WGPU_BACKEND=vulkan` (it defaults to metal). The ~/.cache/winvm/wine-wow64 store path is GC'd; use ~/.cache/winvm/wine. A fresh prefix needs `wineboot -i` first.
- The bridge's `Presenter::generation` must only change through `adopt_*`. Scenes carry it (`scene.assets`), and the viewer draws only scenes whose generation is within the whole it shows.
- Many agents edit the same tree. Re-read before editing; failures in units.rs, trace.rs and snap.rs belong to others.

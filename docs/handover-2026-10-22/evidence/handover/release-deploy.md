# Handover: Civ III renderer release and deployment to TS Neo

## Task and goal

Run the combined release of the renderer to the user's MacBook ("TS Neo"):
1. Full tests on papa.
2. An immutable source snapshot with a manifest and SHA.
3. The ARM64 macOS artifacts plus a matching Windows DLL.
4. Deployment to TS Neo, with backup and rollback.
5. On-Mac verification: Wine/FEX start, protocol match, env vars reaching FEX (`FEX_X87REDUCEDPRECISION`, `HADRON_HARDWARE_TSO`), sound-thread CPU, heap-lock lines and memory.
6. Updating `/mnt/datasets/c3r-build/macos-port/ACTIVE_STATE.md`.

Mid-task, the user added standing rules (see Gotchas). Production also changed to the in-process "dual-stack" renderer: five files.

## Installed on TS Neo now (consistent 5-file set, one snapshot)

The source snapshot is `7055148cf5c47a28d6ccb8cf3fa1e6b484fa582cbe7069877227b0c5999c35bf` (618 files), scene protocol 44. It was installed 2026-10-20T06:49:39Z.

The shader is `d19e66b8b204f1d9ee0d8c383e81f6180d5e14fa459f76551d168769d201980b`. It is embedded exactly once in each of `IFC23.dll`, `civ3r-viewer` and `civ3r_native.so`.

| File | Location under `/Users/sirati/civ3r-tsneo-20261019` | SHA-256 |
| --- | --- | --- |
| civ3-play-macos | runtime/bin | 1c47b5b2d30f741afa7f2fc452420e20deab483c4aad7e1f33963bdd1d5c792e |
| civ3r-viewer | runtime/bin | 006025c2155410237f89c54cc0c9c191eba31d822a7685ddbad5015434f20d8a |
| IFC23.dll | runtime/bin | 7988b5dd3cb58d5c589b1e4c23b806134f129eea570fe556d7cf94900f85c388 |
| civ3r_native.so | runtime/native/aarch64-unix | 24b2cb94e5c5ed990291964dd0f3fb90fb5cf5d992bbf64d7f84a5c67939be74 |
| civ3r_native.dll | runtime/native/i386-windows | 4151b134ac7c2076abea13644e54519a95c8bb349cafdf3c01c1e85d35ead935 (byte-identical to the 14:07 one) |
| App starter | ~/Applications/Civilization III.app/Contents/MacOS/civ3-app-macos | 6e09976491dd85bf79d1aaafffc0710bbab00d8008557135727da53cb18054f8 (raw 94bfa357…) |
| RUN.md | (task root) | 15d84f4c6a96927a6cf0ca1f082f0003e6372123ae1ed98bb0f96a72d5fa0954 (= the snapshot's docs/macos-tsneo.md) |

- The install record is `runtime/releases/7055148c…/install-20261020T064939Z.json`.
- The previous set was the coordinator's 14:07 dual-stack install: `c99cec3e` launcher, `b0731fad` viewer, `b98510aa` IFC23, `7a3f34d0` .so, `4151b134` .dll.
- Those files are backed up as `runtime/releases/artifact-<sha>/<name>`, and the app bundle and RUN.md in `runtime/releases/app-backup-20261020T064939Z`. The coordinator's own backup is `runtime/releases/pre-dualstack-20261020140706`.

### The 7055 snapshot contents

- 7055 is snapshot `c7b48034…` (live tree at 08:17:23 papa time) plus one file: `crates/civ3r-viewer/src/main.rs`, the game-connect timeout raised from 120 s to 15 min (the startup-fix agent's change).
- `c7b48034…` already contained:
  - the launcher registry-skip fix (`unix.rs`);
  - the system-cursor fix;
  - the dual-stack code;
  - my title/setup-screen fix (below).
- Archive, manifest and binaries: `/mnt/datasets/c3r-build/release-1019/artifacts/7055148c…/` (`cross-manifest.json` lists every hash). The extracted source is `/mnt/datasets/c3r-build/release-1019/src-7055148c…`.

### UNVERIFIED: the first launch of 7055 did not reach the menu

I launched it with `open` on the app at 06:49:59Z.
- Wine started.
- The in-process renderer picked "Apple A18 Pro (Metal)".
- The bridge log stopped after 2.2 s, at its text-font lines.
- After about 12 min there was still no "forwarding input" line and no "unixlib"/"attached to the host's application" line.

I then stopped it cleanly with `civ3-app-macos stop`. Nothing is left running.

This may be FEX's first-launch translation of the new DLL; the docs say minutes. It may also be a startup stall. An earlier run on the old 3a91 pair showed a similar stall: the inproc-init thread stuck in wait_suspend/NtQueryVolumeInformationFile, with winecoreaudio holding a mutex. A retry of that run worked.

Next step: launch it once more and wait up to 15 min. If it still stalls, `sample` the game process.

If 7055 is broken, roll back to the 14:07 set. Copy the five `runtime/releases/artifact-<14:07 sha>/<name>` files back to their locations; that is what `install-cross.py` does in reverse.

## Fixes I made in the live tree (all with tests)

1. **Metal shader bug** in `crates/tile-render-shaders/src/lib.rs`: `box_mean`, used by the Interpolate background sampling.
   - Naga's SPIR-V-to-MSL translation drops the last iteration of these loops on Metal.
   - Fix: one flat loop with a trailing no-op iteration.
   - The 3 failing `ui_background_sampling` interpolation tests now pass on RADV and on Metal (A18 Pro: tile-render 119/119).
2. **Title and setup screens fitted as window-sized** in two places:
   - `crates/tile-viewer/src/scaling.rs` (`content_bounds`);
   - `crates/civ3-bridge/src/interface_parts.rs` (`menu_popup`).

   The game draws the version number "1.22" in the window's bottom-left and a bar at the bottom centre. Both counted as menu content, so the fitted bounds were [1,640,2148,1408] instead of [1125,640,1024,768]. As a result the title was small, and Player Setup and the other setup screens became "popups" over the title, which stayed drawn behind them.

   Fix: only pieces that reach into the centred 1024×768 count. Tests:
   - `setup_screens_over_the_title_in_a_large_window_are_menus_not_popups` (bridge);
   - the extended `title_over_full_window_backdrop_follows_menu_scale_and_fills_height_at_100` (viewer, with the corner pieces).
3. **Log line**: the viewer's "picture shown … menus N/D" line now prints `(fitted content [x,y,w,h])`, via the `Plan::menu_bounds` field in `scaling.rs` and the format string in `tile-viewer/src/lib.rs`. Use it to confirm the title fit.
4. **In-game Preferences with a C3X modal** (done by a subagent of mine, in the live tree, NOT in 7055). All three causes are in `crates/tile-viewer/src/layout.rs`:
   - Preferences followed the pointer's pane (`following_menu`); it now skips `menu_fit` screens.
   - A modal over a menu is no longer shifted.
   - In the pane screen_place case, the modal is left out of the pane fit, gets its own camera and is placed with `modal_place` in the pane's top third. `scaling.rs` got a `ShownPiece.modal` flag.

   New test: `a_modal_over_preferences_in_a_game_keeps_preferences_and_shows_in_the_top_third`. One existing test was updated to the new behaviour. tile-viewer: 371 passed.

   At that time the civ3-bridge i686 check FAILED: `trace.rs:636` doesn't handle a new `ScreenDraw::Indexed` that another agent was mid-way adding, and 4 bridge host tests failed in text and units, also from others' edits in progress. Re-check that the tree compiles before the next snapshot.

## Test results (papa, AMD iGPU RADV, never NVIDIA)

- Snapshot 3562 (closest full run):
  - host `cargo test --workspace`: 1225 passed, 2 failed (`text_at_target`), 11 ignored. On a fresh rebuild `text_at_target` passes 3/3. The failure was a stale binary; see the shared-target-dir gotcha.
  - viewer ignored GPU tests: 3/3.
  - i686 check and release: OK.
  - Wine bridge+sys: 487 passed (1 ignored), with 40 native-art lines on RADV.
  - Wine inproc + x86 hooks: 25 passed (1 ignored).
  - Wine camera: 4.
  - Native Metal on the Mac: 531 host, 3 viewer GPU, 119 tile-render, 368 bridge lib, AppKit icon control: all passed. This was before the no-compiling-on-Mac rule.
- Snapshot 4d29 also failed `tile-viewer scaling::tests::every_setting_preserves_scale_and_reachable_interface`, from the C3X agent's unfinished work then.
- The 7055 snapshot itself has NOT had a full test pass. Run `test-snapshot.sh` on it, or on the next snapshot.

## On-Mac findings so far

- **FEX env vars:**
  - FEX reads `FEX_*` from the process environment (`FEX::Config::LoadConfig(..., _environ)` in WOW64 Module.cpp, pinned FEX 0df84d38).
  - `X87ReducedPrecision` has AffectsCodeGen=true, so it changes FEX's DiskCache bucket hash and a new RWCacheDB file appears when it takes effect. On the Mac, `RWCacheDB_73d53cfc…` grew during the x87=1 runs, while `RWCacheDB_2cd0ab50…` is from the earlier default run. That is evidence the setting reaches FEX.
  - `HADRON_HARDWARE_TSO=1` is read by Hadron's Wine patch 0010 (`enable_hardware_tso`, getenv in ntdll) and FEX patch 0004. The loader has the `cross-architecture-support` entitlement.
  - `ps -E` shows no env for the Wine game process, so that check is meaningless; use the cache-DB evidence or the sound thread instead.
- **Sound thread** (from `sample`; tool `mac/thread-busy.py`):
  - old build with default x87: the sound thread was 75% on-CPU (`audio-diag/base.sample`);
  - new build with launcher defaults (4d29 run): no game thread above about 5% except `civ3r-publish` (47%);
  - the whole game process used 0.35 core over 62 s;
  - footprint: game 220 MB, viewer 549 MB.
- **Heap lock**: no "heap section wait timed out" lines in that run's Wine log.
- **Publish rounds over 40 ms**: 13 in that run, max 1403 ms.
- **Warm-launch time**: not yet measured. The coordinator expects about 41 s → 11–14 s.
- **3 fps** at ×3 with a 192 px cursor (subagent's analysis): not the cursor, which is cached and drawn as one sprite. Likely cause: the full-resolution frame at ×3 (`scaling::drawn` k=3, 9× the pixels, outline text per glyph pixel). Confirm with `TILE_VIEWER_PROFILE=1` and by toggling sharp text ("T").

## Not done / next steps, in order

1. Retry the 7055 launch (`open "$HOME/Applications/Civilization III.app"`), waiting up to 15 min. Check that the bridge log has "unixlib"/"attached to the host's application" and that "fitted content" for the title is [1125,640,1024,768] with the zoom filling the height. Roll back if it is broken.
2. Take a new snapshot once the live tree compiles (workspace check plus the i686 bridge check). It must include the subagent's in-game Preferences fix. Run `test-snapshot.sh`, deploy with `deploy-cross.sh`, then retest on the Mac:
   - title zoom;
   - Player Setup over the title;
   - in-game Preferences + C3X Info;
   - the native cursor on the title;
   - one warm launch timed from app start to the menu.
3. Sound-thread CPU, heap-lock lines, memory and publish rounds over 2–3 min on the final build (`diag-env.sh` app mode, `analyze-diag.py`, `thread-busy.py`).
4. Update ACTIVE_STATE.md with the final release facts.

## How to build, test and deploy (all on papa; scripts in /mnt/datasets/c3r-build/release-1019)

- `snapshot.sh`: packages the live tree (git ls-files, the same as macos-port's package-source.py) into `staging/`, copies it to `artifacts/<sha>/`, extracts it to `src-<sha>/`, verifies every file and copies it into `macos-port/staging/source-history/<sha>`. Prints `SHA=…`.
- `package-dir.py BASE_SRC OUT [rel=file …]`: a derived snapshot (an existing snapshot plus specific files), used to deploy single fixes without others' half-done edits.
- `test-snapshot.sh <sha>`: run as `cd src-<sha>; nix develop path:$PWD -c ../test-snapshot.sh <sha>`. Runs the full host workspace tests (`--no-fail-fast`), the ignored viewer GPU tests, the i686 check/release and the Wine bridge/sys, inproc and camera tests. Wine runs on a private headless Wayland (wlh socket wayland-c3r-rel1019, VNC 5997, prefix `release-1019/prefix`). Logs go to `logs/snap-<sha>/`.
- `deploy-cross.sh <sha>`, nothing compiled on the Mac:
  - i686 `IFC23.dll` and `civ3r_native.dll` (marked Wine builtin), in fresh `target-win-<sha>`;
  - aarch64-apple-darwin viewer, launcher, starter and `libciv3r_native.dylib` (→ `civ3r_native.so`), via `cross-mac.sh` with the darwin toolchain in `release-1019/darwin-cross/` (compiler/sdk/rust links built from `/home/sirati/sirati-nixos/darwin/cross-toolchain.nix`) and `TILE_RENDER_PRECOMPILED_SPV`;
  - checks the shader embed counts;
  - writes `cross-manifest.json`;
  - scp's the binaries to `runtime/releases/<sha>/` on the Mac;
  - stops a running game cleanly;
  - runs `mac/install-cross.py`: hash and signature checks, backup, atomic swap, rollback, app bundle via the starter's `bundle` command, RUN.md.
- Mac diagnostics: `runtime/logs/diag-env.sh TAG app|terminal [ENV=VAL…]` launches the game. Terminal mode goes through a launchctl GUI agent, because ssh sessions can't load Wine's Mac driver. It waits for the menu, then samples, measures and stops only the launcher it started. `runtime/logs/release-diag/analyze-diag.py DIR` and `thread-busy.py SAMPLE` summarise.
- `mac/validate-release.py` and `install-release.py` are the OLD scripts that build on the Mac. Do not use them any more.

## Gotchas and user rules

- NEVER compile on the Mac (no cargo, rustc or nix builds, no test builds). It overloads the user's laptop. Cross-compile on papa and copy binaries only. Metal GPU tests may only be cross-built on papa and run briefly on the Mac; otherwise skip them.
- Deploy to the Mac IMMEDIATELY when work finishes, before testing; test afterwards. The only gates: it builds, and the protocol and shader of all parts match. If the game is running, stop it cleanly with `civ3-app-macos stop` and install anyway.
- Production is the 5-file dual-stack set. Every deploy builds and installs ALL FIVE from ONE snapshot. A 3-file deploy silently breaks the in-process mode; I did that once by accident and restored it within 30 s.
- Do not use one CARGO_TARGET_DIR for different snapshot directories. Cargo fingerprints workspace members by relative path plus mtime. Snapshots keep the live tree's mtimes, so files edited between snapshot A and A's build look "fresh" for snapshot B. This produced stale test binaries and a stale DLL. Always use `target-<sha>` / `target-win-<sha>`.
- Release DLL builds are not byte-reproducible across target dirs, so you can't compare hashes across them.
- The live tree is shared with other agents. Snapshot only when it compiles; derive snapshots (`package-dir.py`) to ship single fixes.
- Other runtimes on the Mac (e.g. `runtime-dualstack/`, `dualstack/prefix`) are separate. The install check only looks at this installation's `runtime/` processes.
- Wine launched from an ssh session cannot create windows ("graphics driver is missing"). Launch through `open` or a launchctl gui/<uid> agent.
- Never kill the user's own session without the "update anyway" rule; stop it cleanly. Never change the user's `settings.toml` without restoring it.
[Credential-related line redacted from this historical copy.]
- Never commit, stash, reset or discard in the repo. Don't edit skills.

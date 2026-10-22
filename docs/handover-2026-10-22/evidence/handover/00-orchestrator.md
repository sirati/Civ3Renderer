# Civ III renderer: orchestrator handover (Claude → Codex), 2026-10-20

## Project
[Credential-related line redacted from this historical copy.]
- **Goal:** a modern Rust renderer replacing Civ III's own renderer:
  - Generic `tile-render` and `menu` crates, plus asset loaders.
  - One Civ3-specific crate (`civ3-bridge`).
  - Safe abstractions, with `unsafe` isolated in interop crates.
  - Runs under Wine and in the Windows VM.
  - Native dual-stack: on Linux via a Wine unixlib; now also on macOS ARM64 with Wine/FEX.
- **State and evidence:** `/mnt/datasets/c3r-build/macos-port/ACTIVE_STATE.md`; docs in `docs/` (`macos-tsneo.md`, `concurrency.md`, `per-view-layers.md`, `settings-transactions.md`, `modal-input.md`, `civ3-map-rendering.md`).
- **Earlier Codex transcript:** `~/.codex/sessions/2026/10/17/rollout-*01a14bbd-1e01*.jsonl`. Claude's: `/home/sirati/.claude/projects/-home-sirati/075e62a1-1816-4c77-b606-7d347fd36f6d.jsonl`.

## Standing user rules (important)
- **Never compile on the user's MacBook TS Neo.** No cargo, rustc or nix builds, including test builds; it drove the load to 92. Cross-compile on papa for aarch64-apple-darwin (darwin cross toolchain: sirati-nixos `darwin/cross-toolchain.nix`) and copy only the binaries.
- **Deploy to the Mac immediately** when work finishes, even before testing. The only gates are that it builds and that the viewer and DLL protocol match. If the game is running, stop it cleanly (`civ3-app-macos stop`) and install.
- **Test runs on the Mac are allowed;** it is the only Apple Silicon Mac. Stop cleanly afterwards.
[Credential-related line redacted from this historical copy.]
- **Screenshots:** only Sonnet-class subagents look at images or operate GUIs; coding agents don't.
- **Builds and scratch on papa:** under `/mnt/datasets/c3r-build/` (own `CARGO_TARGET_DIR` per agent), never on the root disk; clean up afterwards. `nix develop`, `CARGO_INCREMENTAL=0`. GPU: AMD iGPU only (`TILE_RENDER_ADAPTER=radv`), never NVIDIA.
- **No locks in Rust code.** Use channels, tokio actors and the `civ3r-sync` primitives; a `no_locks` test enforces this.
- **Rust only for shipped code;** objc2 on macOS with minimal isolated `unsafe`.
- **macOS VMs:** "work" (port 22390) is ours. Never touch "wayland-host" (another session). All VMs share a 10 GB cgroup cap (`c3r-vms.slice`). Login details are in `/mnt/datasets/macvm/LOGIN.md` (never print passwords).
- **Dictation:** the user dictates, so "old mode" means Alt mode, "eyebrow" means UI, and "33x" means C3X.

## Mac production state (TS Neo)
- In-process dual-stack build installed at 14:07 local, 2026-10-20 (backup `runtime/releases/pre-dualstack-20261020140706`):
  - `runtime/bin/civ3-play-macos` `c99cec3e…`
  - `runtime/bin/civ3r-viewer` `b0731fad…`
  - `runtime/bin/IFC23.dll` `b98510aa…`
  - `runtime/native/aarch64-unix/civ3r_native.so` `7a3f34d0…`
  - `runtime/native/i386-windows/civ3r_native.dll` `4151b134…`
- **All 5 files must ship together.** The launcher defaults to in-process on macOS; `--separate-viewer` is the fallback.
- **App:** `~/Applications/Civilization III.app`, starter `civ3-app-macos` (has `stop` and `bundle` subcommands).
- **Test copies:** `runtime-dualstack/` plus `dualstack/prefix` hold the dual-stack agent's test copy; they share a prefix with `runtime/`.
- **The tree is now at scene protocol 44,** so the next deploy needs all 5 files rebuilt.

## Finished in the tree but NOT yet on the Mac (next deploy should include all)
- **Title / setup-screen fix** (`tile-viewer` `scaling.rs` `content_bounds`; `civ3-bridge` `interface_parts.rs` `menu_popup`). The version text "1.22" bottom-left and a bar at bottom centre were counted as menu content, so the title fitted as 2148×1408 and the setup screens became "popups".
- **Menu system cursor** (`inproc` `pointer.rs`; `live.rs` installs a ShowCursor hook; `tile-viewer` `cursor.rs`/`lib.rs`). The game's `SetCursor(NULL)`/`ShowCursor` made Wine hide the Mac cursor app-wide.
- **Busy cursor art:** the system IDC_WAIT hourglass, read from Wine's `user32.dll` (`civ3-assets` `syscursor.rs`). Cell 12 was a "NEW UNIT" arrow.
- **Startup-time fix** (`civ3r-launcher` `unix.rs`). The registry writes were skipped only when values are already set; those writes cost 12–30 s per launch. The viewer's game-connect timeout is now 15 min (`civ3r-viewer` `main.rs`).
- **Metal shader workaround:** Naga drops the last loop iteration in `box_mean` (`tile-render-shaders`).

## Open work: see the per-agent notes in this folder
- **release-deploy:** snapshot, cross-build all 5 files on papa, deploy; in-game Preferences + C3X modal; the 3 fps question.
- **scenario-crash:** crash and garbled text after rapid scenario switching (atlas/glyph table generations).
- **preview-smear:** scenario preview columns smeared down the window, 15 fps.
- **mac-fps-music:** 3–18 fps on the A18 Pro since the full-resolution text frames; music loops back (check that FEX_X87REDUCEDPRECISION / HADRON_HARDWARE_TSO actually reach FEX).
- **army-units:** army civ colour wrong, health bar missing.
- **tile-popup-menus:** tile info box scale (world scale), drawing bugs (cattle shadow), block the map while open, dismiss on recapture or outside click, Terrain details opening at the wrong tile (parked game camera).
- **text-fit-atomic-ui:** text overflowing popup/tooltip backgrounds; one-frame flicker (UI must update atomically, with a 150 ms fallback).
- **slider-arrow-keys:** Left/Right step a selected slider or scroll bar.
- **targeting-cursors:** tile snap only for Go To/stack move (not rebase/airlift/bombard); "not allowed" cursor over invalid targets.
- **unit-notices:** "unit already moved" notices stack and never expire; dedupe and expire by viewer time.

## Other known open items
- GPU-thread split in the viewer (no dedicated GPU thread yet).
- Viewer transport not yet on tokio.
- City screen worked-tile Alt targets.
- macOS VM `sonoma-prepared` snapshot + fresh-clone verification (`ACTIVE_STATE.md` steps).
- Leader-head blink rate: compare the game's message-loop wake-ups with the original if the user still sees fast blinking.

## New user request, not started: startup progress window
The user, 2026-10-20: "On Mac or whatever, where we need to wait for the x86 translation to happen, just display a screen saying "Loading x86" or "Compiling x86 to arch" or something like that. Just make it clear for the user what's happening. During the complete boot process there should be a small window showing what's happening and why we are waiting."
- **Window:** show a small status window from the moment the app or launcher starts until the game's first menu appears. It must be native and in Rust, with no shell scripts. On macOS, the launcher or viewer process can open it; in the in-process mode it can be the viewer window itself showing a status panel early.
- **Stages to show, each with a short reason:**
  - preparing the Wine prefix / registry (one-off after an update or a moved install);
  - starting Wine;
  - translating the game's x86 code to ARM64 (FEX cache build: slow on the first launch after an update, minutes);
  - loading the renderer / art;
  - waiting for the game.
- **Detecting stages:**
  - from the launcher's own steps;
  - from the bridge's first log or heartbeat;
  - FEX translation detected as "bridge loaded but game not connected yet while CPU-busy", or via FEX cache-file growth in the FEX cache directory.
- **Behaviour:** show elapsed time. Close as soon as the game window or menu is visible. On failure, show the error and log paths. That ties in with the launcher's new non-zero exit/alert on game death.
- **Platforms:** Linux too, e.g. the first Wine prefix creation.

## UPDATE: current Mac production state (supersedes the "Mac production state" section above)
- Installed at 06:49:39Z (08:49 local): a complete 5-file in-process set from snapshot source `7055148c…`, scene protocol 44. It includes the title/setup fix, the menu system cursor, the busy cursor and startup fixes, as far as they were in the tree at that time.
- **Its first launch did NOT reach the menu within 12 min** and was stopped cleanly.
  - Possibly just FEX's first-launch translation of the new IFC23.dll / civ3r_native.dll; unconfirmed.
  - **First thing to do:** retry one launch and watch the bridge log and viewer log. If it still hangs, roll back to the known-good 14:07 set at `runtime/releases/pre-…`. Exact commands are in release-deploy.md.
- See release-deploy.md for SHAs, retry and rollback steps.

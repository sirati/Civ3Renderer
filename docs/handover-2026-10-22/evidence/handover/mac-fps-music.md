# Handover: Mac low FPS + music loops (Civ III renderer, TS Neo, A18 Pro)

## Task
The user reports low FPS the whole time on the newest builds (3–18 fps, 64–100 ms/frame) at window 3274×2048, pixel scale ×2/×3, Scale menus ON 100%, world 1.5×, split view, cursor 192. Music sometimes loops again. The job: profile on the Mac, fix what is ours, reach ≥60 fps with no music loops, and report before/after numbers. Production is the in-process dual-stack build (runtime-dualstack, task root /Users/sirati/civ3r-tsneo-20261019).

## Measurements so far
Sources: the user's own live session (TILE_VIEWER_PROFILE was on) in runtime-dualstack/logs/session-1792475241016997000-56336.wine.log (copy: /mnt/datasets/c3r-build/perf-agent/live.wine.log), plus an 8 s `sample` of the game process (/mnt/datasets/c3r-build/perf-agent/live1.sample).

- **The GPU is the bottleneck.** "gpu frame span" p50 is 20–25 ms on the plain map view (14–16 cameras, 18–26 render passes), so 45 fps at best. On some screens it is 70–110 ms (10 fps; 1024×768-content screens such as the city screen or advisors, around 06:01–06:03, 06:06–06:09 and 06:17 UTC). "acquire" (nextDrawable wait) mirrors it: the CPU waits on the GPU. "draw (all)" on the CPU side apart from acquire and submit is only about 1–3 ms.
- The main thread is NOT saturated: in the sample it is 86% idle in mach_msg, and draw is about 8%. Display::receive's park_timeout (IDLE_WAIT) blocks the run loop about 4% of the time. Minor.
- Per-pass GPU timestamps on Apple overlap ("passes sum" is 6–8× the span), so per-view attribution from the existing log is meaningless. I added end-increment timing to fix that (see DONE).
- Micro-benchmark (passcost, cross-compiled; source /mnt/datasets/c3r-build/perf-agent/passcost): a separate render pass on a 3274×2048 target costs about 0.3 ms extra only when it touches the whole screen. Scissored small passes are nearly free. So the pass count alone does not explain 25–100 ms. The likely heavy hitters are full-resolution fragment work since sharp UI text (`scaling::drawn` boxes every camera ×k):
  1. **Text drawn from outlines** (tile-render-shaders/src/text.rs). Every glyph quad runs 3 pipelines (r, g, b), and **each recomputes the full 3-channel coverage** (4 rows × band curves × 6-step bisection) at window resolution. That is about 3× wasted work and the main suspect for text-heavy screens. Fix ideas: dual-source blending (one pass), or a glyph-coverage cache texture.
  2. World steps from the first zoomed (text) sprite on are drawn at zoom k·z straight into the full-res canvas: city labels are after the pass before fog (`ui_screen::under_pass`), so fog (2 layers, full-screen) and units (20 layers) are drawn at full res. `world::first_zoomed` is global, not per view (any text sprite anywhere makes all world views zoomed from that pass).
  3. UI sprites (big 1024×768 backgrounds scaled ~2.7×) at full res.
  Not yet measured which of these dominates. That was the next step (see IN PROGRESS).
- CPU, publisher thread `civ3r-publish`: 72% of a core. About 15% goes to NtReadVirtualMemory (inproc::mem safe reads, each with setjmp → sigprocmask + sigaltstack). About 15% goes to file-system syscalls (NtQueryDirectoryFile, NtCreateFile, NtQueryVolumeInformationFile), because `Compiler::files()` in civ3-bridge/src/live.rs builds a brand-new `GameFiles` **every publish round** (live.rs around line 1181 `let files = compiler.files(&folders);` and around line 1553 `fonts.add(&compiler.files(&folders), …)`). `GameFiles::complete` does 3× `find_ci` read_dir and starts with an empty dir cache. Fix idea: a process-wide (Latest/ArcSwap, no lock) listing cache in civ3-assets/src/files.rs, or memoise GameFiles per ArtFolders. Not done.
- **FEX settings: delivered and effective.** A 32-bit x87 probe (/mnt/datasets/c3r-build/perf-agent/probe/probe.rs → probe.exe) run with the Mac's Wine (native-wine/runtime/dist/bin/wine):
  - without env: 80-bit precision true, a 20M-iteration x87 loop takes 4801 ms;
  - with FEX_X87REDUCEDPRECISION=1 HADRON_HARDWARE_TSO=1: precision false, 287 ms.
  So FEX reads the env var. No registry or AppConfig is needed. HADRON_HARDWARE_TSO is a plain getenv in Hadron's ntdll and FEX patches. The launcher (civ3r-launcher/src/unix.rs `configure`/game command) sets both on the wine command. The installed launchers contain the strings. `ps -E` cannot show the env of Wine processes (the preloader rewrites it), which is why the earlier ps check looked empty. In the live sample the sound threads (wine_dsound_mixer and 3 neighbours) were about 98% idle, so they were not starved during that session. Remaining loop risk: CPU/GPU contention or thermal throttling on the fanless A18 Pro (GPU-bound plus a 72% publisher).

## DONE (in the shared tree; compiles: verified in a snapshot build of natives + bridge)
- crates/tile-render/src/timing.rs: `FrameTimes.increments`, each pass's end minus the previous passes' latest end. These add up to the span on overlapping tiled GPUs.
- crates/tile-viewer/src/lib.rs (profile loop after `timer.collect`): logs `gpu+ <label>` per pass alongside `gpu <label>`. Use these lines on the Mac for real per-view attribution.
- No tests were run for these two small instrumentation edits beyond the compile.

## IN PROGRESS (stopped cleanly)
- Papa measurement rig (all processes stopped):
  - prefix reflink copy /mnt/datasets/winvm/wine-civ3-perf2;
  - natives /mnt/datasets/winvm/civ3r-native-perf2;
  - snapshot tree /mnt/datasets/c3r-wt/perf-snap (one local-only patch: trace.rs match arm `ScreenDraw::Indexed { .. } => Ok(())`, because the shared tree did not compile at that moment due to another agent's WIP on ScreenDraw::Indexed in civ3-bridge/src/trace.rs:636);
  - env /mnt/datasets/c3r-build/perf-agent/run/env.sh (WLH_SOCKET=wayland-wlh-perf2, port 5973, CIV3R_SCALE=2, CIV3_SESSION_SCALE=2, TILE_VIEWER_PROFILE=1);
  - settings /mnt/datasets/c3r-build/perf-agent/run/settings.toml (pixel 2, world 1.5, split 2:1, scale_menus currently false).
  `tools/civ3-session load Egypt31.SAV` failed ("Load Game dialog did not open"): the viewer window was 3832×2106 at scale 1, not 3840×2160. Fix the output size/scale or click with mapped coordinates. Once in game, compare GPU `gpu+` per view with sharp text on/off (layout key 0x54 or TILE_VIEWER_LAYOUT text=pixel) on the AMD iGPU (accurate timestamps).
- Nothing is half-edited in the shared tree.

## NOT done / next steps
1. Attribute GPU time (papa rig, or the Mac with the `gpu+` lines once deployed). Then fix the dominant cost. Candidates in order:
   - text 3× coverage recompute (single pass via dual-source blend, or a glyph coverage cache);
   - per-view `first_zoomed`;
   - drawing the world's post-label steps at canvas res;
   - merging consecutive canvas passes.
2. Skip GPU redraws of unchanged frames (currently only `gpu.software` skips, lib.rs `unchanged`).
3. Publisher: stop rebuilding GameFiles every round (see above).
4. Optional: compare --separate-viewer.
5. Before/after fps numbers are still owed.

## Gotchas
- **Never run cargo, rustc or nix builds on TS Neo.** It overloaded the user's laptop (load 92), and the user demanded it stop. Cross-compile on papa: build the wrapper with `nix build --impure --expr 'let t = import /home/sirati/sirati-nixos/darwin/cross-toolchain.nix { inputs = (builtins.getFlake "/home/sirati/sirati-nixos").inputs; }; in t.compiler'` (→ /nix/store/l45gi47hkspp4pg6fsm3f1k0c0byvl4x-darwin-cross-clang). Then use `CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=$CC/aarch64-apple-darwin-cc CC_aarch64_apple_darwin=… AR_aarch64_apple_darwin=… cargo build --target aarch64-apple-darwin` inside the repo's `nix develop`. That worked for a wgpu binary, which runs on the Mac. Copy only binaries.
- The Mac has 8 GB of RAM and other agents run VMs and games there. Check `pgrep -fl Civ3Conquests` and the user's idle time (`ioreg -c IOHIDSystem | grep HIDIdleTime`) before any GPU test.
- The shared tree often fails to compile mid-edit by other agents. Snapshot it to /mnt/datasets/c3r-wt for measurement builds.
- No input injection on the physical Mac. There is no autoload of saves, so in-game Mac measurements come from the user's own sessions (profiling env is on there).
- Build dirs: /mnt/datasets/c3r-build/perf-agent/target (several GB; delete when done), /mnt/datasets/c3r-wt/perf-snap, /mnt/datasets/winvm/wine-civ3-perf2 (reflink copy). The Mac perf-agent dir has been removed. ~/.cargo/registry on the Mac (366 MB) got wgpu crates from my aborted build.

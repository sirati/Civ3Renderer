# Handover: scenario-preview smear + lag (Civ III renderer, /home/sirati/devel/rust/civ3-renderer)

## Task
User report on the Mac (snapshot ef68...): the Load Scenario preview (world beside the menu, scene::Beside, fitted and downscaled) shows a thin correct strip at the top, then every column smeared down to the bottom. It also runs at 15 fps / 64.6 ms on the A18 Pro. Goals: root-cause and fix both, and add a regression test. Rules: shared tree, minimal edits, no commits, never compile on TS Neo.

## Root cause (evidence below)
- The bridge picks the preview downscale n from the driver's screen (live.rs ~1757: `preview::downscale(p.area, (input.viewport.0 - rect[2], input.viewport.1))`). On the Mac input.viewport = plan.screen = 3274x2048 (Mac viewer log: "driver's screen 3274x2048", canvas 1629x952 x2, menus 119/176, world 3/2).
- The viewer fits that world into a much smaller pane of its canvas (layout.rs frame_beside / fit_world). When the driver already downscaled (shown>1), fit_world does not downscale further (labels are laid out for n), so the zoom drops to about 1/9.
- The view's zoom-1 picture height (Camera::source_size = viewport*d/n) is then about 8.5k rows. source_size capped it at MAX_SOURCE=4096. Reads past the picture clamp to the last row: Metal clamps (smear); AMD returned zero/white. The map is letterboxed in the middle, so only its top lies above row 4096: hence the correct strip, then the smear.
- The cap also hit the cell range (frame_params used source_size) and the at-zoom scissor (placements used group.source_size()).
- Lag: the oversized 2304x4096 picture was drawn and blitted on every full redraw. On the A18 Pro, a synthetic benchmark (user geometry: 72x140 cells, 12 layers, downscale 4, 254x952 pane at k=2) took 28 ms wall per frame old vs 11 ms new; the old blit alone was ~7 ms GPU. The real scene has more layers and labels, which plausibly gives 64 ms.

## DONE (all uncommitted)
- crates/tile-render/src/camera.rs:
  - New `Camera::world_size()` (uncapped).
  - `source_size()` = world_size capped at MAX_SOURCE (meaning unchanged for the viewer's callers).
  - New `drawn_at_zoom()`: cells && scaled && (zoom <= 1/2 || world_size > MAX_SOURCE).
  - New `picture_zoom()`.
  - `source_texel` returns (0,0) for drawn_at_zoom views.
  - frame_params uses world_size. In placements, the scissor uses group.world_size(), drawn_at_zoom views get scaled_from=Some(0), and their picture is allocated 1x1 (one background pixel).
- crates/tile-render/src/renderer.rs: the blit OutputParams zoom uses `group.picture_zoom()`, i.e. (u32::MAX,1) for drawn_at_zoom views, so it copies the 1x1 background everywhere.
- crates/tile-render/tests/fitted_preview.rs (new), 2 tests. Each compares the GPU against the CPU reference drawn at zoom 1 (uncapped), nearest-sampled by hand:
  - fitted preview at 1/12, downscale 2, at pixel scale 1 and 2 (boxed), plus an inset;
  - a 4/5 view taller than 4096.
- The fitted-preview test FAILS on the pre-fix ef68 code (rows 342..399 differ; 4096/12 = 341.3) and passes now.
- Test results: tile-render all suites pass (gpu_matches_reference 38, per_view_layers, text tests, fitted_preview 2). tile-viewer: 371 passed, plus 3 --ignored GPU tests passed.

## IN PROGRESS
Nothing. The tree compiled and tested at handover. Build dirs and the Mac bench dir have been removed.

## NOT done / next steps
- Deploy: VIEWER ONLY for the Mac (rendering happens in the native viewer; protocol unchanged). The bridge DLL links tile-render too, but needs no redeploy for this fix.
- Not verified live on the Mac (no TILE_VIEWER_PROFILE run on the Load Scenario screen). Next: run with TILE_VIEWER_PROFILE=1 and check "frames drawn whole" vs "frames drawn on the last picture". Static previews should reuse the picture; if they redraw every frame, find what bumps generation or cameras.
- Optional better fix upstream: the bridge should size the preview downscale for the viewer's actual pane, not the driver screen (live.rs, owned by another agent), or the viewer could report the pane. Not done.
- The viewer still uses Camera::source_size (capped) as a view's world extent in layout.rs, pan.rs and world_rescale.rs. That's slightly wrong for views over 4096 world px; left alone.

## Evidence
- Mac viewer log: /Users/sirati/civ3r-tsneo-20261019/runtime/logs/session-1792447875184502000-47149.viewer.log
  - "a 1629x952 picture shown 2x2 ... driver's screen 3274x2048; menus 119/176"
  - frames 76-155 ms
  - beside areas such as [-64,-64,9152,4384]
- Screenshot analysis: in picture rows the map is about 2284x992, matching the 9152x4384 area at n=4 in a ~254-px pane at zoom ~1/9.
- Headless repro (Napoleonic, N=2, 1300x922 canvas, k=2): the old code cut the map at 0.52 of the height; the new code shows it whole and identical at k=1 and k=2.
- AMD timing (narrow pane): old 7.0 ms vs new 4.7 ms GPU. Wide pane (picture fits): at-zoom per-pixel cost is about 2x, which is why the rule only applies at zoom <= 1/2.

## Gotchas
- The CPU reference shares `placements` and source_size, so GPU-vs-reference tests smeared identically. The regression test needs the independent zoom-1 reference plus manual sampling.
- When the bridge's n=1, the viewer chooses its own downscale (no problem). The bug needs bridge n>1 and a narrow pane.
- Never compile on TS Neo. Cross-compile on papa: rust = cross-toolchain.nix `.rust` (/nix/store/2lnh...-rust-default-1.99.1 with aarch64-apple-darwin), linker = darwin-cross-clang `aarch64-apple-darwin-cc`, run inside `nix develop` (build scripts need a host cc), TILE_RENDER_PRECOMPILED_SPV=<built spv>.
- tile-viewer tests can fail to compile mid-edit by other agents (e.g. snap.rs SliderEdit.resync); retry later.

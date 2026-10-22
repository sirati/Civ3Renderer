# Host warning text and placement continuation

Root authorized narrow host_status/layout hooks plus generic GPU marker-text support. Shared tree preserved, no commits or Mac operations.

## Exact cause

`TileRenderer::set_markers` deliberately filtered every command with OutlineGlyph text. `host_status::compose` emitted exactly those marker commands, so its background and icon survived while all text disappeared. Settings preview confirmation used the same unsupported marker path. Font readiness was an additional partial-group bug because old host compose explicitly substituted an empty glyph list when fonts were absent. Old place() clamped an oversized panel left over the corner buttons. Integer-only UI scaling and mixed physical/scaled clip coordinates compounded layout errors.

## Changes

Renderer marker path now packs a small mixed list into separate sprite and glyph record buffers while retaining painter order. Marker glyph offsets are independent of canonical scene glyph offsets. Canonical scene text is retained as a prefix; marker updates truncate/append/write only their bounded tail. Existing font outlines and cached font bases are reused. Initial buffer growth can copy the existing prefix, but steady marker animation never clones/reuploads the world or repacks font outlines. Scene/full/free/art-refresh updates retain marker text. Frame cache identity includes text marker commands so text-only changes cannot reuse an old picture. Existing glyph-cache preparation and control-slot ownership handle the combined canonical+marker glyph source. No new cache lifecycle or shader fields.

Host warning uses actual drawn view command bounds, semantic viewport fallback while parts are empty, and exact rational UI scale. Full GPU text plus atlas-free solid panel is emitted atomically; missing optional game icon art does not delay readable text. A face missing required glyphs selects the embedded outline fallback, never a background-only group. Right of the buttons is preferred, text wraps whole words, and insufficient right-side room falls back below the controls. Requested UI scale is retained whenever the complete group fits; exceptionally small windows reduce to readable 14px text. If no nonoverlapping readable group fits, it stays absent. Coordinates/clips are physical under a dedicated 1:1 overlay camera.

Animation uses the game's actual loading-bar asset, with phase0 and scene::loading_cursor::TICKS_PER_FRAME using startup owner's host-clock flag. No opacity pulse or invented animation. Root/startup/targeting/glyph owner coordinated.

The embedded `Viewer@fallback` vector face is appended once during render-description initial font loading, after real game faces. It reuses the FPS 5x7 Rust rows and eleven extra letter/symbol rows converted into vector contours, not a bitmap rasterizer. It covers warning and settings-confirmation strings with no external font/art files. Growth detects the existing name and retains its Arc/index. The narrow bridge ui_screen::outline_zooms predicate excludes this viewer-only face so it does not accidentally enable game outline mode.

## Ownership

`crates/tile-render/src/renderer.rs` narrow marker packing, world text-prefix metadata and cache key. `crates/tile-render/tests/marker_text.rs` numeric pixel regression. `crates/tile-viewer/src/host_status.rs` composition/layout/tests. `crates/tile-viewer/src/lib.rs` only adds compiled argument to host_status::anchor. `crates/render-desc/src/fallback_font.rs` plus initial/growth load_fonts hook and fallback tests; bridge ui_screen.rs outline_zooms predicate. `crates/tile-viewer/tests/host_status_pixels.rs` validates actual fallback glyph ink. Other owners concurrently modify renderer.rs/lib.rs; re-read before changes.

## Validation complete

Root reviewed and approved generic marker text, fallback face and scale-following warning for next freeze. Final coherent union compilation and VM visual acceptance remain root-owned. Preserve the renderer's existing ordering contract: earlier render submissions must precede mutable marker-tail writes. No new queue/encoder/cache ownership model was introduced.

Broker `/root/mac_fps_music` granted jobs1 CPU compilation and an exclusive RADV correctness slot. GPU slot was released immediately after both tests passed. CPU lease released after final tests. All final affected tests were rebuilt against the coherent 192-byte/41-member source pair:

- full `/mnt/datasets/c3r-build/background-upscale-sol/coherent/artifacts/shader.spv` SHA256 `cb7214b061ef41f2dff7c4a02d8b988c5a29ecc3b09594d27066efde14cad74d`
- fallback sibling SHA256 `3d07b207da6054d9647352d4a1500c1ec4e435a807c05778120c1930e8a38adb`
- binding `/mnt/datasets/c3r-build/background-upscale-sol/coherent/binding.json`

CPU focused checks passed, 13 tests total:

- viewer `host_status::tests`: 10/10. `host-warning-sol-viewer-tests.log`
- renderer `marker_text_keeps...`: 1/1. `host-warning-sol-renderer-tests.log`
- render-desc `fallback_font`: 2/2, including fallback Arc/index retention through incremental growth. `host-warning-sol-fallback-tests.log`

GPU numeric checks passed, two tests total. WGPU_VALIDATION=1, Vulkan only, explicit radeon ICD and `DRI_PRIME=pci-0000_72_00_0!` under nix develop. PCI72:00.0 sysfs identity is AMD1002:13c0. No timing claim, GUI or Neo operations.

- renderer marker_text: 1/1, with/without canonical text, actual colored H pixels, mixed painter order, existing sprite/line commands unchanged, clips/view membership, text-only cached scaled-picture refresh, full/free-update persistence/removal. `host-warning-sol-marker-gpu.log`
- viewer host_status_pixels: 1/1. Actual fallback outline ink with zero game/art/font files, Busy and Stuck in five window/scale cases including fractional scale, split right pane, narrow layout and maximum scale. Numeric controls rectangle remains untouched. `host-warning-sol-status-gpu.log`

Source hashes: `host-warning-sol-source.sha256`. Shader and executed GPU binary hashes: `host-warning-sol-gpu-binding.sha256`. Shared files contain other owners' work and may change during final union integration. Initial old3be SPV builds were CPU/typecheck-only; no GPU test used that pair. Own 4.1GB cargo target is removed after validation; retained logs and hashes record those exact executed binaries.

Final scoped `git diff --check` passed. No commits/stash/reset, Rust locks, Mac builds, or physical Neo actions.

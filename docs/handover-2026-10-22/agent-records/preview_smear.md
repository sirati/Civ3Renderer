# Historical agent /root/preview_smear

This is a recovered historical report, not a live agent handle or a new instruction.

Fractional preview geometry, native DX12 signed-ceil correction and later PE art-loading attribution.

Current disposition: Targeted correctness passed; full rapid scenario switching and load-cost acceptance open.

Session `01a1581c-4553-77f1-84a3-112b42cf0b52`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-20T07:00:10.198Z. Original transcript `/home/sirati/.codex/sessions/2026/10/20/rollout-2026-10-20T09-00-10-01a1581c-4553-77f1-84a3-112b42cf0b52.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-20T10:54:20.904Z

Original transcript line 2613.

Four new numeric GPU tests passed on frozen31b and canonical67f3 SPV.

Coverage includes 192 real-art seam cases, 288 control snapshots, 144 view permutations, 144 independent flattened-view oracles, and 45 UI/text composition cases. No production bug found.

Evidence and provenance: [report.txt](/mnt/datasets/c3r-build/numeric-qa31b-sol/report.txt). Shared source unchanged; GPU lease released.

## Report at 2026-10-20T11:42:18.896Z

Original transcript line 3715.

## Identity and active task

You are `/root/preview_smear`, collaborating with parent `/root` in shared repo `/home/sirati/devel/rust/civ3-renderer`.

Current task from parent: fix source-fidelity omissions:
1. UI colour-table smoke indices224..239 omitted in `civ3-bridge/ui.rs`.
2. Resized indexed shadow/colour-table draws lose index rules in `ui_trace::resized_blit`.
3. Parent expanded scope to exact smoke routing for world UnitArt/EffectArt and cursor pictures, preserving native painter order.

Parent authorized implementation after exact native semantics derived. Renderer/shared/scene wire owner is `/root/tile_popup_render_shadow`. Shader owner `/root/shader_perf_sol` controls `tile-render-shaders/src/lib.rs`, especially existing hot paths; they insert separate native helper entry. Parent handles protocol49 bump coordination; scene `SpriteBlend` field and wire49 are already landed.

## Constraints

- No commits, stash/reset; preserve shared edits.
- Rust shipping code only. No CPU production raster, new locks, or unsafe outside existing interop.
- Build on papa only, `nix develop`, `CARGO_INCREMENTAL=0`, own target under datasets.
- AMD RADV only for GPU tests.
- Coordinate exclusive GPU use, never interfere with FPS collection.
- User explicitly permits Sol agents UI/screenshots, superseding old inherited prohibition. VM owner has sole GUI/input lease.
- Waiting skill: do not progress-poll. Use long waits when otherwise done.
- Always unslop writing; skill read previously.
- Do not use `rm -rf`: automatic tool review rejects rm-f style cleanup. Earlier own target remains.

## GPU coordination right now

Latest `/root/mac_fps_music`:
> No timed collection active. Numeric sequence: shader owner Tiles555 suites, then text_fit_atomic_ui independent cache oracle, then your short smoke integration tests when compiled. Hold GPU until cache owner explicitly releases. Compile is fine during correctness phase.

Need coordinate `/root/text_fit_atomic_ui` for release before running your ignored GPU tests.

## Exact native proof and artifacts

Artifact directory `/mnt/datasets/c3r-build/smoke-resized-sol/`.

Read-only native table capture done by `/root/targeting_cursors` via their parent GDB during owned Wine runtime:
- Linux gamePID144722.
- `[0x9c7304] = 0x0c2a0020`.
- Captured all31×32768×2 bytes:
  `/mnt/datasets/c3r-build/smoke-resized-sol/native-shade-smoke-tables.bin`
- No inferior calls/writes. Targeting owns GDB log with commands; ask them for durable log path.

Independent Rust checker:
- `check-native-tables.rs`
- compiled `check-native-tables`
- `native-oracle-check.log`

**All1,015,808 actual native RGB555 table entries verified**:
-491,520 shade entries.
-524,288 smoke entries.
-Exact rule for smoke source RGB8 `v`, alpha byte `a` from original ExplosionPalette entry index:
  `q5' = (v*a + 8*q5*(256-a)) / 2048`, integer floor, each channel.
-Indices240..254 shade `q5*(idx-239)/16`.
-Smoke224+j selects native table level15+j.
-ExplosionPalette non-blue filler entries in palette order:
  indices `[51,76,77,102,127,128,153,178,179,180,181,204,228,229,230,231]`,
  greys `[150,160,120,150,180,115,150,200,125,100,75,150,210,125,88,50]`.
-Filler RGB `[0,0,255]`.
-Source PCX read in place:
  `/mnt/datasets/winvm/wine-civ3-wow64/drive_c/Games/Civilization III Complete/Art/ExplosionPalette.pcx`.
-No assets copied.

Evidence:
- `native-sources.sha256`
- `native-tables-unmodded.disasm` from `Civ3Conquests-Unmodded.exe`0x6283A0.
- `native-colored-wrapper.disasm`0x5F87E0.
- `native-colored-jgl.disasm`0x10015BF1.
- `native-colored-jgl-setup.disasm`.
- `jgl-pe.txt`.
`Civ3Conquests-Original.exe` is encrypted/garbage at fixed address; use **Unmodded.exe** for actual disassembly.

Native second canvas:
- `ui_hooks` documents first canvas background/read-only, canvas2 output.
-Actual JGL loop writes `[ebx]`, if destination equals0x7c1f reads fallback `[edi]`, otherwise reads current destination.
-UiModel key clear drops magenta fill, so flattened correct backing+local commands give same input. Added regression for key-cleared overlay over backing.
-**Do not claim arbitrary unrelated native backup canvas reconstructed**. Parent was told conditional semantics and exact loop evidence.

A standalone Windows OpenProcess probe `read-native-tables.rs` exists but linking failed missing libpthread.a; unused. GDB capture is authoritative.

## Current shipping edits owned by you

### `crates/civ3-assets/src/loader.rs`

Added generic loader option:
```toml
palette_alpha = { source = "Art/ExplosionPalette.pcx", first =224, count =16, skip =[0,0,255] }
```
-`Civ3Loader::options_rgba` reads actual PCX palette, selects first count non-filler entries, maps RGB8 plus **original entry index as alpha byte**.
-No quantization of explicit values.
-Validates descriptor ranges, byte channels, conflicting rgba overrides.
-Missing source returns error, no transparent fallback.
-`source_key` includes resolved palette_alpha source.
-`palette_alpha_colors` pure helper.
-New CPU tests preserve non-filler order/raw channels/index alpha and missing-source failure.

### `crates/menu/src/screen.rs`

Added:
```rust
pub struct Indices {
  pub resize: Option<IndexedResize>,
  pub palette_alpha: Option<PaletteAlpha>,
  pub transparent: Vec<u8>,
  pub shade: Vec<(u8,u8)>,
}
pub struct PaletteAlpha { source:String, first:u8, count:u8, skip:[u8;3] }
pub struct IndexedResize { size:(u32,u32), sample_step:[u32;2] }
```
-`Indices::size(rect)` returns destination size.
-options emits generic palette_alpha.
-key includes resize and palette identity.
-Indexed sheet description emits `resize`/`resize_step`.
-Readiness/empty check use destination extent.
-Scene free sprite gets `scene::SpriteBlend::PaletteAlpha256` for palette_alpha.
-Preload `indexed` metadata line accepts old7 fields or new10 fields, new trailing resize/alpha/source fields. **Initial adjacent indexed_rules design was replaced** because it could erase a genuine unresized entry when both versions coexist. Current one-line schema is atomic.
-CPU test `resized_palette_alpha_rules_round_trip_and_keep_native_samples_and_typed_mode`: old/new metadata, same original+resized region coexist, exact samples `[8,9,11,13,15]`, typed scene mode, shade16.
-Added **new variant** `Element::PaletteAlphaSprite { name,at,clip }` for already-compiled smoke-only FLC sprites. Menu matches and ready/art/sprite generation support it.
-Updated `crates/menu/src/lib.rs` explicit `FreeSprite` literal with `blend: Alpha`.

### `crates/civ3-bridge/src/ui.rs`

-`Event::ResizedBlit` now includes `indices:Option<menu::screen::Indices>`.
-Model builds `Source::Indexed` for rules.
-`screen_draw` supports `Op::ResizedImage` with Indexed source, retains whole original source and destination sample phase, puts size/step in Indices.resize.
-Indexed visibility uses destination size.
-`indices::colored` still handles ordinary indices<224, now documents separate smoke.
-New `indices::colored_smoke()` transparent all, palette_alpha actual ExplosionPalette.
-New tests:
  -`resized_index_rules_survive_copied_clips_without_restarting_sampling`.
  -`keyed_overlay_smoke_keeps_lower_backing_and_prior_local_draws_in_order`.

### `ui_trace.rs`

-`resized_blit` accepts optional index rules, event preserves them.
-Palette uses `palette_from(...).or_else(copied_civ_colors)`.
-Known scaled blend tables now use resized indexed path rather than ordinary sprite fallback.
-Scaled coloured **file sprites** emit ordinary/shade/smoke splits with same native scale geometry.
-Unscaled coloured file sprites emit smoke third pass.
-Original hook guards retained.
-Army owner also editing small city eligibility parts of this file. Do not overwrite whole file.

**Open scope finding just sent parent:** scaled coloured FLC sprites still are not covered by file-source resized capture because model has no `Source::File` for them. Existing unscaled FLC route now uses split UnitSmoke named art. Do not claim resized-FLC fixed. Last message to root:
> Identified adjacent preexisting unsupported case: scaled_colored FLC sprites are not Source::File, so resized capture drops them. Current file-index fixes do not prove this case. It requires explicit FLC frame selection/source-crop metadata and native palette provenance; I have not guessed crop or colours. Keeping that distinct from validated file rule path while finishing requested gates.
Parent has not responded yet. If addressing, derive native crop/palette first; avoid blindly broadening unscaled FLC placement, which had prior native evidence.

### Trace/bounds

-`trace.rs` Indexed draw writes `ui_resized_image` when resize exists, then existing indices line, then optional `ui_image_palette_alpha first count r g b path`.
-Removed erroneous `?` inside format_trace returning String after Army owner flagged it.
-`world.rs` parser handles indexed rules on ResizedImage, new palette_alpha metadata, old defaults preserved.
-New `world::navigator_trace_tests::resized_indexed_and_smoke_trace_metadata_preserve_native_rules_and_old_defaults`.
-`interface_parts.rs`, `city_screen.rs`, `ui.rs` bounds use resized indexed destination extent.
-Army owner owns other `city_screen.rs` changes, knows your bounds arm.

### World UnitArt/EffectArt splitting (recent, not fully validated)

`units.rs`:
-Ordinary body transparent224..255.
-Added `UnitSmoke@path/chunk`, `UnitSmokeAnim@path/block` image/set generation, generic palette_alpha loader; separate from owner recolours, raw index pixels should share atlas texels.
-Removed old lossy `smoke_rgba` string builder.
-`Needed.smoke` retained for request identity but real art always loads actual palette source; updated docs.
-Byte estimate includes extra smoke image.
-`Piece` gained `pub smoke:Option<String>`, defaultNone; `animation` fills for coloured art.
-New `smoke_set` checks `SpriteDesc.blank & tile_render::BLANK_EMPTY ==0`.
-New `smoke_free_sprite` emits typed native free command, palette0, category units.
-`free_sprites` inserts smoke immediately after each ordinary piece.
-`place` promotes first unit with nonempty smoke and all later units into ordered free suffix. It also honors slots().None. **slots() already verifies native monotonic row/descending-column/slot order and makes all units after violation None**, so this preserves actual tile-layer ordering, not arbitrary vector assumption. Earlier ordinary prefix remains layered.
-Added `cursor_smoke_sprite` for go-to marker.
-`targeting.rs` emits typed smoke twin after ordinary go-to cursor sprite.
-`ui_screen.rs` unscaled FLC picture emits shadow, ordinary UnitArt, then `PaletteAlphaSprite UnitSmoke`.
-`preview.rs::art_ready` now requires UnitSmoke and EffectSmoke groups. Updated optional preview fixture adding UnitSmoke sheet.

`effects.rs`:
-Added EffectSmoke image generation in units description.
-`place` emits Shadow, Art, Smoke in same order and same effect pass.
-Skips smoke if BLANK_EMPTY.
-Explicit blend Alpha for ordinary parts, native for smoke.
-Updated actual effect CPU test: body224 nowtransparent, smoke palette alpha actual; ordinary art/shadow reference check excludes native mode, native smoke covered GPU tables/end-to-end tests. Dynamic ordering count checks optional nonempty under-effect smoke.
-Parent/Army previously saw effects FreeSprite missingblend from snapshots. **Current source has field.**

`tile-render/src/lib.rs` single-line public reexport `BLANK_EMPTY` added by you with renderer owner permission.

### BLANK_EMPTY owner work

`tile_popup_render_shadow` added BLANK_EMPTY bit23 in shared/atlas metadata, unchanged SpriteDesc wire layout:
-Computed in existing full-alpha scan, no extra scan.
-400×300 allclear metadata regression passed.
-Nonzero/clear patch transitions, own palette alpha invalidation, RGB-only preserve tested.
-Native renderer CPU bounds early-skips proof, no snapshots/pipelines for empty native sprites.
-Do **not** use box_w/h as smoke-presence marker: ordinary full-cell sampling boxes remain full for transparent frames, and switching smoke to sample_from box would change native downscale phase. This was caught and corrected; your helpers now use BLANK_EMPTY.

One disclosed limitation to parent:
-In-process live compiler makes pixels then retains without_pixels descriptors, so has proof and keeps ordinary layer fast path.
-Remote bridge tables-only compilation lacks pixel proof and conservatively may promote suffix. Remote GPU renderer knows actual BLANK_EMPTY and allocates no empty smoke snapshots. No new feedback/wait introduced.

## New tests added, pending

### `ui_screen::tests::resized_smoke_palette_alpha_matches_native_tables_after_clipped_canvas_copies`
Ignored, requires actual assets + native capture + RADV.
-Fake index image16×32 generated in memory, actual Civ3Loader indexed palette-alpha route loads real Territory + ExplosionPalette.
-Native geometry via scaled_geometry `(2,3),(0,0,16,32),[11,17,32]` =>5×17.
-Partial copy retains source phase: resulting draw at8,5 clip10,5..13,22.
-Background rows vary all RGB555 channels.
-Builds model→items→ScreenArt description→actual loader→scene typed command→GPU.
-Expected pixels directly read **captured native tables**, not same formula.
-Env:
```
CIV3_INSTALL=/mnt/datasets/winvm/wine-civ3-wow64/drive_c/Games/Civilization III Complete
CIV3_NATIVE_SMOKE_TABLES=/mnt/datasets/c3r-build/smoke-resized-sol/native-shade-smoke-tables.bin
```
Run exact test with `--ignored --nocapture --test-threads=1`.

### `units::tests::empty_smoke_frames_larger_than_saturated_margins_emit_no_native_commands`
CPU,400×300 transparent generated solid, verifies unchanged full source/sample box, BLANK_EMPTY survives without_pixels and no native command.

### `units::tests::overlapping_smoke_preserves_layer_prefix_body_cursor_banner_and_independent_view_order`
Ignored, actual assets + exclusive RADV.
-Newest test added just before compaction, may have compile/runtime defects to resolve.
-Actual world ordinary warrior prefix, Artillery attack smoke, army banner/member, neighboring-row late warrior.
-Scans actual Artillery attack frames to choose nonempty smoke.
-Aligns real smoke texel and late body's opaque texel to force overlap.
-Tests selectionsNone/artillery/army, movement deltas, two native view bounds, downscale1/2, two independent cameras, B zoom3/2.
-Compares layered-prefix+native-free suffix GPU against independent **all-free native vector oracle**.
-Misorders first smoke to end and requires image difference, avoiding vacuity.
-Ordinary first unit remains unit_body cell at4,4; first smoke artillery cell3,4 staysempty.
-Cameras just changed to x360/y32 and x380/y48 for actual overlap visibility.
-Uses `tile_viewer::scene_world` for cell assertions because Scene has no get.

## Existing tests updated for new semantic split

-`units_keep_their_colours_on_every_path`: original slots() still expected five layer-capable units, effective slots after first actual smoke forcedNone. Palette iterators filter native smoke commands. Ordinary CPU reference image comparison filters native mode; preserves old colour/painter coverage. New ignored GPU test covers full exact smoke.
-`units_pick_their_colours_hidden_nationality_and_armies_included`: description image count expected4 now, shadow/smoke/body/armybody.
-`effects::effect_art_compiles_and_places` adjusted as above.
-Preview fixture UnitSmoke added.
Need run relevant CPU suites and fix any missed old expected item counts. Avoid broad test filters invoking GPU while lease held:
  -`units_keep...` uses CPU reference only, safe after exclusion.
  -`armies_draw_in_their_owners_colours` **creates GPU**.
  -`four_k_independent_views...` **creates GPU**.
  -`ui_screen` whole module has GPU tests even nonignored.
  -Use exact safe filters, or await GPU lease before full bridge suite.

## Build/test state

Own target `/mnt/datasets/c3r-build/smoke-resized-sol/target`.

Always **source build current shader**, not canonical old67 precompiled:
-Old67 lacks current new text_dual_fs/native entry, build helper rejects it.
-Current build.rs postprocesses dual-source Index decoration automatically; release_deploy confirmed compile-ready.
-Source-build succeeded already with current dual+native entry.
-`text_perf_sol` alternative staging:
  `/mnt/datasets/c3r-build/text-perf-sol/current-dual.spv`
  SHA13f4e0a0865a45e07709a70d1a89a17f4addc7549fd6f3bf0a38e1e2d811fbff
  fallback09d0de4cd03ae4a5073d0f718399109fd0c10c2dff3736274aacaf921bf7e6b5.
  Correctness staging only, not shipping-validated, source checkpoint641.
-Your source-built path:
  `/mnt/datasets/c3r-build/smoke-resized-sol/target/spirv-builder/spirv-unknown-vulkan1.2/release/build/tile-render-shaders/2c4b24212c33b8db/out/tile_render_shaders.spv`
  Hash not yet recorded; shared source still moving.

Completed:
-`menu-loader-tests.log`: assets18 + menu32 passed,50 total, before later PaletteAlphaSprite variant/world split changes.
-`bridge-ui-tests.log`:38 ui::tests passed.
-`resized-tests.log`:8 passed,1 ignored, all resized model/trace/loader legacy tests plus new CPU rule test; latest run source-built shader succeeded.
-Native checker1,015,808 entries exact.

**Currently running / last launched session58551**:
```
nix develop -c env -u TILE_RENDER_PRECOMPILED_SPV CARGO_INCREMENTAL=0 \
 CARGO_TARGET_DIR=/mnt/datasets/c3r-build/smoke-resized-sol/target \
 cargo test -p civ3-bridge --lib smoke -- --test-threads=1 \
 > /mnt/datasets/c3r-build/smoke-resized-sol/smoke-cpu-tests.log 2>&1
```
No GPU unless ignored tests explicitly selected; filter smoke skips two ignored GPU tests. New CPU large-empty test included.
May have compiled just before final camera-only edit; rerun only if changes/failures warrant.

Renderer owner already ran core GPU:
-`/mnt/datasets/c3r-build/smoke-core-sol/generic-gpu.log`
-`native-table-gpu.log`
-4/4 passed, including all524,288 native smoke RGB555 lookup outputs exact.
-Those manually construct RGBA sprites; **your end-to-end loader/resize test is still necessary**.

## Coordination owners

-Root `/root`: overall, protocol49 decision/bump, source freeze.
-`tile_popup_render_shadow`: renderer/shared/atlas BLANK_EMPTY/scene typed wire/viewer packing, core GPU.
-`shader_perf_sol`: shader helper insertion/hotpath performance; no direct shader edits yourself.
-`text_perf_sol`: dual-source text staging, current shader artifact.
-`release_deploy_sol`: build.rs dual-index postprocessor/artifact coordination.
-`army_units`: city_work native eligibility, city_screen and small live/ui_trace parts. Units/effects files explicitly free for you. Notify after stable for their Windows check.
-`targeting_cursors`: read-only native table capture, Wine runtime inputs.
-`vm_snapshot_sol`: sole macOS GUI lease; original game Wine audio startup blocked by mmdevapi issue, audio owner fixing. No live VM native pixels needed for tables anymore.
-`mac_fps_music`: GPU scheduling and performance.

## Immediate next steps

1. Finish/read session58551 result; fix compile/tests.
2. Notify Army when effects/units compile stable; request Windows check of hook signature changes. Current Event::ResizedBlit extra field applied all known literals.
3. Run safe CPU focused old/new tests: menu/assets, ui::tests, navigator_trace_tests, relevant unit description/colour path tests, effects metadata, preview readiness. Avoid GPU cases until lease.
4. Obtain GPU release from cache owner, then run two ignored integration tests above under RADV/env actual capture. Resolve failures; parent specifically requires world overlapping row/column/cursor/banner order and no-smoke fast path.
5. Discuss/handle parent response about **scaled FLC capture gap**. No implementation yet. Exact native crop/frame and palette provenance needed; do not invent. Possible generic future `Indices.frame` + actual captured palette overrides, but not authorized schema/design settled yet.
6. Ensure proper native second-canvas caveat, preserve real table capture/GDB provenance.
7. Collect final source hashes/SPV hash/logs, handover. Do not claim all legacy source-fidelity complete until tests and any scaled-FLC scope resolved.

## Earlier completed task artifacts (not active)

-Renderer3 control slots/submission contract completed and81 renderer tests,410 viewer tests.
-Handover `/mnt/datasets/c3r-build/handover/renderer-sync.sol.md`.
-Old own target3.5GB remains cleanup blocked by automatic rm-f rule.
-Preview/DX12 evidence `/mnt/datasets/c3r-build/handover/preview-smear.sol.md`.

-Numerical frozen31b creative QA completed4 new tests, no production bug found:
  `/mnt/datasets/c3r-build/numeric-qa31b-sol/report.txt`
  Four tests cover192 actual terrain/fog/territory seam cases,288 control frames,144 disjoint view permutations+144 independent flattened views,45 procedural/text equivalences.
  Source immutable31b, canonical67, all631 original files verified; only isolated dev manifest/Cargo lock edge changed.
  GPU released.


## Report at 2026-10-20T11:50:58.252Z

Original transcript line 4155.

Implemented smoke routing, resized index rules, and scaled FLC frame capture with native source geometry.

Validation passed:
- 50 asset/menu tests.
- 38 UI model tests.
- 9 resized/FLC tests.
- Both RADV smoke integration tests, including captured native table pixels and world painter order.

Handover: `/mnt/datasets/c3r-build/smoke-resized-sol/handover.md`

GPU released. Army owner is checking Windows hooks.

## Report at 2026-10-20T13:32:18.167Z

Original transcript line 4375.

Implemented VSync and FPS limiter controls, with a 30–480 FPS slider and saved disabled value.

Fixed an extra refresh delay in uncapped frame scheduling. Explicit FPS caps retain their timing behavior.

All 35 focused CPU tests passed. No GPU or MacNeo work performed.

Handover: `/mnt/datasets/c3r-build/smoke-resized-sol/pacing-handover.md`

## Report at 2026-10-20T13:56:18.285Z

Original transcript line 5073.

Fixed three races that could permanently lose the latest scenario preview, plus worker restart and canceled-art recovery.

Deterministic tests reproduced all three failures before the fix. Final suite passed: 30 tests, 1 intentionally ignored.

Handover: `/mnt/datasets/c3r-build/smoke-resized-sol/preview-lifecycle-handover.md`

CPU released. Native visual validation remains with the VM owner.

## Report at 2026-10-20T14:20:54.921Z

Original transcript line 5255.

Implemented bounded FLC metadata reads. `AnimFile::read` now seeks past pixel payloads instead of loading entire files.

Validation passed:
- 22 asset tests.
- Exact parity across 13 real FLCs and 1,151 frames.
- Unit metadata, anchors and frame selection unchanged.

Read bytes fell from 4,481,758 to 8,570 in the parity sample. Source is stable for freeze; CPU released.

## Report at 2026-10-20T19:01:02.357Z

Original transcript line 5264.

I’ll compare the frozen source and archived art/upload logs to locate where HUD and unit assets stop progressing, and verify whether the renderer build includes the bounded FLC reader.


## Report at 2026-10-20T19:08:44.532Z

Original transcript line 5461.

Both incomplete screenshots precede PE art-table adoption. Healthy image 024 later shows full HUD and units.

The 30–36s delay is inside PE art compilation; GPU uploads had already finished. The bounded FLC fix is present, but body/shadow/smoke variants still repeat metadata scans and filesystem calls.

Report: [read-only findings](/mnt/datasets/c3r-build/handover/preview-smear-missing-hud-readonly.sol.md). Next diagnostic: per-source metadata call counts and art-job stage timings. No source or runtime changes made.

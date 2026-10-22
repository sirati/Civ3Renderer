# Historical agent /root/ui_feature_audit

This is a recovered historical report, not a live agent handle or a new instruction.

Menu scale/fit and texture-only background behavior; scenario preview lifecycle coordination.

Current disposition: Focused evidence and recipe prepared; current UI acceptance matrix open.

Session `01a152c3-ba33-7850-9319-cbe1ebbddcc3`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-19T06:05:21.333Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T08-05-21-01a152c3-ba33-7850-9319-cbe1ebbddcc3.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T12:13:17.658Z

Original transcript line 4869.

## Identity and active assignment

You are `/root/ui_feature_audit`, a subagent of `/root`, in `/home/sirati/devel/rust/civ3-renderer`.

**Current task, just started:** Implement shared menu scaling and `viewer-settings` model/control metadata for the newest user requirements. Camera optimization is complete and must remain untouched.

Root’s exact assignment:

> Own shared menu scaling + viewer-settings model/controls metadata, coordinating assets/preferences.toml with `/root/settings_transaction` (owns draft dialog state + assets panel). Checkbox “Scale menus”. Off uses game/native menu scale, ignoring multiplier; title/main menu and Preferences ALWAYS fit visible canvas even pixel scale x3. On slider is screen-relative 25%–100%; 100% fits standard/main menu top/bottom subject to width/aspect, 25% quarter height. Do not reinterpret as 0.25..8 multiplier. Respect existing UI placement/per-view game screens. Only title + settings must fit when off; do not shrink other dialogs gratuitously. Use semantic scene screen/bounds and actual content/reference height, not text heuristics. Persist boolean + percent, default/migration explicit. Draft scaling does not apply until Accept (settings owner handles draft). Add pixel/input/Alt tests for on/off, x1/x3, title/settings, scenario Beside, nonsquare windows. No physical UI or source freeze until sibling work settles.

No menu-scaling source changes have been made yet. Only reads and coordination messages.

## Constraints and coordination

- Autonomous, no user questions.
- Never touch Nexus/NixOS checkouts.
- No physical Mac GUI, screenshots, or binary/session changes.
- QA sole GUI owner: `/root/pointer_vm_qa`.
- Root controls immutable source freeze and native/Windows builds.
- Shared/uncommitted checkout has many owners. Edit only assigned files/hunks.
- Skills already read: boss, waiting, unslop, you-are-on-nixos.
- On NixOS, use `nix develop -c ...`; plain `python3` is unavailable.
- Batch independent reads with `functions.exec` + `Promise.allSettled`.
- Avoid repeatedly polling agents/background commands.
- Root wants evidence and exact changed source hashes/test names when settled.
- Root siblings currently:
  - `/root/settings_transaction`: draft/apply/Cancel confirmation, `assets/civ3/preferences.toml`, prefs labels/state/live integration.
  - `/root/macos_icon`: app/Dock icon; edits only narrow `tile-viewer/src/lib.rs` bootstrap fields/hunks plus Cargo dependency. Preserve them.
  - `/root/ui_background_dither`: shader/render-desc background-only downsample dither; no layout/scaling/settings edits.
  - A bridge owner investigates actual game→title crash.
- Do not acknowledge messages merely formally. Send substantive findings/coordination only.

## Menu model decisions already sent to settings owner

To `/root/settings_transaction`:

- New persisted/control fields:
  - `resolution.scale_menus: bool`
  - `resolution.menu_scale_percent: f32`
- Percent control range **25..100, step 1**.
- Defaults **false / 100**.
- Legacy `resolution.menu_scale` multiplier will be removed from editable controls.
- Legacy TOML multiplier is consumed/dropped during migration; no mapping of 1.5 to 150% or 0.5 to 50%.
- Old file without new fields loads false/100.
- New serialization writes boolean + percent, not old multiplier.
- Settings owner will add checkbox + percent slider in their owned asset, update labels and prefs tests. Active/draft remains theirs.

Settings owner replied:

> Will add Scale menus checkbox and 25..100% menu_scale_percent slider to owned preferences asset and labels. Draft remains isolated through Accept. Will adapt slider mapping test to checkbox+percent.

To dither owner:

> Plan to keep per-sprite UI transforms and crisp text, no new composed-UI downsample path. Plan/menu ratio will be selected from semantic menu bounds; overlay cameras continue current per-sprite scaling.

## Current menu code and findings

### Viewer settings

`crates/viewer-settings/src/lib.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Resolution {
    pub pixel_scale: u32,
    pub menu_scale: f32,   // current old multiplier
    pub frame_scale: f32,
    pub world_scale: f32,
}
impl Default {
    Self { pixel_scale: AUTO, menu_scale: 1.0, frame_scale: 1.0, world_scale: 1.0 }
}
```

Need replace old field with boolean + percent, explicit migration handling/tests. Consider custom Deserialize using a helper saved struct which accepts old `menu_scale` but ignores it. Generic unknown fields are currently ignored, but root requested explicit migration. Keep serialization only new fields. Percent values should be finite/clamped to 25..100.

`crates/viewer-settings/src/controls.rs`:

- `SCALE_RANGE = 0.5..3.0, step .05` currently shared menu/frame/world.
- `KEYS` length 19 includes `"resolution.menu_scale"`.
- `range`, `Settings::control`, `set_control` all handle old field.
- New bool key + percent key means KEYS length 20.
- Add separate `MENU_SCALE_PERCENT_RANGE` 25..100 step1.
- Existing tests around lines 329, 384 expect old menu multiplier controls; update.
- `viewer-settings/src/lib.rs` test line ~516 uses old TOML `menu_scale=1.5` only to assert default pixel Auto; extend explicit migration tests.

### Scaling

`crates/tile-viewer/src/scaling.rs`:

- `Plan` has:
  - `present: Present`
  - `screen: (u32,u32)`
  - `menu: Affine`
  - `frame: Ratio`
  - `world: Ratio`
- `plan(window, desktop, screen, &Resolution)` at ~175 currently:
  ```rust
  let menu = Affine {
      from: center(screen), to: center(canvas),
      zoom: ratio(settings.menu_scale),
  };
  ```
- `ratio` clamps multiplier .25..8. Keep for frame/world and internal arbitrary affine ratios; **do not use new percent as this multiplier**.
- `fit(inner,outer)` returns aspect-fit rational.
- `arrange_panned` ~342:
  - handles `Frame::beside_screen` specially first.
  - normal named `frame.placed` bypasses interface scaling.
  - computes `menu_rects` from interface-only `view.menu` overlay sprite bounds, `menu_box`.
  - menu group centered/kept within canvas or `frame.menu_region`; oversized ordinary menus pan at requested size.
  - `screen_piece` handles popup/screen_view about their own bounds and opening region.
  - menu content bounds and dedicated screen camera bounds are available here.
- Existing behavior keeps generic dialogs large/pannable. Must preserve this when checkbox off except mandatory title/Preferences fit.
- Need work out semantic metadata for **title versus setup** and **Preferences versus generic `"Menu"`**. Do not infer from text.

### Semantic scene state

`crates/scene/src/lib.rs`:

- `Scene.shape` empty outside actual game.
- `Interface` currently has:
  `screen`, `roaming`, `capture_hint`, `popup`, `screen_view`, `focus_control`, `reachable`, `beside`.
- No explicit title/Preferences fit flag/reference-size metadata yet.
- `scene::wire` manually encodes Interface, so adding fields needs codec + version + tests and root/transport coordination.
- `viewer-transport` version 1 has scene_version handshake, uses scene wire bytes. Root owns exact deployment pair.
- Possible design under consideration, **not decided/sent yet**:
  - semantic `Interface` metadata for mandatory-fit title/Preferences and standard menu reference size.
  - `menu_reference` size e.g. standard 1024×768.
  - `fit_menu` bool.
  - actual content bounds can be computed in scaling from dedicated screen camera/menu sprite union.
  - Percent should scale standard UI relative to screen height, not make tiny context popup fill entire screen. Use reference height, actual content width to cap as appropriate.
- Need notify root/settings owner if adding scene metadata and delegate/coordinate producer changes to avoid competing `live.rs` edits.

### Producer semantic signals discovered

`crates/civ3-bridge/src/interface_parts.rs`:

```rust
pub fn active_screen(frame, build) -> Option<(usize,&'static str)> {
    // Preferences vtable matches -> returns (i, "Menu")
    // otherwise known screen_forms names.
}
```

Thus string `"Menu"` cannot uniquely identify Preferences. Need a specific semantic signal.

`crates/civ3-sys/src/controls/setup.rs`:

```rust
pub fn title_layout(mem, build) -> Option<TitleLayout> {
    if build != PcGames { return None; }
    // Nested New Game/Preferences/etc dialog blocks title:
    if mem.u8(0xB4_25CA)? == 0 || mem.u32(0xCA_D384)? != 0 {
        return None;
    }
    ...
}
```

This is a **real semantic title-only signal**. Existing live publisher already calls it around lines ~859 for title controls.

`crates/civ3-bridge/src/prefs.rs`:

- `Preferences::open()` / `screen_area` / `shows_screen` uses actual `PreferencesForm` vtable and canvas.
- Settings owner edits this module, can supply semantic fit metadata from actual Preferences-open state.

`live.rs`:
- `Shown::Map`, `Shown::Title`, `Shown::Preview`.
- `Shown::Title` covers all nongame states, including setup, so alone not title-only.
- Final `scene.interface = Interface { screen: screen_open..., ... }` around ~1578.
- Settings owner owns live settings integration. Root crash owner may also have nearby changes.

### Existing tests and migration workload

`rg -n menu_scale ...` found ~54 references. Mostly:
- `scaling.rs` test helper:
  ```rust
  fn res(pixel_scale, menu:f32, frame,world) -> Resolution {
      Resolution { pixel_scale, menu_scale:menu, frame_scale:frame, world_scale:world }
  }
  ```
- Many existing affine/render mechanism tests hardcode arbitrary multiplier 1.5/3.0 and expected positions.
- Preserve these tests as **explicit fixed-affine mechanism tests**, rather than falsely treating 150 as valid user percent. Suggested easy adaptation:
  - replace test helper `res` with `fixed_plan(window,desktop,screen,(pixel,menuMultiplier,frame,world))` which uses production plan for pixel/frame/world with new defaults, then explicitly sets `p.menu.zoom=ratio(menuMultiplier)`.
  - mechanically replace single-line `plan(...,&res(...))` calls with this explicit helper.
  - New policy tests must call production `plan`/semantic arrangement directly.
  - Or update existing tests properly; no cfg(test) special behavior in production functions.
- `layout.rs` tests have 5 `Resolution { menu_scale: ... }` literals (~5351,5432,5488,5584,5661) used to exercise affine placement. Replace with explicit plan overrides if preserving mechanism test.
- `snap/tests.rs` uses `l.menu_scale=plan.menu.zoom` in preview test; this is **Layout’s final ratio**, not persisted old config field. Keep this field concept or rename carefully.
- `Layout.menu_scale: (u32,u32)` is runtime final ratio used by `frame_beside`.
- `lib.rs` sets `self.layout.menu_scale=plan.menu.zoom` around ~2747.
- To derive ratio from actual menu content, likely plan needs store menu policy and arrange computes actual ratio; must also set correct ratio **before** `layout.frame_beside`. Could add helper `plan.for_scene(scene, content bounds)` or compute plan’s menu policy from semantic source/reference before layout, using `named_screen_content` and menu bounds.
- `Frame` already carries `BesideScreen` with final mapping; preserve single-transform invariant.

## Completed earlier work, preserve it

### Camera candidate clamp, just settled

Changed only:

```
crates/tile-render/src/camera.rs
  SHA256 cbc422d0284f4704b1de9905ae3281ae64013c7f2850417ff65416491b80edc7
crates/tile-render/tests/gpu_matches_reference.rs
  SHA256 b1d226514986081fbb15551acc8f31c6fb97578f5d0c81947cc42c3178e090ec
```

Shared `Camera::frame_params` now intersects candidate ranges with grid bounds on **nonwrapping axes only**, after original overhang/offset reach. Wrapped copies unchanged. Empty intersections zero instances.

Why: Rise of Rome preview at 1280×678 with 1024-wide menu:
- World viewport252×678; downscale32, zoom21/23; source276×743, camera6,-300.
- Grid70×142, overhang128 unchanged.
- Old candidates135×1001 =135,135/layer, 10,270,260 instances for76 layers.
- New70×142=9,940/layer,755,440 instances. 13.6× reduction.
- Blind overhang division rejected because `SHRINK_KEEP_ANCHOR` forests/districts retain offsets.
- Readback was only3,471,360 bytes, below64MiB. Timeout30s unchanged.
- Exact Rise of Rome VM rerun still pending; small Mesopotamia pass does not prove large timeout resolved.
- No shader/bridge/transport changes for clamp.
- Evidence:
  `/mnt/datasets/c3r-build/macos-port/phase24-camera-candidate-diagnosis.json`
- Tests:
  - `cargo test -p tile-render --lib camera::tests::` 4 pass.
  - Full tile-render lib22 pass.
  - Full tile-viewer lib306 pass,3 existing GPU ignored.
  - New GPU regression passes:
    `nonwrapping_candidate_bounds_preserve_tall_wide_and_fixed_anchor_pixels`
    compares CPU/GPU to independent free-sprite oracle and deliberately padded grid candidates, 3 scaling policies, offset cells, orthogonal/staggered, downscale1/2/32, zoom1 and21/23, empty intersections + UI.
  - New camera unit names:
    `tiny_nonwrapping_preview_bounds_candidates_without_shrinking_sprite_overhang`
    `nonwrapping_empty_intersections_have_zero_cells_and_keep_interface_parameters`
- Native owners have filters, await root freeze. Full GPU suite now55 rather than54.

### Title/setup and async scenario preview fixes, settled phase24

Own viewer files previously changed:
`tile-viewer/src/layout.rs`, `scaling.rs`, `lib.rs`, `pan.rs`, `snap/tests.rs`, `unit_orders.rs`.

- Empty-world title/setup bypass saved A/B/zoom configuration.
- Full-window input/Alt remains mapped through interface scaling.
- Returning title clears capture, pending orders, pan/momentum and gameplay feedback.
- `frame_beside` uses **driver_screen** cameras, not canvas-size cameras.
- `BesideScreen` final source/mapping/region and dedicated `arrange_beside` prevent double-centering disappearing Conquests menu.
- Layout has `menu_scale` final ratio; frame_beside reserves scaled menu size and remaining preview region.
- `arrange_beside` maps entire supplied menu rectangle for input/Alt, supports panning, excludes capture hint transform.
- Ready preview never emits gameplay A/B/zoom feedback.
- Child `/root/ui_feature_audit/scenario_preview_lifecycle` completed bridge fixes:
  `live.rs`, `preview.rs`, `art_schedule.rs`.
  Menu publishes and uploads mandatory art before optional preview whole-atlas work.
  Pending/failed preview empty world and no hint.
  Actual matching scenario art/labels/unit frames/palettes required before world publication.
  Closing/new selection clears/cancels stale preview.
  Whole replacement counts as pending mandatory art, avoiding missing-art flash.
- Evidence prior:
  `/mnt/datasets/c3r-build/macos-port/title-preview-viewer-tests.json`
- That manifest’s source hashes are now historical after sibling changes; do not treat as current freeze.

### Native Wine runtime already complete

On TS Neo private task root:
`/Users/sirati/civ3r-tsneo-20261019`.

Preferred ARM64 Wine+FEX:
```
native-wine/runtime/dist/bin/wine
native-wine/prefix
DYLD_FALLBACK_LIBRARY_PATH=.../native-wine/runtime/dist/ext/lib
FEX_DISKCACHE=1 WINEARCH=win64 WINEDLLOVERRIDES=mscoree,mshtml=
```
Both x86/x64 console smokes and actual Civ III+native Metal viewer startup passed. Provisioned loader untouched/signed. Only public Wine11.18 ntdll/wineserver rebuilt for macOS26.5 fallbackpipe, all38 upstream patches unchanged. Original x86 Wine fallback preserved.

Evidence:
`/mnt/datasets/c3r-build/macos-port/tsneo/native-wine/native-game-result.json`
`runtime-provenance.json`, valid `smoke-summary.json`.
No runtime action needed for current task.

## Immediate next steps

1. Settle menu policy/reference-size + semantic mandatory-fit design. Notify root and coordinate producer semantic metadata with settings owner; avoid `live.rs` conflicts.
2. Implement `viewer-settings` boolean/percent fields, explicit old multiplier migration, control metadata/ranges/tests.
3. Implement shared menu ratio selection:
   - off native1× ratio;
   - title and Preferences always aspect-fit within available visible canvas/assigned region when too large;
   - on uses standard/reference menu height ×25–100% screen-relative ratio, width capped appropriately;
   - ordinary dialogs preserve placement/native behavior off;
   - scenario Beside uses chosen ratio **before** layout reserves its menu region, then maps once.
4. Preserve existing affine mechanism tests through explicit fixed-ratio test helper; add real policy tests including pixel + Alt/input and nonsquare x1/x3.
5. Run appropriate settings/viewer/scene wire tests if changed; communicate exact source files/hashes/tests to root, no source freeze or deployment yourself.
6. No physical UI actions. Root coordinates native rebuild and QA after siblings settle.

## Report at 2026-10-19T12:13:21.258Z

Original transcript line 4879.

I’m continuing from the recovered state and checking the current ownership before changing code, so the platform API work and menu-scaling work stay coordinated.

## Report at 2026-10-19T12:16:04.364Z

Original transcript line 5005.

The recovered menu setting still used a native-size multiplier. I’m replacing it with the requested checkbox and 25–100% screen-relative size, and adding an explicit title/Preferences fit flag so other dialogs keep their existing placement.

## Report at 2026-10-19T12:20:46.141Z

Original transcript line 5120.

The new scaling tests now cover title and Preferences pixels, clicks, and Alt targets at x1 and x3, including wide and portrait windows. The scenario menu also keeps one consistent transform while its preview loads, so rendering and input agree.

## Report at 2026-10-19T12:23:08.005Z

Original transcript line 5204.

Menu sizing now respects the view where the menu opened. The checks cover both sides of vertical and horizontal splits, while oversized dialogs remain pannable when scaling is off. I’m finishing the combined regression run and recording the source hashes for the coordinated build.

## Report at 2026-10-19T12:23:56.623Z

Original transcript line 5239.

Menu scaling is implemented and settled.

- Checkbox plus 25–100% sizing; defaults off/100%.
- Title and Preferences always fit, including x3.
- Pixel, click, Alt, split-view, and scenario-preview tests pass.
- 644 tests passed; 3 existing GPU tests remain ignored.
- Scene protocol is now 38; rebuild bridge and viewer together.

Evidence and source hashes: [menu-scaling-result.json](/mnt/datasets/c3r-build/macos-port/menu-scaling-result.json). Native build and GUI validation remain with root.

## Report at 2026-10-19T13:34:50.184Z

Original transcript line 5261.

For the VM measurement, a 1012×632 physical client area lowers requested x3 to effective x2 because the renderer keeps at least 320×240 pixels. I’m writing the recipe around measured client size and effective scale so the evidence does not label x2 as x3.

## Report at 2026-10-19T13:37:21.802Z

Original transcript line 5346.

Sent QA the control coordinates, Accept/reopen sequence, and expected measurements.

At 1012×632 physical client size:

- Requested x3 runs at effective x2.
- Preferences off or on100: approximately 843×632.
- Preferences on25: approximately 211×158.
- Actual x3 requires at least 960×720 physical client pixels.
- Title and Preferences always fit. Middle-drag panning applies to other oversized dialogs.

Background sampling has no UI toggle. Its four GPU regressions verify background changes while controls, text, and world art retain their sampling.

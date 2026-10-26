# Native C3X mill icons

This Group 6 subset implements the resource badges over improvement icons on
the city screen. It does not complete Group 6 or accept the no-jgl default.
The isolated candidate merges `339bac3` and the accepted standalone Group 7
checkpoint `5ca2cf6`. Main's documentation was preserved at both merges.

## Producer and source

[V] C3X R28 source `d4e71c4e644bbfe52638c1d180778136e3b906b6`,
`injected_code.c:34682` through `34734`, selects resources while drawing each
improvement row. `patch_draw_improv_icons_on_city_screen` supplies the actual
improvement ID. Its nested sprite draw supplies the canvas and row position.
The new hook wraps that specific call at PCGames `0x421315`, whose target is
the C3X producer `0xDEEC18`. It calls the original first, preserves its return
value, and emits `Event::MillIcons` before the row's subsequent decorations.
No pointer/input, window/pump, owned-image or frame-mask behavior changes.

| Field | Original producer | Native source |
|---|---|---|
| Improvement ID | `is->drawing_icons_for_improv_id` | R28 state `+0x2588`, set by the enclosing list-row callback |
| Mill order and resource IDs | `current_config.mills/count_mills`; packed six-byte `struct mill` | R28 state `+0x6E0/+0x6E4`, signed-short improvement/resource IDs and flags |
| Resource class and icon | `BIC.ResourceTypes[resource]`, stride `0x5C` | Pointer at `0x9C7194`; class `+0x3C`, IconID `+0x48` |
| Focus city and civ | `p_city_form->CurrentCity` | Pointer at `0x7423AC`; City.Body ID and signed civ byte |
| Resource technology | `can_generate_resource` | Actual answer at call site `0xDEED65` to `0xD9FA83`, including `MF_NO_TECH_REQ` |
| Active building | `has_active_building` | Actual answer at call site `0xDEED87` to `0xDA7B13`, including free/extended improvements, government and obsolescence |
| Required input resources | `has_resources_required_by_building` | Actual answer at call site `0xDEEDA9` to `0xDA9632`, including production chains, local/extended resources and radius requirements |
| Sheet identity | `is->resources_sheet` | R28 state `+0x258C`; its PCX owner names the retained source |
| File art and palette | C3X load at `24568`: `BIC_get_asset_path("Art\\resources.pcx", true)`, then `PCX_Image_read_file(..., flags=2)` | The same resolved path, captured at successful file load; palette decoded once and retained until clear |
| Row position | `pixel_x/y` passed to the nested Sprite::draw | Exact untrimmed call arguments, after native list placement/scrolling |
| Clip | Destination image/DC clip | Exact JGL clip field through `civ3_sys::ui_hooks::canvas_text_clip`; full and empty clips retained |

Raw reads and checked foreign calls stay in `civ3-sys`/`inproc`.
`civ3-ui::mill_icons` owns selection and layout. The bridge only wires the
producer, file lifetime and event. Three checked call-site hooks retain the
answers from C3X's existing query calls. They return the original bytes and
add no engine queries or game-state changes. Recursive resource queries do
not pass these three producer call sites. A game-thread scope keeps nested
row draws separate. It does not infer active buildings or input availability
from names or from a later publisher snapshot. The count-zero configuration
emits no icons.

[V] `Producer::find` checks the R28 state version/size, the row call, the
producer prologue, all three helper call displacements and helper prologues,
and the state-pointer binding. This audited binary has SHA-256
`a9bb72429328db88da8dfcab3e706437dc7bdc540e07cbfd4923695665e1671a`.
Another R28 rebuild needs its own checked addresses. Other versions and
vanilla builds get no mill hook. Missing source data or unreadable state
produces an explicit error, never a guessed icon.

## File rendering contract

[V] Selection keeps config order and duplicate resources, with a cap of 16
accepted entries. Bonus resources require `MF_SHOW_BONUS`; non-bonus resources
are hidden by `MF_HIDE_NON_BONUS`. `local` and `yields` do not independently
gate the badge. The three native queries must all succeed.

For accepted entry `n` of `count`, the original source is
`[9 + 50*(IconID%6), 9 + 50*(IconID/6), 33, 33]`. Its destination is
`[pixel_x+15, pixel_y+(n*160/max(1,count-1)+5)/10, 24, 24]`, with integer
division. The full source remains intact through destination clipping and
canvas copies. Later icons overlap earlier ones, and later row decorations
remain above them. Redraw/clear and composition use the existing UI model.

[V] The C3X sheet is an 8-bit PCX image. Its base vtable's `+0x10` method
at `0x607C50` returns zero, so `0x6001A7` takes the indexed decoder
`0x62BD90`, which explicitly creates an 8-bit image at `0x62BE84`.
TransparentBlt converts this source into the row's RGB555 destination before
applying the RGB magenta key. Five private GDI boundary-palette cases verify
that order, including near-magenta and duplicate-magenta entries.
The native path makes exactly those file palette entries that convert to
`0x7C1F` transparent. It does not apply the ordinary sprite rule
that also clears index 254. The checked resource file has only index 255
as the key. The renderer uses the same file art pipeline as other UI art.

[V] Private Wine 11.0 tests found different horizontal and vertical sampling
phases for this 33-to-24 operation. The atlas's existing fixed-point sampler
now accepts an initial offset. Mill icons use step `[90112,90112]` and offset
`[65535,32768]`. Existing art defaults to zero offset. Art keys, preload
serialization and UI traces retain the offset. This is a generic sampler
parameter; no game state entered a generic crate.

Scratch HDC pixels are never renderer input. This work neither intercepts
TransparentBlt output nor builds a canvas rasterizer. C3X's original DC
calls remain for its original return/handle behavior; native badges come
from mill state and the resource file even though the scratch bitmap is
only 1x1. The separate GDI+ line contract is outside this subset.

## Evidence and reproduction

Evidence root: `/mnt/datasets/c3r-build/jgl-mills-t3-269e6006`.
`audit/inputs.sha256` binds the original exe, R28 source/headers/config,
rules and art. `audit/supported-binary.json` records the checked code bytes.
`audit/layout.txt` records offsets generated by the 32-bit compiler.

- [V] `tools/mill-icon-oracle` extracts the original C3X producer and compiles
  it with controlled native-query answers and a command recorder. All 65,537
  selector/layout cases match, including empty outputs, classes/flags,
  disabled query answers, duplicates/order and the 16-entry cap. FNV-1a
  over counts and source/destination coordinates is `0x5217C163DF8B908C`.
  These controlled answers do not prove a running city's resource state.
- [V] `tools/mill-icon-gdi` decodes the reference PCX into a private indexed
  8-bit source and runs original Wine TransparentBlt into RGB555 in a private Wine prefix.
  The production UI-model/atlas path matches all 36 icons in 20,736 RGB555
  pixels, with zero differences. It compares over background `0x1234`.
  The final source-depth receipt is `gdi8-oracle/` and `tests/art8.log`.
  The earlier 16-bit-source diagnostic remains under `gdi-oracle/`; it
  independently found the sampling phase but is not the source-depth proof.
- [V] Source/state tests cover packed runtime IDs, count-zero and invalid
  resource IDs. Model tests cover overlapping badge order, list clipping,
  later decorations and copied-canvas sample phase. Preload and trace
  metadata preserve that phase. Earlier failing sampling/compile attempts
  remain in the evidence root.

Run inside `nix develop`, with `CARGO_INCREMENTAL=0` and a private target:

```sh
python3 tools/mill-icon-oracle /mnt/datasets/winvm/c3x-src "$EVIDENCE/source-oracle"
python3 tools/mill-icon-gdi "$ART/resources.pcx" "$EVIDENCE/gdi-oracle"
C3R_MILL_ART="$ART/resources.pcx" C3R_MILL_GDI="$EVIDENCE/gdi-oracle" \
  cargo test -p civ3-bridge ui_mills::tests::file_art_matches_original_transparent_blt_for_all_36_icons -- --ignored --exact --nocapture
C3R_MILL_RULES="$CONQUESTS/conquests.biq" \
  cargo test -p civ3-bridge ui_mills::tests::enabled_and_disabled_config_fixture_uses_resolved_game_ids -- --ignored --exact --nocapture
```

The [minimal fixture](evidence/jgl-mill-icons/fixture.json) binds standard
Granary ID 2 to Iron resource ID 1, IconID 1. The [enabled config](evidence/jgl-mill-icons/enabled.c3x_config.ini)
is `[Granary: Iron]`; its query answers are explicitly supplied in the offline
fixture. The [disabled config](evidence/jgl-mill-icons/disabled.c3x_config.ini)
is empty. Reference rules SHA-256 is
`1bd610ab1420217f7d64a9214b60c0bc7686e4781a0d6e66590171535495609c`;
resource art SHA-256 is
`5dafd5f681768e2db13e9e08f783e409905a99f35cf56f3065b0ac240d19cd11`.
The checked default config has `buildings_generating_resources = []` and
SHA-256 `09d1da8ab1cf0fbb7a72e422179fa95fbaa92fe271a4b7b9b9e819bb4f98c3b3`.

## Remaining live gate

[G] The injected row hook has not yet been exercised in an enabled-mill
game. The offline results do not establish live menu ordering, every mod's
art, another GDI/Wine implementation or Neo behavior. No game, shared DLL,
shared prefix, ini or save was changed in this task. No fresh game-session
no-jgl/counting claim is made. The original no-viewer/no-trace path and
explicit `CIV3R_JGL=own` opt-in remain unchanged.

After the parent assigns the sole GUI lease, use `tools/civ3-session` with
`input.alt_in_menus=false`. Retain fresh DLL/config/log hashes. Use a save
whose city has a Granary and knows Iron Working, with the enabled fixture
config. Compare original-jgl and owned sessions at the same city row,
including a scrolled/clipped row, a second resource on the same improvement,
closing/reopening the city and reloading the save. Repeat with the empty
config, a missing Granary and unavailable technology/input resources.
Capture `CIV3R_UI_WATCH` for the actual row canvas to bind `MillIcons` IDs,
coordinates and clip to each screenshot; compare only the corresponding
row crops. Run counting/loader checks afterwards. Restore changed DLLs and
config files and retain matching hashes. This live receipt remains open.

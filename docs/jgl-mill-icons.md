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
the C3X producer `0xDEEC18`. It retains the original sprite command and C3X
selection queries, preserves the sprite return value, and emits `Event::MillIcons` before the row's
subsequent decorations. In owned mode a checked branch skips the producer's
legacy mill drawing block after its original availability gates.
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
row draws and draw-readiness notifications separate. It does not infer
active buildings or input availability from names or from a later publisher snapshot. The count-zero configuration
emits no icons.

[V] `Producer::find` checks the R28 state version/size, the row call, all
1,031 bytes of the producer, all three helper call displacements and helper
prologues, and the state-pointer binding. The retained producer body has
SHA-256 `8950949b63dca04860a25044e41c0feeb953250d27a21fbad56d5230317e1e6e`.
This audited binary has SHA-256
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

## Avoiding legacy mill drawing

[V] The original selector and native queries still execute once. C3X's own
checks for a positive selected count, a resource sheet, its JGL image and a
non-null `TransparentBlt` pointer remain at `0xDEEDDE..0xDEEE33`. The new
boundary at `0xDEEE34` notifies the current row that these gates passed.
Owned mode then jumps to `0xDEF018`, the existing `mov eax,[ebp-12]; leave;
ret 16` epilogue. It avoids both image DC acquisitions, every mill
`TransparentBlt`, both DC releases and the intervening draw-coordinate loop.
The original saved sprite result, query order, short circuits and native
query side effects remain intact. No arbitrary GDI result is invented.

The boundary saves registers and flags, aligns the callback stack, and
restores them before continuing. In original-jgl/viewer mode it executes
the stolen six bytes and resumes the original draw block. Dropping the
boundary restores the original bytes. It is installed only with UI capture
on the checked PCGames R28 producer. Plain no-viewer/no-trace pass-through
never installs it. The bridge uses the existing immutable `own_mode()`
selection; the default remains original jgl.

The safe bridge capture emits icons only when the native availability gates
actually reached that boundary. A missing sheet no longer erases the mill
identities needed to retain earlier query answers. Reentrant rows have their
own capture/readiness scope, and no borrow spans a foreign call. Missing
file provenance still produces an error, without resuming legacy drawing.

[V] The retained sprite command calls C3X's original trampoline at
`0xCD4040`, which resumes `Sprite::draw` at `0x5F8215`. The normal loaded
sprite path uses graphics scale `+0xA0` and sprite `+0x44`. Owned sprite
blits record file/model commands and perform no GDI drawing. The deferred
file-backed path retains loading/decoding, an image-copy command and clear;
owned canvas copies remain commands. Existing owned image/font/DC resource
creation and state work is outside this bypass and is not claimed removed.
The three retained query helpers inspect technology, active/free/extended
buildings, government/obsolescence and recursive resource/radius state;
they do not draw. GDI+ lines are a separate contract and this path does not
call them.

Scratch HDC pixels are never renderer input. No DC is acquired for a mill
badge in owned mode. The native path uses the existing resource-file art
pipeline, without a canvas rasterizer or a captured GDI result.

## Evidence and reproduction

Retained evidence root: `/mnt/datasets/c3r-build/jgl-mills-t3-269e6006`.
The drawing correction and fresh comparisons use
`/mnt/datasets/c3r-build/jgl-mills-t3-630df663`.
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
- [V] The correction's `tools/test-mill-icons` executes the actual supported
  producer machine code in a private, headless Wine process. Twelve paired
  cases retain identical sprite/query/readiness order and return values,
  including nested draws, empty and rejected selections, absent sheet/JGL/
  function pointers, DC failures and the 16-icon cap. Owned mode calls zero
  legacy raster dependencies and avoids 41 recorded acquire/blit/release
  calls. Original mode and removal of the patch retain the original block.
  The non-nested cases also check stack cleanup and all callee-saved registers.
  Query answers and Sprite/GDI dependencies are controlled recorders in this
  fixture. This does not test a running city's helper answers or live hook
  installation. See `boundary-final/receipt.json` and the retained `test.exe`.
- [V] Source/state tests cover packed runtime IDs, count-zero and invalid
  resource IDs. Model tests cover overlapping badge order, list clipping,
  later decorations and copied-canvas sample phase. Preload and trace
  metadata preserve that phase. Earlier failing sampling/compile attempts
  remain in the evidence root.

Run inside `nix develop`, with `CARGO_INCREMENTAL=0` and a private target:

```sh
python3 tools/test-mill-icons "$CONQUESTS/Civ3Conquests.exe" "$EVIDENCE/boundary"
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

## Correction validation

[V] Signed production commit `4020c1c` contains the draw bypass and its
fixtures. The [continuation receipt](evidence/jgl-mill-icons/continuation-receipt.json)
binds the source, supported binary, private test executable, art oracle,
release artifacts, test results and cleanup record. All final boundary
source hashes match that commit exactly.

Fresh source and art comparisons repeat the 65,537-case selector result and
all 20,736 RGB555 icon pixels with zero differences, plus five palette-key
boundary cases. The affected packages `civ3-ui`, `civ3-sys`, `civ3-bridge`,
`menu` and `render-desc` pass 834 tests with 13 ignored. The two manual
art/config tests also pass when explicitly enabled. `no_locks` passes both
tests and `generic_crates` passes all three. Native `civ3r-native` release and
i686 Windows `civ3-bridge`/`civ3r-native` release builds pass. The full
workspace was not rerun for this scoped correction; its known
`tile-render/subpixel_coverage` failure is not waived.

Earlier failed private fixture attempts remain recorded. Attempt 1 failed
to compile because a hook closure needed `move`. Attempts 2/3 could not
reserve the binary's low address range after Wine had allocated heaps.
The final fixture links its own text/data above `0x02000000`, checks that
the low image gap belongs to itself, and runs the original producer there.
Its temporary PE header is restored on drop. Attempt 4 and final binaries
are retained; the earlier attempt 2/3 executable hashes remain, but Cargo
had overwritten those executables before retention was added.

The task changed only its isolated checkout and private evidence/target/
Wine prefixes. All five private prefixes are quiescent. No shared game,
DLL, configuration, save, input or warm target was used or changed, so no
shared-file restoration was needed. Targets and diagnostics remain retained
for parent review. The private fixture does not establish live installation
or real-city query answers, and the atlas pixel test does not establish final
live composition.

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
coordinates and clip to each screenshot. Bind a scoped caller count to
`0xDEEE6A`, `0xDEEE90`, `0xDEEFC9`, `0xDEEFF2` and `0xDEF016`: the original
session must reach its mill acquire/blit/release calls, while owned mode must
reach the boundary and none of those five sites. Check that native badges
still appear and that queries and subsequent row decorations remain ordered.
Compare only the corresponding row crops. Run counting/loader checks afterwards. Restore changed DLLs and
config files and retain matching hashes. This live receipt remains open.

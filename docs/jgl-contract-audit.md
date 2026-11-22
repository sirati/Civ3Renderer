# Remaining jgl object contracts

The latest reviewed isolated candidate is `31da02c`, combining Group 6 with
the accepted standalone Group 7 implementation. Parent review accepted the
scoped data/state/DC comparisons after independently replaying all three
fixtures. Full Group 6 remains unaccepted because the final game exits on an
unresolved HUD pixel query. See the [parent review](evidence/jgl-readback-review/review.json)
and [current handover](jgl-continuation.md).

Earlier direct-reader audit evidence remains at
`/mnt/datasets/c3r-build/jgl-t3-d94deaaf/audit`, with input hashes, all 24
call-site contexts and checked binary targets. Earlier Palace/source/data
evidence remains in the [replacement record](jgl-replacement.md). The
separate [window/pump matrix](jgl-window-contracts.md) retains its accepted
standalone scope and pending integrated game gates.

## Group 6 correction checkpoint, 25 October 2026

This isolated continuation merges `339bac3` and accepted standalone Group 7
`5ca2cf6` while retaining the parent's acceptance record. Group 6 remains
unaccepted and `CIV3R_JGL=own` remains an explicit opt-in. Evidence for this
round is `/mnt/datasets/c3r-build/jgl-group6-data-20261025`.

[V] New binary comparisons found and corrected two concrete defects. Image
`+0x38` traverses overlapping bytes forwards, observing preceding writes;
a snapshot differs on nine adversarial cases. The constructor key is
`0x80007C1F`, not `0x7FFF7C1F`. Defined depth/no-DC errors now precede the
data policy. The corrected test passes 201 data operations, 19,660 bytes,
and 492 defined state/error pairs. It covers padded rows, destination clips,
negative source/destination origins, overlap, initial `+0x58`, byte/word
replacement, repeated creates, lock thunks and over-unlock. It excludes
undefined register values, handle identities, allocation failure and use of
the original's dangling pointer after clear. This closes these scoped
subcontracts, not the entire image state/error row.

[V] The C3X GDI+ sequence now has a drawing test against original jgl,
8,224 status/state pairs and 16 image lifetimes with zero differences. It
covers direct-DC binding, smoothing, clips, pen width/alpha/dashing, four
line placements, release before drawing and deletion. The 1x1 scratch DC
has no authoritative canvas pixels. C3X's checked sequence consumes the
status and retained graphics handle, never the scratch pixels. Logical image
clip state matches; effective bitmap extents and resulting pixels are not
claimed equal. Mill source/rendering and actual enabled-feature qualification
remain outside this test.

[V] Both frame-reader sites cover **four** caches. The branch into
`608E7E` also selects `+0x124` and `+0x120`, alongside bottom `+0x128`;
`608E0A` reads top `+0x11C`. `606F30` allocates and initializes them;
`607170` paints backgrounds, tiled images, frame lines, caption text and
virtual per-form content. The retained disassembly records these writers.
The global capped sprite history and its O(n) eviction are removed. Frame
queries now use the viewer's retained canvas model. The oldinterface probe
alone retains per-image immutable source/crop/placement/clip, independent of
sprite and source-image destruction. Tests include 1,200 unrelated canvases
without evicting a retained frame source.

The shared model now explicitly refuses unrepresented writers, unknown
initial content, text glyphs, masked/paletted/translucent samples and indexed
samples without an exact query. Such provenance follows slices/copies and
ends with the appropriate clear/reset/full overwrite. Owned hit queries
terminate with a contract error instead of guessing transparency or reading
dummy bytes. **This is an explicit unsupported-contract boundary, not frame
acceptance.** `+0x38` frame tiling and caption pixels still need exact shared
semantic answers and original live predicate comparisons. The loaded-art
`+0x58` basename rules have not been accepted as instance/rectangle transforms;
those sources are now refused by exact hit queries rather than silently
certified. Other reader families and dynamic/replay/scenario qualification
remain open as described in the matrix below.

[V] The installed baseline failed before a usable viewer because its bridge
sent wire version 43 to a native library expecting 11. Fresh, paired builds
using the initial merged Windows candidate `b81e7ce` reached a visible original title and save summary,
then an owned title, loaded map, F3/F5 and the quit dialog. Those images are
scoped startup diagnosis on the **initial merged code**, which still had the
rejected frame reconstruction. They establish neither final-code frame
comparison nor wider screen/turn/combat acceptance. The initial host build
has retained hashes but no contemporaneous dirty-source snapshot, so its
pure-commit provenance is not certified. Final production artifacts were
rebuilt after `34d7add`. Failed launch attempts, artifacts and restoration
records are retained separately.

## Group 6 correction checkpoint, 25 October 2026

This isolated continuation merges `339bac3` and accepted standalone Group 7
`5ca2cf6` while retaining the parent's acceptance record. Group 6 remains
unaccepted and `CIV3R_JGL=own` remains an explicit opt-in. Evidence for this
round is `/mnt/datasets/c3r-build/jgl-group6-data-20261025`.

[V] New binary comparisons found and corrected two concrete defects. Image
`+0x38` traverses overlapping bytes forwards, observing preceding writes;
a snapshot differs on nine adversarial cases. The constructor key is
`0x80007C1F`, not `0x7FFF7C1F`. Defined depth/no-DC errors now precede the
data policy. The corrected test passes 201 data operations, 19,660 bytes,
and 492 defined state/error pairs. It covers padded rows, destination clips,
negative source/destination origins, overlap, initial `+0x58`, byte/word
replacement, repeated creates, lock thunks and over-unlock. It excludes
undefined register values, handle identities, allocation failure and use of
the original's dangling pointer after clear. This closes these scoped
subcontracts, not the entire image state/error row.

[V] The C3X GDI+ sequence now has a drawing test against original jgl,
8,224 status/state pairs and 16 image lifetimes with zero differences. It
covers direct-DC binding, smoothing, clips, pen width/alpha/dashing, four
line placements, release before drawing and deletion. The 1x1 scratch DC
has no authoritative canvas pixels. C3X's checked sequence consumes the
status and retained graphics handle, never the scratch pixels. Logical image
clip state matches; effective bitmap extents and resulting pixels are not
claimed equal. Mill source/rendering and actual enabled-feature qualification
remain outside this test.

[V] Both frame-reader sites cover **four** caches. The branch into
`608E7E` also selects `+0x124` and `+0x120`, alongside bottom `+0x128`;
`608E0A` reads top `+0x11C`. `606F30` allocates and initializes them;
`607170` paints backgrounds, tiled images, frame lines, caption text and
virtual per-form content. The retained disassembly records these writers.
The global capped sprite history and its O(n) eviction are removed. Frame
queries now use the viewer's retained canvas model. The oldinterface probe
alone retains per-image immutable source/crop/placement/clip, independent of
sprite and source-image destruction. Tests include 1,200 unrelated canvases
without evicting a retained frame source.

The shared model now explicitly refuses unrepresented writers, unknown
initial content, text glyphs, masked/paletted/translucent samples and indexed
samples without an exact query. Such provenance follows slices/copies and
ends with the appropriate clear/reset/full overwrite. Owned hit queries
terminate with a contract error instead of guessing transparency or reading
dummy bytes. **This is an explicit unsupported-contract boundary, not frame
acceptance.** `+0x38` frame tiling and caption pixels still need exact shared
semantic answers and original live predicate comparisons. The loaded-art
`+0x58` basename rules have not been accepted as instance/rectangle transforms;
those sources are now refused by exact hit queries rather than silently
certified. Other reader families and dynamic/replay/scenario qualification
remain open as described in the matrix below.

[V] The installed baseline failed before a usable viewer because its bridge
sent wire version 43 to a native library expecting 11. Fresh, paired builds
using the initial merged Windows candidate `b81e7ce` reached a visible original title and save summary,
then an owned title, loaded map, F3/F5 and the quit dialog. Those images are
scoped startup diagnosis on the **initial merged code**, which still had the
rejected frame reconstruction. They establish neither final-code frame
comparison nor wider screen/turn/combat acceptance. The initial host build
has retained hashes but no contemporaneous dirty-source snapshot, so its
pure-commit provenance is not certified. Final production artifacts were
rebuilt after `34d7add`. Failed launch attempts, artifacts and restoration
records are retained separately.

## Per-contract matrix

| Contract | Required behaviour and source | Current owned implementation or change | Scoped evidence and outstanding gate |
|---|---|---|---|
| Image construction, create/clear, getters, key and clip | ABI fields, row pitch, retained sizes on clear, same-size create preserving content, clip intersection, lock counters and owner clearing. No canvas drawing is required to answer these. | Own constructors and plain allocations exist. Clip/key/getters and pixel thunks are implemented. Clear frees plain memory and zeroes its pointer where original leaves a dangling pointer. No equivalence is claimed for using that freed original pointer. Same-size create after clear preserves the original no-op, including its absent DC. | [V] Recovered binary semantics and layout tests. The new 492 defined state/error pairs cover the scoped 8/16-bit transitions above; handle identities, allocation failure, undefined register values and unsupported file/draw slots remain outside that test. Earlier mismatches remain retained in §9. |
| City-screen region map, form `+0xC54` | Reads at `414FC9`, `4154C9`, `415719` return an 8-bit region index or `FF`. | Exe PCX decoder writes plain memory; `get_pixel` reads that memory. No drawing reconstruction should run. | [V] Reader addresses rechecked. Recovered research attributes the source to file data. Still needs source-load/writer provenance and live coordinate/index comparison for this family. |
| Region map at form `+0x662C` | Read at `4CCBF6`, index used by the form's interaction handler. | Same decoded-file/plain-memory answer. | [V] Reader address rechecked; source/writer and live family evidence still required. |
| Espionage region map, form `+0x3DC8` | Reads at `52B409`, `52B55E`, `52B629`, with `FF` meaning no region. | Same decoded-file/plain-memory answer. | [V] Reader addresses rechecked; source/writer and live family evidence still required. |
| Region map at form `+0x2B60` | Reads at `5835BA`, `583626`, `583A23`. | Not a file: the player setup fills it (all `FF`, then one rectangle per choice, `5819D6` and `5FFEF0`). Answered from the recorded fills (`option_masks`). | [V] Writer found (2026-11-22); live check pending. |
| Region map at form `+0x574` | Reads at `5A45DC`, `5A488F`, `5A5013`; indices `FC` through `FE` have special handler branches. | Same decoded-file/plain-memory answer. | [V] Reader addresses and special branches rechecked. Do not substitute a generic opaque/transparent answer. Source/writer and live family evidence still required. |
| Region map at form `+0x3958` | Reads at `5DEA2A`, `5DEA94`; the result selects an entry. | Not a file: the world setup fills it (all `FF`, then one rectangle per choice, `5DD4B2` and `5FFEF0`). Answered from the recorded fills (`option_masks`). | [V] Writer found (2026-11-22); live check pending. |
| Cursor hotspot reads | `54AFC5`/`54AFE9` scan the decoded cursor sheet for index `FB`. | Exe file decode and raw reads; sprites remain placeholders. | [V] Reader loops rechecked. Fresh sheet-byte and hotspot comparison remains required; existing cursor appearance tests do not by themselves establish this contract. |
| Oldinterface presence | `549FAD` tests whether each of three sprite-derived images exists. | Retained decoded source and sprite crop answer the index from the same `FilePixels` source used by the model. | [V] Prior three original comparisons returned `40` on both paths. Later source ownership comparison covers oldinterface without synchronous decode at first read. Not repeated here. |
| Palace selection | `57C706`, `57C7C4`, `57C7FA` read the eligible-piece ordinal. Geometry, clip, order and indices below `FE` decide game state. | Shared `Selection` and immutable decoded file handles; publisher computes anchors from the same Present's captured fields. | [V] Prior 14,515,200 cells, 15 reads, upgrades/style changes; later 23,713,540 source indices and four crops. Source retention and anchor corrections are preserved. Other saves/resolutions and initially credit-free entry remain separate coverage gaps. |
| Frame hit masks | `608E0A` reads form pointer `+0x11C`; **`608E7E` also selects `+0x128`, `+0x124` and `+0x120`**. Both compare with the colour key before accepting a frame hit. | Both sites now query the shared retained canvas model. The capped sprite-pointer reconstruction and initialized-key guess are removed. Unsupported owned reads are logged and forwarded to the active image implementation; no process-termination diagnostic remains. | [V] Both readers and allocation sites rechecked in `audit/frame-data-writers.txt`. The shared model is used; frame `+0x38` and non-text source provenance remain separately scoped. |
| Keyed form canvas | `608FE3` tests the form's `+0x274` canvas against the key. | `UiModel::shown_at` answers from the model and file pixels. | [V] Existing hit-test checks. Owned mode now refuses incomplete sources, text glyphs and missing provenance. Default mode retains its existing behavior. Original live predicate equivalence is still open. |
| Generated navigator copies and clear | `5784EC`/`578733`: terrain `+18` to grey-work `+588`; `578625`/`57886C`: grey-work `+588` to political `+2D0`. The exe then modifies and reads the bytes; `+858` is cleared to black before reveal. | **Changed:** host marks the four buffers of the known HUD/advisor navigators as caller-written data. Own `+40` uses safe row copies only when both ends have that contract; own `+44` preserves their 16-bit fills. Canvas targets remain record-only. | [V] Disassembly in `audit/navigator-create-reset.asm`. Original-binary test: 40 copy/clear calls, 1,369,360 bytes, zero differences, including padded rows and destination clips. Live buffer comparison is recorded below. Dynamic/replay navigators are not covered by this static-owner policy. |
| Escaped pixel pointers, file decode/crop, FLC | Decoders and crop trimming use the pixel/lock thunks and strides; callers write/read the bytes directly. | Plain image memory; sprite init keeps one striped allocation. | [V] ABI/decoder research. No new per-pixel hook was added. The lower-bound reader inventory still needs runtime coverage for these pointer families and the dotted-frame loops, independently of `get_pixel` counts. |
| C3X minimap dot and combat-odds background | C3X writes a city-dot pixel and saves/restores the underlying HUD bytes through `+1C`, sometimes without unlocking. | Plain buffer preserves these writes; navigator copies now retain the generated source. Viewer minimap/HUD rendering uses world/model state. | [V] C3X source. [G] Display-only saved bytes need not match the old HUD raster. Confirm no logical consumer and exercise the enabled C3X features; do not call an arbitrary blank read a verified answer. |
| Image `+38` tile-under | Only replace destination key pixels with the file tile through its palette, clipped to the requested frame piece. Caller `5FCE9C` is inside `5FCD40`, reached by frame painter `600520`. | Caller-written data copies now match original forward overlap and clipped/padded 8→8 and palette 8→16 operations. Canvas targets remain record-only and are explicitly marked unanswered for exact hit queries. | [V] Recovered call graph and old counts. The new binary test covers this data subset. Frame tile-under semantics and all four frame-hit consumers remain open; there is no canvas pixel rasterizer or new mirrored bitmap. |
| Image `+58` on loaded 8-bit art | `4C6ABA`/`4C6ACA` replace `0`/`1` with `FF` in `Art/Terrain/Territory.pcx`; `54A3A5` replaces `FD` in `Art/units/units_32.PCX`; `54ADBF` does so in `art/Advisors/foreignTAB.pcx`. | Safe `Indexed::replace` changes owned bytes. Model file-source metadata currently carries no replacement transform. `ui::indices::blank` already names Territory's `0`/`1` rule on the coloured-blit path. | [V] Call contexts and asset string VAs checked in `audit/checked-sites.json`. Need compare modified indices with original and verify each viewer draw/hit path uses the same transparency rule, including units_32 and foreignTAB. Raw-memory equality alone is insufficient. |
| Image `+58` on glyph canvases | Text renderer calls at `6230xx` through `6236xx` recolour 16-bit glyph pixels. | Owned `replace_color` currently changes only 8-bit images; 16-bit body returns success without work. Text commands already retain font/colour. | [G] Record-only may be sufficient for display, but every caller's later reads/copies and the resulting text command must be checked. Unsupported depth and absent-pixel error answers also differ. This contract is open. |
| Image `+3C`, PNG load/save, unobserved draws | Their error answers and side effects are defined by jgl even if previous sessions never called them. | `+3C` warns and returns success; PNG/PCX file slots return not implemented; several line/frame bodies return zero. | [V] Source inspection. These are explicit unsupported cases, not equivalent implementations. Keep them visible in acceptance scope and counting evidence. |
| Font measurement and resources | Real GDI metrics/extent determine game layout; HFONT and FOT lifetimes matter. | Dedicated real memory DC, GDI font/metrics/measurement, owned FOT lifetime and embedded-base destruction. A real DC is justified because the OS font engine supplies layout and viewer glyphs. It need not contain a game image. | [V] Prior 2,369,297 measurements with zero differences; later eight load/measure/release resource cycles and registry-order comparison. No settled font/registry work repeated here. |
| Image text-out and state/DC slots | Text command needs selected font, colour and clip. Any external DC consumer must receive its actual required state. | Producer retains text state; image text bodies are record-only. Semantic keyed reads use the same retained atlas images or packed outline words and integer coverage routine as the viewer's glyph commands; no `GetGlyphOutlineA`/`TextOutA` calibration path remains. | [V] Existing text-record path plus shared renderer-source query wiring. Runs without a retained atlas/outline source remain explicit unsupported cases. |
| C3X GDI+ line initialization | `34479` directly reads image `+4BC`, calls `CreateFromHDC`, retains `gp_graphics`, and returns `0` or `2`. | Existing line hooks retain model pen/canvas state. In owned mode bind/color/opacity/width/draw do not call the legacy C3X/GDI+ implementation; line fills are emitted to the model for the renderer. Pass-through keeps the original calls. | [V] C3X source and current hook inspected. The later original-binary sequence establishes 8,224 status/state pairs over 16 lifetimes, including actual GDI+ draws after release, with zero differences. Owned live line appearance remains open pending the combined GUI gate. |
| C3X mill icons | `34682` selects up to 16 resources for the actual improvement row; `34703`/`34705` acquire DCs and scale 33x33 resource-file regions to 24x24, keyed by RGB magenta. | Native source integration is implemented at the checked PCGames row call `421315`. Actual C3X mill/resource IDs and native technology/active-building/input-resource queries feed safe `civ3-ui` selection/layout and `MillIcons` events. The resource palette is retained at file load; the existing atlas renders file art with original order, clip and sampling phase. The checked owned draw boundary skips both mill DC acquisitions, every TransparentBlt and both releases after native selection/availability gates. Original-jgl/viewer resumes the original block. Scratch/GDI pixels are never viewer input. | [V] 12 actual-binary original/owned pairs preserve query/order/returns and avoid 41 raster calls, with zero owned raster calls. 65,537 original-source selector/layout cases and 20,736 original Wine GDI/native file-art pixels match, with zero differences; raw-state, disabled, order/clip/copy-phase fixtures pass. [G] Live enabled-mill hook/row comparison remains a parent GUI-lease gate. Exact sources, fixture and procedure are in [the mill contract](jgl-mill-icons.md). This closes the missing source implementation, not the live acceptance gate. |

## Runtime data check

The temporary diagnostic snapshots all four navigator buffers immediately
after their normal initialization. It uses one guarded block read per buffer,
never per-pixel hooks. Those bytes are evidence for the generated-data
contract only; they are not sent to the renderer. Source, DLL hashes, logs,
screenshots and restoration receipts are retained under `diagnostic/`,
`production/` and `runtime/` at the evidence root.

All eight buffers matched, 546,720 bytes with zero differences. The diagnostic
owned session found no jgl mapping at all eight observations and no load in
its Wine log; original mode loaded jgl. Source commit `9dc2194` contains no
snapshot code. Its final production run repeated save load, map, F3/F5,
Escape and confirmed Quit, with no forced stop. The minimap and both advisor
crops matched exactly. The diagnostic F5 crop's earlier 1,731-pixel text
difference remains retained separately.

The exact source/artifact binding is in `continuation-receipt.json` at that
root. Neither this scoped comparison nor
the prior Palace checks close group 6, group 7 or the default switch.

## Final-code runtime result for the correction checkpoint

[V] Final production implementation `34d7add` was built with matched PE and
Unix artifacts and retained hashes. The test-only commits `3625617` and
`5090da9` do not alter those production bodies. The owned session reached
the title, Load Game dialog, save summary and loaded map. Entering the HUD
at `(1642,970)` then produced the observed error
`owned keyed canvas 0x9fdd84: no exact model answer`, followed by process
exit 1 at 290.127 seconds. `runtime/final-own/map.png` precedes that pointer
probe; `hud-read.png` shows the desktop after the exit. This is a reproducible
missing HUD semantic source, not a load-dialog or browser failure. It blocks
Group 6 acceptance and further integrated qualification. The earlier
`b81e7ce` map/advisor/confirmed-Quit run remains separately bound and cannot
waive this final-code failure.

[V] `cargo test --workspace --no-fail-fast` completed all targets and doc
tests: 2,116 passed, one known `tile-render/subpixel_coverage` failure,
37 ignored. Explicit later runs of `civ3-bridge`, `civ3-ui`, `civ3-sys`, `jgl`,
`jgl-ffi`, `inproc`, `generic_crates` and `no_locks` passed. The final native
release and i686 release builds passed. The corrected navigator fixture again
passes 40 calls and 1,369,360 bytes; its retained earlier failure documents
the invalid assumption that same-size create after clear recreates a DC.

Fresh baseline hashes cover the DLLs, native libraries, ini, bridge log and
save presence. Those entries are restored and checked in the receipt.
`civ3-render-errors.log` was omitted from the initial capture; its later
248-byte zero-error content is retained with an explicit late-capture flag.
Its original pre-run hash cannot be certified. The unchanged empty watchdog
log retains its earlier timestamp. This limitation prevents an unqualified
claim that every ancillary log was restored to a freshly hashed baseline.


### Reached keyed canvas reads, 26 October 2026

The four failures in `/tmp/c3r-live-integrate/owned/civ3-bridge.log` were two
reads each of the form's content canvas at `+0x274`, through `608FE3`.
They were not reads of the four frame caches. The executable's writer and
reader disassembly is retained in `/tmp/c3r-keyed-fix/`. The reader compares
`PCX_Image::get_pixel` with `COLOR_KEY & 0x7fffffff`. Base form setup creates
and clears `+0x274` at `602947..602960`, then calls virtual drawing methods.
Main_GUI's `+0x58` writer is `5583B0`; the domestic advisor's is `51F7E0`.
These dispatch the form-specific content, which the model already retained.
No missing fill, `+0x38` tile, frame line or virtual drawing hook needed to
be invented for either reported point.

- HUD `0x9fdd84`, `(1850,1010)`: `records_at` resolves a local key clear,
  box-right colour/mask region `[0,24,294,64]` at `(1621,966)`, and
  WorkerDefault FLC frame 33 at `(1729,969)`. The file sample `(229,68)` is
  colour index 230, RGB `[255,255,232]`, mask index 0. The animation sample
  `(121,41)` is outside its decoded `50x51` extent. Its absolute game-opened
  Windows path did not identify the viewer's cached relative art source.
  Read-back now retains the exact path resolution made by `ui_screen::items`
  with the active art roots, then queries that same decoded animation.
  The mask establishes a hit and the out-of-source animation leaves it so.
- F1 `0xa38e4c`, `(132,324)`: the relevant retained writer is the first
  city's italic black Lucida Sans -12 caption, `Alcácer do...`, at baseline
  `(111,329)`. The renderer places this run at `(559,485)` in the centered
  advisor, while the reader uses canvas coordinates. Exact run matching
  incorrectly treated those as different text sources. Text retention and
  lookup now use coordinates relative to the baseline. Both use the same
  loaded atlas or packed outline source and the renderer's coverage code.
  The reported sample stays the key, matching the original reader.

Owned canvases hold no authoritative legacy raster. An unanswered owned
semantic query now panics immediately with `no exact model answer`; it
cannot forward into placeholder image memory. The ownership opt-in makes
this an explicit unsupported-contract failure. Plain and pass-through
modes retain their original calls. Oracle comparison is disabled under
ownership even if its diagnostic flag is set. The diagnostic includes
`records_at`.

The two `text::keyed_read_tests` fixtures replay these coordinates, draw
records, sampled PCX values, animation extent/identity and measured caption
atlas cells. They test the same model and source adapters used in production,
without drawing a replacement canvas or reading a placeholder. The fixture
README records source hashes and its deliberately omitted unqueried data.

Live evidence in `/tmp/c3r-keyed-fix/index.txt` uses the shared WoW64 game,
R28 executable, the Roman 130 AD save and `input.alt_in_menus=false`.
The owned pass repeats the blank-HUD hover/click and F1 first-city hover/
double-click, city screens, HUD/minimap/diplomacy controls, F1-F8,
Civilopedia, menu and enabled Granary Iron badge. It has zero unanswered
owned reads. Original-jgl comparison answers both reported points with the
same key predicate. The HUD sweep has 129,600 matches, zero differences and
zero unknowns. The F1 sweep has 48,472 matches, 680 differences and zero
unknowns over its 49,152 samples. Its reported differences concern text
rasterization away from the requested point. This closes the two missing
source connections, not all F1 text raster equivalence or full Group 6.

The workspace test run passes except the known `tile-render` test target
`subpixel_coverage`; the generic-crate and no-lock guards pass. Host native
and i686 Windows release builds pass. Shared-file backup/restoration hashes,
activation, original-jgl mappings, logs and screenshots are kept with the
live evidence. No default switch, merge, push or deployment is part of this
correction.

# Remaining jgl object contracts

The later checkpoint `907d777` was withheld from integration. Its proposed
data/DC/mask changes do not close the rows below. The
[current handover](jgl-continuation.md) and
[source review](evidence/jgl-group6-review/review.json) record the new
identity, first-fill and self-copy defects, missing second frame reader,
and unverified scratch-DC behavior. The matrix below describes the earlier
`83caad2` checkpoint and its scoped evidence.

Group 7's separate [window/pump matrix](jgl-window-contracts.md) is accepted
for standalone original-binary comparisons at `5ca2cf6`; its integrated
gameplay gates still depend on completing the Group 6 rows below.

This is the group 6 contract audit for the continuation of `3f6dceb` on
`t3/finish-jgl-groups-switch-2`. It describes the experimental owned path.
Ordinary viewer launches still use original jgl. `[V]` identifies the
specific source inspection or test below; `[G]` identifies an inference.
A checked caller does not establish runtime coverage of every screen.

The new evidence root is `/mnt/datasets/c3r-build/jgl-t3-d94deaaf`.
`audit/inputs.sha256` binds the PCGames exe, C3X R28 exe/source and jgl.
`audit/get-pixel-sites.txt` preserves the contexts of all 24 direct calls to
`0x600220`; `audit/check-sites.py` checks each call's bytes and target against
the named exe, plus the four navigator calls. The scan remains a lower bound
for indirect calls and pointers
that escape through `lock`. Earlier Palace and lifetime evidence remains
at the roots documented in [the replacement record](jgl-replacement.md).

## Per-contract matrix

| Contract | Required behaviour and source | Current owned implementation or change | Scoped evidence and outstanding gate |
|---|---|---|---|
| Image construction, create/clear, getters, key and clip | ABI fields, row pitch, retained sizes on clear, same-size create preserving content, clip intersection, lock counters and owner clearing. No canvas drawing is required to answer these. | Own constructors and plain allocations exist. Clip/key/getters and pixel thunks are implemented. Clear retains allocated bytes where original leaves a dangling pointer. This is an intentional lifetime difference, not proven original behaviour after clear. | [V] Recovered binary semantics and layout tests. Earlier broad comparison has create/table mismatches retained in §9. A new exhaustive state/error comparison is still required; matching counters alone is insufficient. |
| City-screen region map, form `+0xC54` | Reads at `414FC9`, `4154C9`, `415719` return an 8-bit region index or `FF`. | Exe PCX decoder writes plain memory; `get_pixel` reads that memory. No drawing reconstruction should run. | [V] Reader addresses rechecked. Recovered research attributes the source to file data. Still needs source-load/writer provenance and live coordinate/index comparison for this family. |
| Region map at form `+0x662C` | Read at `4CCBF6`, index used by the form's interaction handler. | Same decoded-file/plain-memory answer. | [V] Reader address rechecked; source/writer and live family evidence still required. |
| Espionage region map, form `+0x3DC8` | Reads at `52B409`, `52B55E`, `52B629`, with `FF` meaning no region. | Same decoded-file/plain-memory answer. | [V] Reader addresses rechecked; source/writer and live family evidence still required. |
| Region map at form `+0x2B60` | Reads at `5835BA`, `583626`, `583A23`. | Same decoded-file/plain-memory answer. | [V] Reader addresses rechecked; source/writer and live family evidence still required. |
| Region map at form `+0x574` | Reads at `5A45DC`, `5A488F`, `5A5013`; indices `FC` through `FE` have special handler branches. | Same decoded-file/plain-memory answer. | [V] Reader addresses and special branches rechecked. Do not substitute a generic opaque/transparent answer. Source/writer and live family evidence still required. |
| Region map at form `+0x3958` | Reads at `5DEA2A`, `5DEA94`; the result selects an entry. | Same decoded-file/plain-memory answer. | [V] Reader addresses rechecked; source/writer and live family evidence still required. |
| Cursor hotspot reads | `54AFC5`/`54AFE9` scan the decoded cursor sheet for index `FB`. | Exe file decode and raw reads; sprites remain placeholders. | [V] Reader loops rechecked. Fresh sheet-byte and hotspot comparison remains required; existing cursor appearance tests do not by themselves establish this contract. |
| Oldinterface presence | `549FAD` tests whether each of three sprite-derived images exists. | Retained decoded source and sprite crop answer the index from the same `FilePixels` source used by the model. | [V] Prior three original comparisons returned `40` on both paths. Later source ownership comparison covers oldinterface without synchronous decode at first read. Not repeated here. |
| Palace selection | `57C706`, `57C7C4`, `57C7FA` read the eligible-piece ordinal. Geometry, clip, order and indices below `FE` decide game state. | Shared `Selection` and immutable decoded file handles; publisher computes anchors from the same Present's captured fields. | [V] Prior 14,515,200 cells, 15 reads, upgrades/style changes; later 23,713,540 source indices and four crops. Source retention and anchor corrections are preserved. Other saves/resolutions and initially credit-free entry remain separate coverage gaps. |
| Frame hit masks | `608E0A` reads form pointer `+0x11C`; **`608E7E` also reads `+0x128`**. Both compare with the colour key before accepting a frame hit. | General `get_pixel` currently falls back to raw owned bytes. These are allocated `PCX_Image` objects in `603Dxx`/`603Exx`, not established file sources. | [V] Both readers and allocation sites rechecked in `audit/frame-data-writers.txt`. The earlier single-reader/file-source guess is insufficient. Trace their writers and use shared frame/model geometry if they are drawn. No honest accepted answer yet. |
| Keyed form canvas | `608FE3` tests the form's `+0x274` canvas against the key. | `UiModel::shown_at` answers from the model and file pixels. | [V] Existing hit-test checks. Open limits remain text-only pixels, an unavailable file returning `Unknown`, and `NotMirrored` falling back to raw bytes. Owned mode must not silently accept those as exact answers. |
| Generated navigator copies and clear | `5784EC`/`578733`: terrain `+18` to grey-work `+588`; `578625`/`57886C`: grey-work `+588` to political `+2D0`. The exe then modifies and reads the bytes; `+858` is cleared to black before reveal. | **Changed:** host marks the four buffers of the known HUD/advisor navigators as caller-written data. Own `+40` uses safe row copies only when both ends have that contract; own `+44` preserves their 16-bit fills. Canvas targets remain record-only. | [V] Disassembly in `audit/navigator-create-reset.asm`. Original-binary test: 40 copy/clear calls, 1,369,360 bytes, zero differences, including padded rows and destination clips. Live buffer comparison is recorded below. Dynamic/replay navigators are not covered by this static-owner policy. |
| Escaped pixel pointers, file decode/crop, FLC | Decoders and crop trimming use the pixel/lock thunks and strides; callers write/read the bytes directly. | Plain image memory; sprite init keeps one striped allocation. | [V] ABI/decoder research. No new per-pixel hook was added. The lower-bound reader inventory still needs runtime coverage for these pointer families and the dotted-frame loops, independently of `get_pixel` counts. |
| C3X minimap dot and combat-odds background | C3X writes a city-dot pixel and saves/restores the underlying HUD bytes through `+1C`, sometimes without unlocking. | Plain buffer preserves these writes; navigator copies now retain the generated source. Viewer minimap/HUD rendering uses world/model state. | [V] C3X source. [G] Display-only saved bytes need not match the old HUD raster. Confirm no logical consumer and exercise the enabled C3X features; do not call an arbitrary blank read a verified answer. |
| Image `+38` tile-under | Only replace destination key pixels with the file tile through its palette, clipped to the requested frame piece. Caller `5FCE9C` is inside `5FCD40`, reached by frame painter `600520`. | Record exists; owned body returns zero without touching pixels. No new tile-under mirror is introduced. | [V] Recovered call graph and old counts. A record-only answer is justified only after checking the frame/keyed-hit consumers against shared frame geometry. The two frame-hit readers above prevent closing this contract now. |
| Image `+58` on loaded 8-bit art | `4C6ABA`/`4C6ACA` replace `0`/`1` with `FF` in `Art/Terrain/Territory.pcx`; `54A3A5` replaces `FD` in `Art/units/units_32.PCX`; `54ADBF` does so in `art/Advisors/foreignTAB.pcx`. | Safe `Indexed::replace` changes owned bytes. Model file-source metadata currently carries no replacement transform. `ui::indices::blank` already names Territory's `0`/`1` rule on the coloured-blit path. | [V] Call contexts and asset string VAs checked in `audit/checked-sites.json`. Need compare modified indices with original and verify each viewer draw/hit path uses the same transparency rule, including units_32 and foreignTAB. Raw-memory equality alone is insufficient. |
| Image `+58` on glyph canvases | Text renderer calls at `6230xx` through `6236xx` recolour 16-bit glyph pixels. | Owned `replace_color` currently changes only 8-bit images; 16-bit body returns success without work. Text commands already retain font/colour. | [G] Record-only may be sufficient for display, but every caller's later reads/copies and the resulting text command must be checked. Unsupported depth and absent-pixel error answers also differ. This contract is open. |
| Image `+3C`, PNG load/save, unobserved draws | Their error answers and side effects are defined by jgl even if previous sessions never called them. | `+3C` warns and returns success; PNG/PCX file slots return not implemented; several line/frame bodies return zero. | [V] Source inspection. These are explicit unsupported cases, not equivalent implementations. Keep them visible in acceptance scope and counting evidence. |
| Font measurement and resources | Real GDI metrics/extent determine game layout; HFONT and FOT lifetimes matter. | Dedicated real memory DC, GDI font/metrics/measurement, owned FOT lifetime and embedded-base destruction. A real DC is justified because the OS font engine supplies layout and viewer glyphs. It need not contain a game image. | [V] Prior 2,369,297 measurements with zero differences; later eight load/measure/release resource cycles and registry-order comparison. No settled font/registry work repeated here. |
| Image text-out and state/DC slots | Text command needs selected font, colour and clip. Any external DC consumer must receive its actual required state. | Producer retains text state; image text bodies are record-only, image DC fields are zero. | [V] Existing text-record path. Must establish text on every read-back image has a model answer; returning zero DC is not a blanket equivalence proof. |
| C3X GDI+ line initialization | `34479` directly reads image `+4BC`, calls `CreateFromHDC`, retains `gp_graphics`, and returns `0` or `2`. | Existing five line hooks retain model pen/canvas state, but bind still calls original and the owned image has no DC. A null DC changes initialization status. | [V] C3X source and current hook inspected. Decide between successful command-only binding with no GDI draw, or a small owned DC with a proven lifetime. A scratch bitmap for every image was rejected during this audit: it does not establish semantic status or the retained handle lifetime. |
| C3X mill icons | `34703`/`34705` acquire canvas/sheet DCs; `TransparentBlt` scales a file resource icon into a 24x24 destination. | Zero DC skips the original work. No equivalent native city-list icon source was found in this audit. | [V] C3X source. Need native icons derived from mill/resource state and the viewer's file art, plus an enabled-mill runtime case. A scratch destination DC alone would still leave the viewer icon missing. |

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

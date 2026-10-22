# Historical agent /root/retirement/retire_crash_review

This is a recovered historical report, not a live agent handle or a new instruction.

Reviewed underlay ownership/filtering across recursive surfaces/copies.

Current disposition: No concrete findings in final ownership fix; no runtime claim from review.

Session `01a14bbf-4c06-7073-bcc1-f9166715548d`, parent session `01a14bbe-4f0b-73c2-9dcb-c3498505b7e6`, depth 2. Started 2026-10-17T21:23:10.472Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-23-10-01a14bbf-4c06-7073-bcc1-f9166715548d.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T22:26:15.411Z

Original transcript line 1062.

One remaining P2: [live.rs:1339](/mnt/datasets/c3r-wt/retire/crates/civ3-bridge/src/live.rs:1339) keys cached text sprites on `fonts.len()`. A completed font reload and pruning in the same publishing round can preserve that count. For an already-described font, no atlas growth follows, so cached sprites can retain fallback glyphs or missing text despite the fixed completion guard.

Use a font revision in `art_key`, or invalidate text-sprite memos on completion and pruning.

The shader delta is confined to the requested opacity branch and uses existing `div255`. Export rounding, `mixed_source`, transfer APIs, profiling marks, and replay publication remain intact. Read-only review.

## Report at 2026-10-17T22:27:43.157Z

Original transcript line 1079.

No remaining findings in signed `5b02390`. Insertions and pruning now advance the font revision used by all text-sprite memos. The regression covers unchanged font count and atlas state. Read-only review; no builds or edits.

## Report at 2026-10-17T22:35:38.674Z

Original transcript line 1098.

Two correctness gaps remain in `e81e1aa`:

- A first direct write adds a new `Screen` layer. On a subsequent **partial** present, `present()` sees changed layer count and replaces the whole displayed composition. Reproduce with a fully presented green form, a full-buffer black write, then present only `[100,100,300,300]`. Pixel `(5,5)` incorrectly turns black. The regression introduces the screen layer with a full present, so misses this case.
- A later form composite outside a paint only updates its earlier `Form` layer. The retained `Screen` layer stays above it, hiding the newly composited form even though the game copied that form over the direct write. Sequence: present form, direct black write, present, composite that form again outside a paint, present.

Both need regressions covering direct writes interleaved with partial presentation and standalone form composition. No edits, builds, or UI runs.

## Report at 2026-10-17T22:41:37.892Z

Original transcript line 1182.

The current hook matches the executable and supports the initial-content hypothesis.

- `0x5FC5F0` takes six stack arguments, returns `int`, and uses `ret 0x18`. Zero means success. The nine-byte prologue is correct.
- It clears the old image, creates a fresh JGL object, then initializes it. The software JGL implementation calls `CreateDIBSection` with no shared section, consistent with fresh black contents.
- Recording `Size` and an unclipped opaque black `Fill` only after successful 16-bit creation preserves the original allocation and skipping behavior. Palette images remain excluded; generated surfaces remain renderer-owned.
- Existing keyed-form composition filters black fills, while ordinary opaque copies retain them.

No concrete defect found in the current `pcx_create` changes. The paired live check still needs to establish that this supplies the missing victory surround. No edits, builds, or UI runs.

## Report at 2026-10-17T22:51:15.679Z

Original transcript line 1218.

No findings in signed `c8c2fc5`. All three call-site byte sequences match the executable and resolve to `0x5FC5F0`. Separate hook modules preserve the six stack arguments and `ret 0x18` ABI. Each calls the original once, records initialization after success, and returns before the caller triggers redraw.

My earlier review missed the inline relocation restriction. This implementation leaves the original entry untouched and removes that fault.

Read-only review; no edits, builds, or UI runs.

## Report at 2026-10-17T22:53:36.208Z

Original transcript line 1234.

No findings. The fix preserves the first opaque fill without changing draw order or exceeding `MAX_OPS`. At the cap, retaining that fill leaves 4,098 operations after the new push, below 8,192.

Clears and full-cover replacements still discard obsolete contents. The stress regression covers repeated pruning and backdrop retention. Read-only review.

## Report at 2026-10-17T23:14:33.202Z

Original transcript line 1268.

The source confirms why the arc can reappear. `skip_canvases` contains only the unit overlay; the shared map/title canvas is handled separately through `underlays`. `set_skipping(false)` currently restores both categories.

The overlay's retained mirror is incomplete: while skipping, `Event::Copy` discards copies from the map canvas before applying their occlusion. Those copies can erase older cursor/unit drawings in the game, while the mirror retains their FLC operations. Revealing that cached overlay can therefore expose stale content. Its draw method is a no-op, so it does not rebuild itself on composition.

If the paired reference confirms the arc is absent, the minimal generic fix is to keep explicitly renderer-owned `skip_canvases` excluded regardless of `set_skipping`, including copy propagation. Leave `underlays` conditional so title text remains available. This follows canvas ownership without checking end-screen classes.

I found no evidence yet supporting a specific missing end-game reset. No edits, builds, or UI runs.

## Report at 2026-10-17T23:28:21.986Z

Original transcript line 1289.

No findings in the ownership fix. Filtering stays scoped to declared underlay canvases and survives recursive surface resolution and copies. Ordinary forms can still draw the same file.

The regression covers mode handover, copied fog removal, and preservation of title art, text, black fills, and unrelated forms. Operation/depth bounds remain unchanged; ordinary canvases avoid the filename filtering work. No edits, builds, or UI/GPU runs.

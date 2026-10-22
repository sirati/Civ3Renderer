# Historical agent /root/unit_notices

This is a recovered historical report, not a live agent handle or a new instruction.

Native message expiry audit and Windows-1252 Unicode correction.

Current disposition: Continued by unit_notices_sol; older concurrent build blockers are historical.

Session `01a14f57-54ce-78b1-b8a5-c28e5b579bb0`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T14:08:05.841Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-08-05-01a14f57-54ce-78b1-b8a5-c28e5b579bb0.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:18:54.410Z

Original transcript line 354.

Implemented missing unit/map notices.

- Reads localized text, exact font/style, tile position, and lifetime from engine records.
- Draws plate and text in all world views, including B outside A.
- Removes expired messages and prevents duplicate UI copies.
- Honors each view's Clean Map Units mask.

Passed 3 sys regressions and 4 notice regressions. Initial i686 check passed; subsequent check hit concurrent `inproc/message.rs` errors already reported to its owner.

Live reproduction steps are in [unit-map-notices.md](/home/sirati/devel/rust/civ3-renderer/docs/unit-map-notices.md). Live game verification remains required.

## Report at 2026-10-18T14:23:53.251Z

Original transcript line 446.

Corrected semantic notice text to real Windows-1252 Unicode. Added accent, quote, dash, and euro regressions, plus conversion to renderer glyph keys only at the font boundary.

Expiry remains active with original rendering disabled. The animator's visibility branch skips pixel drawing, then always checks `timeGetTime` at `0x4EF3DA` and removes expired messages through `0x4EF3F7 → 0x4F05B0`. Renderer suppression preserves this pass. Evidence is documented in `docs/unit-map-notices.md`.

Latest checks are blocked by concurrent `world_input.rs` missing function modules; its owner was notified. Earlier 7 notice tests passed before these added encoding regressions.

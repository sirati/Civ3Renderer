# Historical agent /root/unit_visibility

This is a recovered historical report, not a live agent handle or a new instruction.

Collects units outside the native A camera through native selection, fog and detection rules.

Current disposition: Targeted tests passed; full current B-view runtime acceptance open.

Session `01a14f39-3f63-7813-88f5-63960cedb276`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T13:35:14.278Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-35-14-01a14f39-3f63-7813-88f5-63960cedb276.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T13:43:10.581Z

Original transcript line 239.

Implemented B-view unit collection.

Cause: `snapshot()` used only `Animator.Units`, which contains units inside the game's A camera. B had no unit data outside that area.

The fix collects other occupied tiles on the game thread using the game's own stack selection, fog, shared-vision and invisible-unit detection functions. It preserves moving-unit defenders and draw order. Bulk memory reads limit collection overhead.

Changed:
- New bridge and sys `unit_views.rs` modules.
- Module exports, `live.rs` integration and sys world readers.
- Rendering documentation.

Validation:
- Bridge: 202 tests passed.
- Sys: 34 tests passed.
- Six focused B-view regressions passed again after final edits.
- Windows i686 check passed.
- Hook signatures and prologues checked against installed game binaries.
- `git diff --check` passed.

Live Wine validation remains for combined QA. Logs now report `world units: N in the game camera, M visible across the map`, allowing confirmation that M exceeds N with off-camera units.

No commit, deployment, assets or laptop process changes.

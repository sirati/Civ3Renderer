# Historical agent /root/scale_review

This is a recovered historical report, not a live agent handle or a new instruction.

Menu-pan cancellation, release hover mapping and camera redraw/presentation fixes.

Current disposition: Focused scaling passed; early zoom-box failures handed to owners, not a current full pass.

Session `01a14f3d-a770-7582-a01d-c3e671add208`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T13:40:03.059Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-40-03-01a14f3d-a770-7582-a01d-c3e671add208.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T13:44:12.395Z

Original transcript line 190.

Implemented review fixes:

- Cancel menu panning when the pointer leaves the window.
- Restore hover after middle-drag release using the displayed menu coordinates.
- Redraw camera-only changes on software renderers; retain previous cameras until presentation succeeds.

Scaling tests pass **10/10**. Full viewer tests: **185 passed, 2 failed, 2 ignored**. Both failures concern zoom-box hit geometry, already reported to root. `git diff --check` passes. No commit made.

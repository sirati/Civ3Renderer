# Historical agent /root/world_input

This is a recovered historical report, not a live agent handle or a new instruction.

Independent world input and stationary-pointer refresh after accepted presentation.

Current disposition: Focused tests and viewer suite passed at its checkpoint; live input matrix open.

Session `01a14f59-29f4-7d22-955e-035da24ec1c8`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T14:10:05.945Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-10-05-01a14f59-29f4-7d22-955e-035da24ec1c8.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:51:54.222Z

Original transcript line 1083.

Implemented virtual world input across A/B, including G targeting beyond the engine camera, native selection/context menus, safe preview confirmation/cancellation, and parked-camera capability routing.

Validation passed: viewer 275 tests, bridge 253, sys 59, scene 6, i686 bridge check, and a real Wine message-order/cleanup test. Live gameplay QA remains; reproduction steps are in [world-input.md](/home/sirati/devel/rust/civ3-renderer/docs/world-input.md).

Camera observer and local wheel work are handed to their owners. No commit, deployment, laptop UI interaction, or game-file copies performed.

## Report at 2026-10-18T15:06:37.533Z

Original transcript line 1345.

Stationary-pointer refresh is complete and ready to freeze. G targeting and held right-drag previews now follow camera movement; released destinations stay fixed. Static frames, captured cursor following, UI, snapping and active map drags avoid duplicate input.

Refresh runs after successful presentation and skips failed acquisitions or pending-art replays.

Validation: 7 focused tests passed, full viewer 283 passed with 3 ignored, i686 bridge check passed, and `git diff --check` passed. Include the new [world_pointer.rs](/home/sirati/devel/rust/civ3-renderer/crates/tile-viewer/src/world_pointer.rs).

# Historical agent /root/native_alt_sliders

This is a recovered historical report, not a live agent handle or a new instruction.

Native slider transaction routing clears stale world input before callbacks.

Current disposition: Seven tests and i686 check passed at its checkpoint.

Session `01a14f6a-2849-7413-afee-0ad0bf2ad41e`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T14:28:39.628Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-28-39-01a14f6a-2849-7413-afee-0ad0bf2ad41e.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:39:47.230Z

Original transcript line 345.

Implemented native Alt slider transactions with exact cancel restoration, real slider geometry, game-thread updates, and stale-control guards.

Validated: 7 transaction tests, 27 controls tests, scene export regression, and i686 bridge check. Binary evidence is in `docs/native-alt-sliders.md`. Functional UI checks remain with integration QA.

## Report at 2026-10-18T14:57:17.891Z

Original transcript line 383.

Added `world_input::leave_world()` before dispatching recognized native slider messages, clearing stale map-pointer state before callbacks.

All 7 slider tests and the i686 bridge check pass.

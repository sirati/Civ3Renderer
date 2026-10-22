# Historical agent /root/world_input/engine_mouse

This is a recovered historical report, not a live agent handle or a new instruction.

Native independent-camera input hooks, key dispatch, city-focus protection and stale-target clearing.

Current disposition: 48 sys tests passed; parent completed later integration.

Session `01a14f5a-0b22-7693-83ae-457c232f87c0`, parent session `01a14f59-29f4-7d22-955e-035da24ec1c8`, depth 2. Started 2026-10-18T14:11:03.591Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-11-03-01a14f5a-0b22-7693-83ae-457c232f87c0.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:33:56.057Z

Original transcript line 591.

Implemented and validated the standalone native input module at [world_input.rs](/home/sirati/devel/rust/civ3-renderer/crates/civ3-sys/src/world_input.rs).

It includes:

- Virtual tile override hook for `0x4E3D20`.
- Native hover, left/right click, release, and double-click paths.
- Native key dispatch through `0x4E06B0`.
- Scoped suppression of off-camera recentering through `0x4DFAA0`.
- City-focus protection using pointer `0x9CB3F4`.
- Invalid-tile mode to clear stale targeting.
- Exact PCGames addresses, prologues, and validation tests.

`cargo test -p civ3-sys` passes all 48 tests. i686 Windows checking remains blocked by the unrelated existing missing `controls.rs::setup` module.

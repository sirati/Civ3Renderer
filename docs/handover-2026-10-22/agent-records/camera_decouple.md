# Historical agent /root/camera_decouple

This is a recovered historical report, not a live agent handle or a new instruction.

Parks native world camera, publishes semantic AI focus and keeps animations independent; restores native fallback on viewer loss.

Current disposition: Source tests and binary audit passed; live AI/fallback/populated-map acceptance open.

Session `01a14f6f-fbcd-72f3-a1e6-523c5da1612e`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T14:35:01.456Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-35-01-01a14f6f-fbcd-72f3-a1e6-523c5da1612e.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T15:17:33.147Z

Original transcript line 1362.

Implemented and frozen.

- Native main camera parks at `(0,0)` with tile bounds `-1..5`, after input, animation and AI hooks are ready. City/UI geometry stays native.
- Explicit focus events drive A independently. AI movement, bombardment and combat use native visibility, detection, shared vision and preference rules.
- Independent units/effects retain native animation timing and membership.
- Viewer loss disables parking permanently and restores full native rendering/camera on the game thread.
- Selected-unit focus now survives cursor blinking.

Passed: 74 sys tests, 8 scene tests, selected-focus regression, Windows compilation; animation child passed 11 bridge tests, 2 Wine/i686 tests and 36 binary checks.

Final AI source blob: `451b351d9c01b2f8a20f83b547996e32d4124ec7`. Only `crates/civ3-sys/src/ai_camera_focus.rs` changed after my earlier freeze notice.

Evidence/logs: `/mnt/datasets/c3r-build/camera-decouple/`. Documentation: `docs/independent-camera.md` and `docs/independent-animations.md`.

Real-game QA remains for AI focus, off-camera animations/input, City transitions, viewer-close fallback and populated-map performance. No GPU/UI sessions, laptop changes or commits performed.

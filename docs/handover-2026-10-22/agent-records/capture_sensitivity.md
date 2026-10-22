# Historical agent /root/capture_sensitivity

This is a recovered historical report, not a live agent handle or a new instruction.

Configurable captured relative sensitivity with fractional preservation and scale tests.

Current disposition: Targeted settings/Preferences tests passed; live ratios/focus boundaries open.

Session `01a14f4b-547c-7dc1-8158-6487d5fe22ea`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T13:54:59.327Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-54-59-01a14f4b-547c-7dc1-8158-6487d5fe22ea.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:02:13.183Z

Original transcript line 277.

Implemented captured mouse sensitivity in Preferences → Layout.

- Persisted `input.capture_sensitivity`, default `1.0`, slider `0.1x–5x`.
- Updates live and scales captured relative motion once, preserving fractional movement and independent window/world zoom conversions.
- Free pointer, map gestures, absolute edge pointer and menu snapping remain unchanged.
- Passed 16 settings tests, 13 Preferences tests and 4 sensitivity regressions covering 96 scale combinations.
- `git diff --check` passes.

No commits or deployment performed. Integration with `capture_edge` is coordinated.

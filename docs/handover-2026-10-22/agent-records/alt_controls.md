# Historical agent /root/alt_controls

This is a recovered historical report, not a live agent handle or a new instruction.

Native Alt targets for status orb, unit portrait and cargo icons.

Current disposition: Focused source/platform tests passed; current full-game Alt acceptance open.

Session `01a14f63-fbef-78d1-be4f-e159bfdd1fba`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T14:21:55.058Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-21-55-01a14f63-fbef-78d1-be4f-e159bfdd1fba.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:41:21.667Z

Original transcript line 511.

Implemented missing Alt targets for the status orb, unit portrait, and unit/cargo selection icons. Native coordinates and handlers remain authoritative, including C3X end-turn checks. Orb hover/pressed artwork now works when its image starts outside the clickable rectangle.

Verified:

- All 57 `civ3-sys` tests.
- i686 Windows `civ3-sys` check.
- Three focused bridge/viewer status tests, covering every split, pixel scales 1–6, and frame scales 50–200%.
- All eight bridge appearance tests.
- Diff whitespace checks.

Details: `docs/alt-status-controls.md`. Tests: `/mnt/datasets/c3r-build/alt-status-{tests,sys-tests,sys-i686,looks-tests}.log`.

Native unit eligibility remains checked when clicked. Live UI verification remains for your combined QA. No commits, deployment, GPU use, laptop interaction, or asset changes.

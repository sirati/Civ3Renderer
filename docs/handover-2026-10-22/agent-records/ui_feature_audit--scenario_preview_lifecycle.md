# Historical agent /root/ui_feature_audit/scenario_preview_lifecycle

This is a recovered historical report, not a live agent handle or a new instruction.

Publishes complete scenario menu before optional preview work; cancels/drains stale selection and clears absent world.

Current disposition: 9 preview and4 scheduler tests plus i686 check passed; rapid-switch runtime acceptance open.

Session `01a153ed-8ec4-7b43-b63a-4724f9b6c23c`, parent session `01a152c3-ba33-7850-9319-cbe1ebbddcc3`, depth 2. Started 2026-10-19T11:30:39.943Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T13-30-39-01a153ed-8ec4-7b43-b63a-4724f9b6c23c.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T11:42:04.884Z

Original transcript line 352.

Implemented bridge lifecycle fixes in `live.rs`, `preview.rs`, and `art_schedule.rs`.

- Publish the complete scenario menu before starting optional preview atlas work, and wait for required menu assets to upload first.
- Show preview terrain only after matching scenario assets, unit frames, palettes, and labels are available. Pending or failed previews publish empty world shape, `Beside.world=None`, and no capture hint.
- Clear old selections immediately. Closing the screen cancels work, drains queued results, and restores base title art folders.
- Keep replacement whole-atlas requests pending for mandatory-art readiness, preventing the transient missing-art diagnostic.

Verified 9 preview tests, 4 scheduler tests, Windows i686 bridge compilation, and `git diff --check`. No GUI or physical Mac actions. No protocol changes.

The viewer must preserve the complete menu when the beside world is absent; your layout fix covers that behavior.

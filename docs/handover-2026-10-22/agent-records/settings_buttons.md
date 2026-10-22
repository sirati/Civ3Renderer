# Historical agent /root/settings_buttons

This is a recovered historical report, not a live agent handle or a new instruction.

Preferences placement inherits Menu panel, clips its background and retains tabs/popups.

Current disposition: Implementation recorded; later successor settings work supersedes early test blockers.

Session `01a14f3d-4221-79d2-87e0-a0b02e745bb5`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T13:39:37.123Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-39-37-01a14f3d-4221-79d2-87e0-a0b02e745bb5.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T13:42:09.637Z

Original transcript line 135.

Fixed the three map buttons appearing over Preferences.

- Preferences now suppresses roaming map controls and restores them after closing.
- Added regression tests for opening, closing, and popups over Preferences.
- All 9 interface tests passed.
- Windows i686 bridge check passed.

Changes are in `interface_parts.rs` and the HUD selection in `live.rs`. No commit or deployment performed.

## Report at 2026-10-18T13:52:00.216Z

Original transcript line 316.

Menu and Preferences placement is ready.

- Preferences inherits `Menu` placement and uses its exact 1024×768 panel.
- Underlying menus are excluded; HUD stays separate.
- Generated tabs and overlaid popups remain intact.
- Full-screen Preferences background is clipped to the panel.
- All 207 bridge tests passed.

Latest Windows check hit an unrelated concurrent syntax error in `clean_map.rs:44`; root was notified. No commit or deployment performed.

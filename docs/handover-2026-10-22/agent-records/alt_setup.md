# Historical agent /root/alt_setup

This is a recovered historical report, not a live agent handle or a new instruction.

Alt navigation for title/New Game groups and editable seed using native hit maps.

Current disposition: Focused tests passed; live setup acceptance open.

Session `01a14f64-f10a-73a1-94f6-b47779c6431e`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T14:22:57.807Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-22-57-01a14f64-f10a-73a1-94f6-b47779c6431e.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:43:48.578Z

Original transcript line 609.

Implemented Alt targets for the title menu and all New Game option groups, including the editable map seed.

Native setup targets now come from the game's actual hit maps, so disabled choices remain excluded. Title targets use native positions and measured translated labels. Direct radio/grid controls are also recognized.

Changed:
- `civ3-sys/src/controls.rs`, `controls/setup.rs`, and control tests.
- `civ3-bridge/src/setup_controls.rs`, plus live integration.
- `docs/alt-setup-controls.md`.

Validation: 64 sys tests passed; 253 bridge tests passed before the final textbox addition; three dedicated bridge regressions cover passive startup, translated labels, and native click routing across ×1–×6 UI scales. `git diff --check` passes.

A previous i686 bridge check passed. The latest combined check encounters another agent's unfinished `Layout::unit_orders_rescale_world` integration. Live UI verification remains for combined QA. No commits, deployment, assets, or laptop operations performed.

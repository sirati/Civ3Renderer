# Historical agent /root/unit_drag_orders

This is a recovered historical report, not a live agent handle or a new instruction.

Civ IV style order gestures and rescaling of pending destinations without synthetic confirmations.

Current disposition: 19 order tests passed; current runtime acceptance open.

Session `01a14f5f-d25d-7f60-87de-d04876ec08d5`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T14:17:22.274Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-17-22-01a14f5f-d25d-7f60-87de-d04876ec08d5.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:28:23.816Z

Original transcript line 333.

Implemented right-drag unit orders in `MapMovement::Drag`.

- Short right click opens the normal unit menu.
- Right drag requests native G route previews.
- Release never moves the unit.
- Separate left click requests guarded confirmation.
- Escape, Ctrl, focus loss, UI crossing, and conflicting gestures cancel safely.
- Free, captured, zoomed, and distant views are covered.
- Existing map dragging remains intact.

Added `Input::UnitOrder`, wire version 34, settings hints, and README instructions. Native confirmation is coordinated with `world_input`.

Validation: 16 gesture tests, 6 scene tests, and 17 settings tests passed. `git diff --check` passed. Broader viewer testing exposed only the previously reported city/menu positioning assertion, expected 960 versus actual 958.

## Report at 2026-10-18T14:47:34.252Z

Original transcript line 424.

Added `Layout::unit_orders_rescale_world(ratio)` and three regressions.

It rebases pending destinations, gesture world coordinates, captured threshold coordinates, and queued order coordinates. Physical pointer positions stay unchanged. Scale changes generate no preview or confirmation.

All 19 unit-order tests passed. `git diff --check` passed.

Log: `/mnt/datasets/c3r-build/unit-orders-rescale-tests.log`

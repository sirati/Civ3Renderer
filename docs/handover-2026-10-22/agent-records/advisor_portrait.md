# Historical agent /root/advisor_portrait

This is a recovered historical report, not a live agent handle or a new instruction.

Transfers popup-owned portraits with their parent position, scale and bounds.

Current disposition: 14 interface-parts tests passed; live popup acceptance open.

Session `01a14f47-6fa3-7b02-8e87-fa9d0982a837`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T13:50:44.136Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-50-44-01a14f47-6fa3-7b02-8e87-fa9d0982a837.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:03:02.635Z

Original transcript line 366.

Fixed detached advisor portraits in `interface_parts.rs` and `live.rs`.

Known advisor popup pictures now transfer from map/HUD drawing into the popup, sharing its position and scale. Popup bounds include the portrait; underlying advisor pictures keep their existing owner.

All 14 interface-parts tests pass, including CPU rendering checks for the top strip, left/right panes, pixel scales 1–3, and menu scales 1–1.5.

Log: `/mnt/datasets/c3r-build/advisor-portrait-tests.log`. Live UI verification remains with integration QA.

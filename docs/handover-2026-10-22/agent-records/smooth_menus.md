# Historical agent /root/smooth_menus

This is a recovered historical report, not a live agent handle or a new instruction.

Optional clipped intermediate row scrolling with correct visible-row hit mapping and release cancellation.

Current disposition: Focused CPU/GPU checks passed; later F1 user regression requires full-game acceptance.

Session `01a14f3f-8d89-7bd3-93d3-65c059902b28`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T13:42:07.499Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-42-07-01a14f3f-8d89-7bd3-93d3-65c059902b28.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:00:54.322Z

Original transcript line 486.

Implemented smooth menu list scrolling in `smooth_menus.rs` with narrow `lib.rs` hooks and README documentation.

- Layout option: “Smooth scrolling in menu lists,” default off.
- Confirmed rows animate through clipped intermediate pixels over 140 ms.
- Hover and clicks follow visible row IDs. Departing rows cannot select adjacent entries.
- Forwarded presses always receive a cancelling release if their row disappears.
- Menu panning restores hover through the same mapping.
- Large jumps without shared rows change immediately; Alt snapping retains its existing behavior.

Validation: 14 focused tests passed, explicit AMD RADV GPU comparison passed at three scales, full viewer suite passed **219 tests**, and `git diff --check` passed.

Logs: `/mnt/datasets/c3r-build/smooth-menu-{tests,gpu-tests,full-viewer-tests}.log`.

Live production-popup QA remains for combined testing. No commit or deployment performed.

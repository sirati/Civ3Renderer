# Historical agent /root/edge_scroll_speed

This is a recovered historical report, not a live agent handle or a new instruction.

Edge-scroll behavior and focus arbitration so stale AI focus cannot override later selection.

Current disposition: Viewer tests passed; later captured AI keep-visible policy belongs to army_units.

Session `01a14f6a-64a9-7bb0-82b8-caff2e875167`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T14:28:55.084Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-28-55-01a14f6a-64a9-7bb0-82b8-caff2e875167.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:53:49.051Z

Original transcript line 712.

Fixed A/B edge-speed mismatch and A snapping after smooth pans. Both now use the same 600 window-pixel/second scrolling.

Added semantic A focus, stable Z transitions, and local wheel scrolling with native speed settings. B stays independent.

Verified: 277 viewer tests, 8 scene tests, Windows target check, and diff check passed. Three GPU tests remain ignored for root QA. No commit or deployment performed.

## Report at 2026-10-18T15:03:10.906Z

Original transcript line 781.

Enabled edge scrolling while a right-drag unit preview is held. Releasing stops scrolling and preserves the destination.

Full viewer suite: 282 passed, 3 ignored. README updated; no commit made.

## Report at 2026-10-18T15:07:22.767Z

Original transcript line 847.

Fixed stale AI focus overriding later unit selections in A. New AI requests still take priority; later unit selection resumes unit following. B and explicit Driver mode remain unchanged.

Full viewer suite: 286 passed, 3 ignored. Source frozen; no commit.

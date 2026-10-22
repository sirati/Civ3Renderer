# Historical agent /root/menu_active_view

This is a recovered historical report, not a live agent handle or a new instruction.

Menus retain their opening view, scale and clipped hitboxes while oversized panels can pan.

Current disposition: Focused tests passed; full current UI acceptance open.

Session `01a14f57-8d32-7873-9e65-107229c5af12`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T14:08:20.278Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-08-20-01a14f57-8d32-7873-9e65-107229c5af12.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:28:03.327Z

Original transcript line 485.

Implemented mouse-view menu placement, including captured input. Menus retain requested UI scale, stay in their opening view, and support middle-drag panning with clipped rendering and hitboxes. Explicit screen assignments remain available.

Passed 511 focused tests. GPU clipping test compiles; GPU execution remains for your AMD validation.

Logs: `/mnt/datasets/c3r-build/menu-active-view-{render-tests,bridge-tests,gpu-compile}.log`.

No commits, deployment, or laptop changes.

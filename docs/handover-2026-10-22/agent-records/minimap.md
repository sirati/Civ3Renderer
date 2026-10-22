# Historical agent /root/minimap

This is a recovered historical report, not a live agent handle or a new instruction.

Independent Geography/Territory sources and immediate minimap invalidation for HUD/advisor.

Current disposition: Targeted bridge/sys tests passed; current feature acceptance open.

Session `01a14f39-5d0d-7fa0-bfa4-265c89938ddb`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T13:35:21.871Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-35-21-01a14f39-5d0d-7fa0-bfa4-265c89938ddb.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T13:40:53.735Z

Original transcript line 230.

Implemented in `ui_screen.rs` and narrow sections of `layout.rs`.

Cause: replacement compared complete edge rectangles, but the real game excludes different corner pixels from each edge. None matched, so the full-screen outline remained. Matching now accepts contained line segments immediately after the minimap, preserving unrelated white UI fills.

A’s reported rectangle now comes from its actual camera, including resolution scaling and smooth movement.

Tests passed:
- 3 bridge minimap tests, including captured game coordinates and wrapped fragmented edges.
- 76 layout tests.
- `git diff --check`.

The duplicate-suppression test issue you observed is fixed. Logs: `/mnt/datasets/c3r-build/minimap-{camera-after,layout-after,after}.log`.

No commit or deployment performed.

## Report at 2026-10-18T14:26:07.133Z

Original transcript line 390.

Fixed Geography/Territory switching.

The renderer always generated Territory pixels and never read the engine’s selected source. Verified installed executable at `0x579E70`: navigator `+4` switches between Geography `+0x18` and Territory `+0x2D0`.

Changes:
- Snapshot both HUD and advisor modes independently.
- Geography shows bright terrain; Territory shows ownership tint and city squares.
- Mode changes invalidate minimap patches immediately, without rebuilding unrelated world layers.
- Preserve modes in traces and keep previous A/B viewport fixes.

Validation: 229 bridge tests and 44 system tests passed; i686 Windows check passed. New tests cover both switching directions, independent advisor mode, multiple owners, ocean, unclaimed land, exploration, and cities.

Changed `minimap.rs`, `ui_screen.rs`, `world.rs`, `live.rs`, `trace.rs`, system `ui_hooks.rs`, and rendering docs. Logs: `/mnt/datasets/c3r-build/minimap-mode-{all-tests,i686,disassembly}.txt` except test/check logs use `.log`.

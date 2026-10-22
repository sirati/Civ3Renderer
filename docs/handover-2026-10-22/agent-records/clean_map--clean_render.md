# Historical agent /root/clean_map/clean_render

This is a recovered historical report, not a live agent handle or a new instruction.

Per-camera 13-category filtering and cache invalidation in renderer/reference paths.

Current disposition: Strict AMD suites passed at its checkpoint; current feature acceptance separate.

Session `01a14f45-253b-7471-8b4a-1cb7210c9a07`, parent session `01a14f44-97a0-7c80-8b29-beb9d044a8c1`, depth 2. Started 2026-10-18T13:48:14.015Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-48-14-01a14f45-253b-7471-8b4a-1cb7210c9a07.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:25:10.238Z

Original transcript line 925.

Clean-map rendering is implemented across CPU and GPU paths:

- Per-camera `hidden_categories` mask.
- Independent category filtering for cells, world/screen sprites, and glyphs.
- 13 public category bits with render-description layer support.
- Scene wire format updated to version 32.
- Cache invalidation when masks change.
- Tests cover all categories, cell/world sprites, glyphs, cache toggling, and GPU/reference parity.

Strict AMD RADV Vulkan tests passed serially: all tile-render suites and 226 civ3-bridge tests.

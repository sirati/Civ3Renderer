# Historical agent /root/clean_map

This is a recovered historical report, not a live agent handle or a new instruction.

Per-camera Game/Normal/Custom masks, 13 categories and separate district-resource layers.

Current disposition: Renderer/scene/bridge checks passed; PCGames offsets verified, other executables have scope limits.

Session `01a14f44-97a0-7c80-8b29-beb9d044a8c1`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T13:47:37.765Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-47-37-01a14f44-97a0-7c80-8b29-beb9d044a8c1.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:20:42.982Z

Original transcript line 1012.

Clean-map changes are ready for integration.

- Original Clean Map enable state and all 13 flags now affect inherited views.
- Preferences → Clean map has separate A, B and Zoom controls. **Game** follows Civ III; **Normal** shows all detail; **Custom** hides checked categories. Choices persist under `clean_map.a`, `.b`, `.zoom`.
- Filtering happens per camera without removing shared world data. Hills, city labels/icons, units, shadows and C3X resources remain independently selectable. Divider fillers preserve their pane’s mask; full-detail zoom preserves its own mask.
- C3X resources beneath districts use separate resource layers. Animated effects retain the original game’s behavior.
- Scene wire version is now **32**. Deploy bridge and renderer together.

Validation passed:

- 99 renderer tests on AMD RADV/Vulkan, including all 13 categories, glyphs and cache hide/restore.
- 6 scene/wire tests.
- 3 bridge clean-map tests, the C3X resource reference test, per-view layout regression and all 17 settings tests.
- i686 Windows bridge check and `git diff --check`.

Evidence: [renderer tests](/mnt/datasets/c3r-build/tile-render-radv-tests.log), [wire tests](/mnt/datasets/c3r-build/clean-map-scene-wire-tests.log), [engine evidence](/mnt/datasets/c3r-build/clean-map-engine-evidence.txt). User documentation is [clean-map.md](/home/sirati/devel/rust/civ3-renderer/docs/clean-map.md).

PCGames engine offsets are disassembly-verified; GOG/Steam offsets derive from C3X tables and remain unverified against those binaries. Final combined checks remain with root after the other agents finish.

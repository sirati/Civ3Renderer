# Historical agent /root/clean_map/clean_engine

This is a recovered historical report, not a live agent handle or a new instruction.

Audited native flags and district/resource category separation.

Current disposition: Parent corrected category ownership; no runtime claim from review.

Session `01a14f46-350f-7593-a29f-7d36d91335b2`, parent session `01a14f44-97a0-7c80-8b29-beb9d044a8c1`, depth 2. Started 2026-10-18T13:49:23.606Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-49-23-01a14f46-350f-7593-a29f-7d36d91335b2.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T13:54:51.332Z

Original transcript line 229.

Added `Civ3::clean_map()` and `clean_map_state()` with all 13 category mappings and separate enabled state. All 37 civ3-sys tests pass.

PCGames offsets verified by disassembly. GOG/Steam offsets derive from C3X tables and remain unverified against those binaries. Evidence saved in `/mnt/datasets/c3r-build/clean-map-engine-evidence.txt`.

## Report at 2026-10-18T14:12:33.492Z

Original transcript line 360.

Review found two category issues:

- `victory_location` belongs to CITIES. Parent has corrected it; non-city `victory_point` remains BUILDINGS.
- Resources beneath C3X districts with `draw_over_resources` enter `district_0..5`, so they still inherit BUILDINGS. They need separate RESOURCES layers before district sprites.

TNT/yields, hills, mountains, pollution and craters match engine masks. City labels/HUD are tagged consistently.

Global Hide Units returns before drawing and does not clear `DrawnUnit.visible`; retain that visibility check. Volcano and city effect categories are sensible extensions, since original effect rendering ignores Clean Map flags. No edits made during review.

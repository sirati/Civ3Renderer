# Historical agent /root/ui_background_dither/background_metadata

This is a recovered historical report, not a live agent handle or a new instruction.

Identified 55 pure texture rectangles across13 sources, excluding controls/bevels/text.

Current disposition: Read-only metadata delivered; title art remains nearest.

Session `01a15412-9b62-79f1-be64-6828b15bb6df`, parent session `01a15411-6a53-7662-846d-d8778b930330`, depth 2. Started 2026-10-19T12:11:08.004Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T14-11-08-01a15412-9b62-79f1-be64-6828b15bb6df.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T12:15:36.920Z

Original transcript line 239.

Implemented in `crates/menu/src/screen.rs`.

- Tags only proven parchment texture crops for background dithering.
- Excludes text, controls, borders, mixed art, masked/remapped images and resized art.
- Preserves source crops, sprite identity and preload serialization.
- All 25 menu tests pass, including two new metadata tests.

## Report at 2026-10-19T12:39:14.485Z

Original transcript line 380.

Read-only audit confirmed:

- Civilopedia `[226,130,418,570]` and its contained crops are pure parchment. Full bordered crop stays nearest.
- All requested popup interior strips, including width 36 and central `[313,46,60,44]`, contain no borders, shading, icons or text.
- Additional exact advisor texture crops are documented in messages; matching coordinates in other advisors can contain separator lines or arrows.

Decoded native crops and enlarged comparisons are in `/tmp/c3r-background-crop-audit/`. No sources changed.

## Report at 2026-10-19T13:02:22.247Z

Original transcript line 560.

Audit complete. Final data is in `/tmp/c3r-background-crop-audit/split-audit/verified-pure-rectangles.json`.

It contains 55 verified texture rectangles across 13 sources, with exact asset paths, SHA256 hashes and native crop images. Preferences controls, panel bevels, advisor separators/arrows and Civilopedia rules remain outside those rectangles.

Base, PTW and Conquests title art has no useful pure patterned UI regions; keep it nearest. No sources changed.

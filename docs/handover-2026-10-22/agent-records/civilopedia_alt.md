# Historical agent /root/civilopedia_alt

This is a recovered historical report, not a live agent handle or a new instruction.

Civilopedia/scenario anchors and clipping that excludes hidden controls across panes.

Current disposition: Focused package suites passed; later wire revisions supersede historical version 35.

Session `01a14f65-2153-7f60-a5e9-7fb4d059d771`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T14:23:10.167Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-23-10-01a14f65-2153-7f60-a5e9-7fb4d059d771.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:43:36.544Z

Original transcript line 643.

Implemented Civilopedia/scenario Alt anchors and central UI clipping.

- Icon rows point at the icon corner; text rows point at rendered text’s end.
- Rich-text links use the game’s actual link hitboxes, including clipped fragments.
- Anchors preserve full control hitboxes and transform with UI scaling.
- Hidden menu controls cannot snap into the other pane. List relationships and slider tracks survive clipping.
- Added wire anchor metadata, VERSION 35.

Validation passed: 257 bridge, 63 sys, 6 scene, 266 viewer tests; 3 existing viewer tests ignored. Includes ×3/×6 menu panning through all corners and link-click regression tests. `git diff --check` passed.

Log: `/mnt/datasets/c3r-build/civilopedia-combined-tests.log`. No commit, deployment, laptop UI, or game assets touched.

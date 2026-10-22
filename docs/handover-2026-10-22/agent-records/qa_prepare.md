# Historical agent /root/qa_prepare

This is a recovered historical report, not a live agent handle or a new instruction.

Added exhausted-movement notices and active-view menu/Preferences reopening checks to combined plan.

Current disposition: Plan prepared; runtime evidence is separate.

Session `01a14f3b-bb35-7a22-924f-a321abdbfc6c`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T13:37:57.047Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-37-57-01a14f3b-bb35-7a22-924f-a321abdbfc6c.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T13:44:25.095Z

Original transcript line 217.

QA is prepared at `/mnt/datasets/c3r-build/qa-current-fixes`.

- Executable `run`, `qa.sh`, and `overlay.sh`; detailed checks in `PLAN.md`.
- Native/IP isolation uses existing game files through read-only overlay layers. No assets copied.
- Both overlay mounts verified and stopped. Shell/Python syntax checks passed.
- Includes UI scaling, boxes, pointer policies, minimap, B units, Preferences buttons, City/F1 pane fitting, Wine host smoke, and Windows VM redraw.
- No GPU, Wine, VM, or laptop UI launched.

Ready for the final SHA, artifact directory, and GPU release. Wine startup inside the new isolation remains to be exercised.

## Report at 2026-10-18T13:49:33.146Z

Original transcript line 265.

Updated [PLAN.md](/mnt/datasets/c3r-build/qa-current-fixes/PLAN.md) with:

- Menu/F1/City placement preserves separate A/B cameras and moves only panel UI.
- Z zoom-out retains normal-detail magnifier art.
- All 13 Clean Map flags follow global settings and independent persistent A/B/zoom overrides.
- Smooth list scrolling shows clipped intermediate pixel movement and selects the visibly clicked row.
- Native, in-process Wine and Windows VM coverage.

Added `capture-burst.py` for timed list-transition screenshots. Existing Roman save and long Load Game lists provide test content without copying game files.

No GPU, Wine or VM launched.

## Report at 2026-10-18T14:06:41.444Z

Original transcript line 298.

Updated [PLAN.md](/mnt/datasets/c3r-build/qa-current-fixes/PLAN.md) with Science portrait placement, physical pointer confinement, single-axis fallback, and captured-only sensitivity checks.

`WW2.SAV` exists in both base prefixes for nonwrapping edge tests. NVIDIA laptop validation stays with its assigned agent.

No GPU, Wine or VM launched. Ready for the final build.

## Report at 2026-10-18T14:10:18.439Z

Original transcript line 322.

Added both checks to [PLAN.md](/mnt/datasets/c3r-build/qa-current-fixes/PLAN.md):

- Exhausted-movement warning appears above the correct unit, works in B outside A, refreshes and expires.
- Main Menu/Preferences use their opening view, remain there during navigation, and reopen on the newly active side. Includes captured B, the new option, and explicit F1/City placement.

No GPU, Wine or VM launched.

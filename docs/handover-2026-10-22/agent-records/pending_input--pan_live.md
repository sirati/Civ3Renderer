# Historical agent /root/pending_input/pan_live

This is a recovered historical report, not a live agent handle or a new instruction.

Native/in-process pan, momentum, walls, capture and fractional-scale live checks.

Current disposition: Checks passed; desktop HiDPI and exact origin untested; view-budget defect handed to owner.

Session `01a14bcb-0abe-70e3-ae92-b616109c2dfb`, parent session `01a14bbf-0265-73f0-b921-dc0c9e14946d`, depth 2. Started 2026-10-17T21:36:00.195Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-36-00-01a14bcb-0abe-70e3-ae92-b616109c2dfb.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T21:54:59.995Z

Original transcript line 528.

Validated rebuilt zoom `8b81c2c68b6cf7bfc90ea8409a557e511f714a1c` in native and inproc modes.

- Divider walls, edge acceleration, drag controls, momentum, held releases, middle dragging, UnitMouse capture, and short-click selection pass.
- Fractional world scale 1.35 tracks dragging within one pixel.
- Found a separate worker-controls issue: both modes repeatedly warn that panels exceed the renderer's view budget. Reported to root.
- Desktop HiDPI and exact `(0,0)` remain untested.

[Full evidence and installed artifact hashes](/mnt/datasets/c3r-build/pan-scratch/recovery-8b81c2c/evidence.txt). No code edits. Settings restored; owned games, compositor, and VNC stopped.

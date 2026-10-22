# Historical agent /root/camera_decouple/ai_focus_evidence

This is a recovered historical report, not a live agent handle or a new instruction.

Disassembly and callsite evidence for native AI/move/bombard camera events and visibility.

Current disposition: Evidence delivered; parent owns final implementation.

Session `01a14f70-9c1f-77f3-aa49-d5d4576af3cd`, parent session `01a14f6f-fbcd-72f3-a1e6-523c5da1612e`, depth 2. Started 2026-10-18T14:35:42.498Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-35-42-01a14f70-9c1f-77f3-aa49-d5d4576af3cd.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T15:07:16.267Z

Original transcript line 726.

Read-only evidence is complete and saved at:

`/mnt/datasets/c3r-build/camera-decouple/ai-focus-evidence.md`

Key verified facts:

- Native camera setter: `0x4DF7C0`, four stack arguments, `ret 0x10`.
- Native `bring_tile_into_view`: `0x4DFAA0`, five stack arguments, `ret 0x14`.
- AI interturn focus call: `perform_interturn` at `0x4F5B63–0x4F5B70`.
- Unit movement: `0x5BCF30`, `ret 0x8`; C3X-patched executable begins with a JMP.
- Bombardment: `0x5C1120`, `ret 0x8`; also C3X-JMP-patched.
- Combat visibility gate: `0x4A1D60`; `param_3` can bypass native tile checks.
- Animator unit list remains camera-range limited at `0x4EF4D0`.

The temporary `ai_camera_focus.rs` module passed 5 focused tests and i686 compilation, but ownership has been handed back to `/root/camera_decouple`; no further edits were made.

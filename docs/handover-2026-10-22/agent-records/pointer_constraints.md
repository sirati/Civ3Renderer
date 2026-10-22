# Historical agent /root/pointer_constraints

This is a recovered historical report, not a live agent handle or a new instruction.

Wayland walls, gap crossing, lock hints, cancellation and hotplug recovery, merged via signed fixes.

Current disposition: Targeted live checks passed; GNOME and live fractional scaling remain unverified.

Session `01a14bbe-677d-7253-a201-0083865bd76d`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-17T21:22:11.967Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-22-11-01a14bbe-677d-7253-a201-0083865bd76d.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T22:03:30.929Z

Original transcript line 1407.

Pointer work is finished and merged into `zoom`, including signed fix `9d52ea8`. The retained worktree is clean at `b24a1ec`.

- Combined workspace tests: **489 passed**, 7 ignored. The i686 bridge check passed.
- AMD/sway live checks passed for walls, gap crossing, scrolling/stopping, captured City placement, stationary production clicks, Ctrl/Alt transitions, and hotplug recovery.
- Cancellation now passes 10/10 regression cases; the old implementation failed 10/10.
- Owned sessions are stopped. Build/scratch paths were removed, freeing 17.5 GiB.

[Full evidence and limits](/mnt/datasets/c3r-artifacts/ptrconstraint-recovery-20261017/live/final-report.txt). GNOME and live fractional scaling remain unverified. Complete seat replacement requires recreating the constraints instance.

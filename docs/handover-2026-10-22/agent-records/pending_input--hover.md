# Historical agent /root/pending_input/hover

This is a recovered historical report, not a live agent handle or a new instruction.

Recovered context-row geometry and local feedback across native/in-process views.

Current disposition: Signed c5035cb integrated; small tooltip differences and combined QA limits retained.

Session `01a14bbf-6b14-7930-92b6-18f3ed16a93f`, parent session `01a14bbf-0265-73f0-b921-dc0c9e14946d`, depth 2. Started 2026-10-17T21:23:18.422Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-23-18-01a14bbf-6b14-7930-92b6-18f3ed16a93f.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T21:50:21.215Z

Original transcript line 780.

`hover` is ready at `c5035cb`: clean, 13 signed commits, rebased onto zoom `8b81c2c`.

- Fixed context-row geometry and preserved hover feedback through HiDPI scaling/menu views.
- Sol verified context rows, unit controls and popup states against both references. Preferences tabs work in native and in-process modes.
- Full workspace tests and Windows i686 check pass.
- Shared-machine hover latency: native 85.1 ms versus driver 178.1 ms; in-process 34.9 ms versus 270.6 ms. These are not isolated performance measurements.
- Small Menu tooltip/reference differences remain. Combined QA and performance verification remain with root.

Both test sessions stopped. Build target/scratch moved to trash; evidence retained under `/mnt/datasets/winvm/civ3r-runs/hover/`.

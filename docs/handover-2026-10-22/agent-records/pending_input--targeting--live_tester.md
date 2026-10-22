# Historical agent /root/pending_input/targeting/live_tester

This is a recovered historical report, not a live agent handle or a new instruction.

Live native/in-process target and cursor checks plus resource shutdown.

Current disposition: Artifacts preserved; VNC/sessions stopped; exact evidence in parent report.

Session `01a14bc1-915a-7192-ae9d-7fba495f9e2b`, parent session `01a14bbf-7e1c-7032-a80c-3dbb727e2283`, depth 3. Started 2026-10-17T21:25:39.292Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-25-39-01a14bc1-915a-7192-ae9d-7fba495f9e2b.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T22:21:55.741Z

Original transcript line 1146.

43999c5 city-site checks pass in native, in-process, and native split views. Half-zoom previews render; exact comparison is limited by the game's own wrap-edge omissions.

Combined b24a1ec bomb and forbidden cursors pass both modes. Earlier missing-pointer findings came from capture settings, corrected with private `wayvnc -r` and `grim -c`.

Both game sessions are stopped. Build-specific results and artifacts: [final report](/mnt/datasets/winvm/civ3r-runs/targeting/final-recovery-report.txt).

## Report at 2026-10-17T22:23:21.617Z

Original transcript line 1166.

All targeting resources stopped and verified:

- Both owned prefixes have zero game processes.
- Owned sway PID 552191, wayvnc PID 1096239, and input daemon PID 1096282 are inactive.
- VNC port 5967 has no listener.

Artifacts preserved. Updated [report](/mnt/datasets/winvm/civ3r-runs/targeting/final-recovery-report.txt) and [cleanup evidence](/mnt/datasets/winvm/civ3r-runs/targeting/cleanup-evidence.txt).

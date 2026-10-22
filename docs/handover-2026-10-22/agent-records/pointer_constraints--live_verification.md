# Historical agent /root/pointer_constraints/live_verification

This is a recovered historical report, not a live agent handle or a new instruction.

Live walls, scrolling, City focus, modifiers and hotplug checks.

Current disposition: Passed on signed fixes; private sessions stopped.

Session `01a14bbf-2963-7582-96ec-7a5502d9902d`, parent session `01a14bbe-677d-7253-a201-0083865bd76d`, depth 2. Started 2026-10-17T21:23:01.605Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-23-01-01a14bbf-2963-7582-96ec-7a5502d9902d.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T21:25:32.457Z

Original transcript line 166.

Recovered evidence in [report.txt](/mnt/datasets/c3r-build/ptrconstraint-scratch/t2/recovered/report.txt) and [tester-transcript.txt](/mnt/datasets/c3r-build/ptrconstraint-scratch/t2/recovered/tester-transcript.txt).

Initial walls, middle crossing, both views' edge scrolling, and capture passed. City warp while captured, release, Alt snapping, and scrolling stopping remain untested. The original tester stopped on a 429.

A VNC reconnect left confinement inactive. The once-created `wl_pointer`, with ignored seat capability changes, is a likely cause.

The old private ptrc game and display processes are stopped. Logs were preserved. Awaiting your updated build and the end of the AMD performance window for final verification.

## Report at 2026-10-17T21:29:19.767Z

Original transcript line 242.

Prepared and compiled [smoke harness](/mnt/datasets/c3r-build/ptrconstraint-scratch/t2/smoke/src/main.rs), with software Wayland buffers and direct safe `Constraints::move_pointer` calls. No window launched.

[Final plan](/mnt/datasets/c3r-build/ptrconstraint-scratch/t2/final-plan.txt) includes capture, warp, confinement, winit grab transitions, and input hotplug regression. [Reconnect helper](/mnt/datasets/c3r-build/ptrconstraint-scratch/t2/reconnect-input.py) verifies process ownership before restarting input.

Ready to rebuild after source handoff and run after explicit live clearance.

## Report at 2026-10-17T21:32:57.005Z

Original transcript line 305.

Rebuilt the harness against signed tip `372d6b0` using `/mnt/datasets/c3r-build/ptrconstraint` as the target directory. Build passed.

The runtime driver checks actual pointer positions, direct API moves, confinement restoration, winit grabs, and hotplug recovery. City focus testing now explicitly requires opening City directly while captured.

No windows or game launched. Awaiting live clearance.

## Report at 2026-10-17T21:40:16.711Z

Original transcript line 503.

Safe API smoke passed all 17 assertions, including exact hint placement, confinement restoration, three winit grab cycles, and pointer hotplug recovery.

Authoritative files are `t2/smoke-final.log`, `smoke-results.json`, `smoke-driver.log`, and `smoke-success.txt`. Paired captures contain no drawable cursor; exact placement was confirmed using a one-pixel relative compositor probe.

Smoke and game are stopped. Private sway/input remain ready. Awaiting the updated native install before game checks.

## Report at 2026-10-17T21:43:45.908Z

Original transcript line 572.

Completion API verification passed. Evidence is separate in `t2/completion/`, preserving the earlier successful smoke run.

The 17-case replay passed. An unfocused window's move request returned `Cancelled` without moving the pointer. After restoring fullscreen, `Hinted((1500.5, 200.5))` placed it exactly, confirmed by the relative probe. Hotplug recovery also passed.

Smoke exited. Game remains stopped pending the updated native install.

## Report at 2026-10-17T21:47:28.862Z

Original transcript line 653.

Cancellation regression verified.

The old binary failed 10/10 cases: it reported `Cancelled` but moved the pointer. The rebuilt version passed 10/10, retained the original position, and also passed the full 17-case smoke plus timeout and restore checks.

Evidence is isolated in `t2/cancel-before/` and `t2/final-smoke/`, with binary hashes and source snapshots. Smoke exited. Game remains paused pending reinstall.

## Report at 2026-10-17T22:01:42.172Z

Original transcript line 985.

Full native checks passed on `9d52ea8`; combined integration passed on `b24a1ec`. Walls, scrolling and stopping, City focus, stationary click, Ctrl/Alt transitions, and hotplug recovery all passed.

All private game and display processes are stopped. Evidence and reproducible harness are archived in [final-report.txt](/mnt/datasets/c3r-artifacts/ptrconstraint-recovery-20261017/live/final-report.txt).

# Historical agent /root/performance/review_performance

This is a recovered historical report, not a live agent handle or a new instruction.

Independent hash and metric audit of 757 run files,72 review artifacts,36 phase rows.

Current disposition: Historical evidence accepted; moving-frame and cold-feedback limits preserved.

Session `01a14bbe-c264-7f71-a317-2414eb3f51db`, parent session `01a14bbe-20e1-73a0-9309-79642739e53b`, depth 2. Started 2026-10-17T21:22:35.238Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-22-35-01a14bbe-c264-7f71-a317-2414eb3f51db.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T00:39:48.020Z

Original transcript line 2043.

I’ll locate the matching Claude transcript, check the repository and any surviving agent work, and resume from the last verified checkpoint.

## Report at 2026-10-18T00:39:55.895Z

Original transcript line 2058.

The existing agent team is still available. Its latest verified checkpoint is a startup capture bug: the default 4K run omits the title image, while the drawing-enabled reference shows it. I’m keeping the ongoing implementation and review work intact while recovering the original transcript’s requirements. I’m using the boss skill to coordinate that work.

## Report at 2026-10-18T00:41:51.812Z

Original transcript line 2172.

Read-only audit complete. Main constraints:

- Reuse the early `UiModel`; late installation currently resets captured file mappings and screen snapshots.
- Serialize installation and retain hook guards once per process. Partial failure restores hooks but leaves globals populated.
- Make early ownership title-safe. `UiModel::new` starts with skipping enabled, which permanently discards some canvas copies before the publisher identifies the title.
- Preserve map/overlay ownership, generated surfaces, Wine text settings and environment overrides.
- Keep replay initialization lazy and its worker unique. Its queue holds 4,096 commands and can apply runtime backpressure.
- Preserve existing bounds without adding an event backlog. The model has per-canvas/depth/piece limits, but no global memory cap.
- Coordinate pixel suppression with successful startup. Showing the original window after viewer failure currently does not restore drawing.

Findings sent to the performance owner and primary implementer. No edits, builds or GPU sessions performed.

## Report at 2026-10-18T00:49:47.532Z

Original transcript line 2230.

Two compatibility changes need resolution:

- With `CIV3R_UI=off`, native default or explicit `CIV3R_GAME_RENDER=off` no longer disables original map rendering. Previously this setting worked independently of UI capture.
- UI capture failure now aborts viewer startup instead of retaining the existing text/title fallback. GOG and Steam necessarily wait 10 seconds then abort because their graphics-readiness check always returns false.

The supported PCGames path passes source review for serialized installation, retained model/guards, DllMain exclusion, permanent drawing restoration, synchronous JGL capture and one lazy replay worker.

No builds, edits or GPU work performed.

## Report at 2026-10-18T00:53:32.336Z

Original transcript line 2268.

Accepted signed `6cfa29e` for the narrow source review. Both compatibility findings are resolved. Startup ordering, retained capture ownership, permanent restoration and lazy replay initialization remain sound.

All seven source hashes and four check-log hashes match the evidence manifest. Live behavior remains for the functional tester. No edits, builds or GPU work performed.

## Report at 2026-10-18T02:01:13.905Z

Original transcript line 2340.

Narrow audit accepted. No numerical or protocol blocker found.

- Approved preparation hash, frozen source/tree, protocol snapshots and common-input hash agree.
- All 42 logs use the AMD iGPU. Final runs record 600 whole redraws and zero reused pictures each.
- All reported aggregates and six GPU ratios recompute correctly. Three current gates support each paired trace; `ww7` remains final-only.
- Quantiles are medians of run summaries. GPU spans exclude earlier uploads; application intervals include measured first-frame work. Baseline redraw counters remain unavailable.
- Atlas allocation maximum is 128.626 ms; queue interval maximum is separately 3.360 ms. This measures one 4 MiB transfer step, not complete page upload.

These fixed-scene results establish neither live responsiveness nor 240 fps across workloads. No artifacts changed or workloads launched.

## Report at 2026-10-18T02:36:41.562Z

Original transcript line 2407.

Accepted the numerical/provenance audit of all eight runs. [Audit report](/mnt/datasets/c3r-build/perf-scratch/live-standard-audit-22cd2e4ece06/audit.txt).

- REUSE0 map/pan/advisor rates: native **251.429/246.714/180.886**, IP **250.629/244.114/175.286** presentations/s.
- REUSE1 pan: native **230.143 full + 84.657 cached**; IP **238.057 full + 25.943 cached**.
- Cap240 pan achieved **192.114 native**, **173.343 IP** presentations/s.
- Worst standard presentation gap: **26.9 ms**; none reached 50 ms.
- Actual Mailbox configuration uses **3 frames in flight**, despite latency1 requested.

Runtime identities, AMD PCI, 1080p rendering, protocol hashes, counts and phase boundaries agree. Historical Vsync runs support no paired live speedup claim. Motion and cold-preview evidence remain outside this audit.

## Report at 2026-10-18T02:44:39.768Z

Original transcript line 2507.

Accepted the handoff and consolidated draft. No blocker found. [Independent audit](/mnt/datasets/c3r-build/perf-scratch/live-final-audit-22cd2e4ece06/audit.txt).

- Verified 757 run files, 72 review artifacts, protocol hashes and the separate recovery diff.
- Confirmed 14 successful runs plus the preserved original preview failure.
- Recomputed all 36 phase rows. Active full/cache rates match **73.743/955.286 native** and **73.657/850.086 IP**.
- Fixed game-camera traces and release velocities support bounded coast evidence. They establish no unique moving-frame rate or pure-coast throughput.

The draft correctly leaves steady/all-workload 240 fps, achieved cap240 and continuous cold feedback unproven. Draft report SHA256 begins `cf1767f84c5835f7`.

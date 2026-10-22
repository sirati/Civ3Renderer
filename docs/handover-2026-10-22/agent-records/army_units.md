# Historical agent /root/army_units

This is a recovered historical report, not a live agent handle or a new instruction.

Army membership/national colors/aggregate stats; atomic UI review; captured AI keep-visible and protocol50 camera-arrival backpressure; combat diagnostics and outcomes.

Current disposition: Implemented fixes and targeted tests recorded. f510 diagnostic PE build/11-test gate and matched battles remain open.

Session `01a1581b-fca8-7d12-8287-46c390329841`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-20T06:59:51.594Z. Original transcript `/home/sirati/.codex/sessions/2026/10/20/rollout-2026-10-20T08-59-51-01a1581b-fca8-7d12-8287-46c390329841.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-20T16:06:21.310Z

Original transcript line 6112.

The Quit fault occurs in a native audio-interface destructor. Its backend pointer is non-null, but the backend vtable is zero.

[Diagnosis and evidence](/mnt/datasets/c3r-build/quit-audit-army/findings.md) sent to parent and audio owner. The failing session unloaded audio libraries during startup, unlike the earlier session. Without a native stack or object-memory capture, the cause remains unresolved; an own-code regression is not excluded.

## Report at 2026-10-20T16:23:29.535Z

Original transcript line 6159.

[Focused VM acceptance plan](/mnt/datasets/c3r-build/combat-recipe-army/next-vm-acceptance-plan.md) is ready. It covers four bounded combat arms, separate F1 and scenario sessions, and the missing diagnostic observations needed for exact pose timing. All acceptance remains unrun.

## Report at 2026-10-20T16:30:14.925Z

Original transcript line 6330.

[Reviewable patch](/mnt/datasets/c3r-build/combat-diagnostics-army/combat-diagnostics.patch) and [audit/observation limits](/mnt/datasets/c3r-build/combat-diagnostics-army/README.md) are ready.

The isolated candidate records bounded game-thread events, including 1x updates, pose-selection brackets and camera-wait outcomes. Patch application check passes. No shared edits, compilation or runtime work occurred; compiler lane requested after audio controls.

## Report at 2026-10-20T17:52:11.045Z

Original transcript line 6993.

Original candidate passed both Windows i686 feature checks and both focused host tests.

[Check evidence](/mnt/datasets/c3r-build/combat-diagnostics-army/check-verdict.json) preserves source and executed-binary hashes. These results exclude the RNG successor. No runtime or shared adoption occurred.

## Report at 2026-10-20T18:07:20.098Z

Original transcript line 7054.

Successor checks passed: Windows i686 feature off/on and all three focused observer tests.

[Evidence and hashes](/mnt/datasets/c3r-build/combat-diagnostics-army/successor/check-verdict.json) are preserved. CPU lane released. The patch remains isolated and nonshipping; native runtime validation is still pending.

## Report at 2026-10-20T18:20:07.404Z

Original transcript line 7190.

[Outcome successor proposal](/mnt/datasets/c3r-build/combat-diagnostics-army/outcomes-successor/README.md) and [patch](/mnt/datasets/c3r-build/combat-diagnostics-army/outcomes-successor/combat-diagnostics.patch) are ready.

It adds stable defender identity and native outcome snapshots, with explicit absence, read-failure and identity-mismatch states. Eleven tests are prepared. No build, runtime or shared adoption occurred; prior evidence is unchanged.

## Report at 2026-10-20T18:23:31.675Z

Original transcript line 7266.

[Exact f510 overlay and patch](/mnt/datasets/c3r-build/combat-diagnostics-army/f510-outcomes-rebind/README.md) are ready.

All 673 frozen entries were verified. The rebind preserves audio initialization and launcher/cancellation sources. Prior proposals and evidence remain unchanged. No compilation, runtime or shared edits occurred.

## Report at 2026-10-20T19:14:40.840Z

Original transcript line 7682.

Builds remain frozen under the new priority. The reviewed [f510 source package](/mnt/datasets/c3r-build/combat-diagnostics-army/f510-outcomes-rebind/README.md) and all prior evidence are preserved.

The 11-test checks and diagnostic PE build have not run. No runtime or shared adoption occurred.

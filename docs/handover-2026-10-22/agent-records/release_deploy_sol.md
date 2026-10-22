# Historical agent /root/release_deploy_sol

This is a recovered historical report, not a live agent handle or a new instruction.

Source freezes, cross-builds, paired artifacts, signed app staging/install, identity-safe cancellation, cleanup inventories and off-host Git backups.

Current disposition: Neo3be installed, f510 staged only. Source backups closed; release runtime acceptance open.

Session `01a1581e-bf4e-72d2-bbb4-8c803fc4dd7e`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-20T07:02:52.513Z. Original transcript `/home/sirati/.codex/sessions/2026/10/20/rollout-2026-10-20T09-02-52-01a1581e-bf4e-72d2-bbb4-8c803fc4dd7e.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-20T17:41:23.146Z

Original transcript line 8002.

Confirmed launcher defect: after a game failure, the UI remains open but TERM only updates an atomic that no surviving worker reads.

Root received the evidence and a minimal event-driven cancellation proposal. Frozen f7b and Neo staging remain unchanged.

## Report at 2026-10-20T17:53:24.077Z

Original transcript line 8284.

Private cancellation fix passed review and gates:

- Linux: 28 passed, 0 failed, including four actual-process signal regressions.
- Both Darwin targets passed compiler checks.
- Shared source, f7b staging and live3be remain unchanged.

Actual winit and shipping lifecycle acceptance remain pending.

## Report at 2026-10-20T18:00:05.645Z

Original transcript line 8417.

f510 production builds passed for ARM, Intel and Linux launcher/starter. Frozen source and carried PE/native/shader hashes verified.

Root has the manifests and bounded shipping lifecycle plan. No staging, install or launch performed.

## Report at 2026-10-20T18:02:42.012Z

Original transcript line 8492.

f510 is signed and staged on Neo, not installed. All eight files and the prepared app verified; live3be, the live app and prior f7b staging remain unchanged.

Prepared package: `2a278ab8ff8b7a7f2fbbdd37763f1c9fe3006ac685c1cd1c1ba31100b1ee54fa`.

## Report at 2026-10-20T19:08:20.037Z

Original transcript line 8593.

Identified seven obsolete compiler caches with about 6.01 GiB of exclusive Btrfs allocation. No external build-output or symlink dependencies found.

[Exact paths and measurements](/mnt/datasets/c3r-build/handover/release-cache-cleanup-inventory.sol.json)

Nothing deleted. Current builds, pinned dependencies, evidence and rollback runtimes are preserved.

## Report at 2026-10-20T19:18:21.578Z

Original transcript line 8670.

Audit complete. Four linked worktrees and the separate FEX checkout are clean.

[Exact backup paths and Git details](/mnt/datasets/c3r-build/handover/source-backup-audit.release.sol.json)

Main’s Git bundle misses FEX and the private binary/wider/combat/cancellation/QA source trees listed there. No commits, pushes or deletions performed.

## Report at 2026-10-20T19:23:54.266Z

Original transcript line 8764.

All three files are backed up and hash-verified on laptop and Neo. Both bare mirrors passed full Git checks and preserve main `8106ae5` plus all refs.

[Verification evidence](/mnt/datasets/c3r-build/handover/source-main-backups-verified.release.sol.json)

Ready for the separate QA-source backup.

## Report at 2026-10-20T19:29:01.363Z

Original transcript line 8812.

QA-source backups are verified on laptop and Neo. Both new mirrors restore commit `6162a7d` and pass full Git checks. Earlier main backups remain unchanged.

[Final receipt](/mnt/datasets/c3r-build/handover/source-backups-final-receipt.release.sol.json) is also verified on both hosts.

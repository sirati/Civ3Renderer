# Historical agent /root/pending_input/targeting

This is a recovered historical report, not a live agent handle or a new instruction.

Recovered targeting fixes including settler radius and cursor capture evidence.

Current disposition: Signed source integrated; unique saves/evidence preserved, owned scratch stopped/removed.

Session `01a14bbf-7e1c-7032-a80c-3dbb727e2283`, parent session `01a14bbf-0265-73f0-b921-dc0c9e14946d`, depth 2. Started 2026-10-17T21:23:23.295Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-23-23-01a14bbf-7e1c-7032-a80c-3dbb727e2283.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T21:28:37.770Z

Original transcript line 361.

`targeting` is clean at `00c4548`, with three signed commits over `zoom` `659b076`. Root's aliasing finding is fixed using `Cell<i32>`.

Verification: seven targeting tests passed; post-rebase CPU subset passed 360 tests; i686 check and release build passed. Updated bridge installed in both private prefixes. Build artifacts and evidence preserved.

Old Wine tests aligned range/frame/path/turn overlays but found pointer defects in superseded code. Sol tester awaits root's quiet-window release for current-build retesting, including split views and settler preview. Parent will run integrated GPU tests.

## Report at 2026-10-17T22:22:09.575Z

Original transcript line 936.

Targeting is clean and current at `2e4b787`.

Signed fix `43999c5` restores settler previews when generated labels remove mirrored map underlays. Native, in-process, and split-view checks pass. Bridge tests: 149 passed; i686 check passed.

Missing primary cursors were a capture-harness issue, fixed with private `wayvnc -r` and `grim -c`. Bomb and forbidden cursor art match in both modes.

Both game sessions stopped. Builds and evidence preserved in the [QA report](/mnt/datasets/winvm/civ3r-runs/targeting/final-recovery-report.txt). Half-zoom comparison remains limited by the game's wrap-edge omissions.

## Report at 2026-10-17T22:25:18.108Z

Original transcript line 1031.

All targeting-owned processes stopped. Port 5967 is free; no owned runtime remains.

Removed:

- `/mnt/datasets/c3r-build/targeting`
- `/mnt/datasets/winvm/wine-civ3-targeting`
- `/mnt/datasets/winvm/wine-civ3-targeting-ip`
- `/mnt/datasets/winvm/civ3r-native-targeting`
- Both associated Wine server directories and targeting runtime/socket alias.

Preserved scripts, screenshots, traces, reports, unique saves including `combined-citysite.SAV`, and shutdown logs under `/mnt/datasets/winvm/civ3r-runs/targeting`. All 16 preservation hashes verified. Source and commits unchanged.

Updated [report](/mnt/datasets/winvm/civ3r-runs/targeting/final-recovery-report.txt) and [exact cleanup paths](/mnt/datasets/winvm/civ3r-runs/targeting/cleanup-removed-paths.txt).

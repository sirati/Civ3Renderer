# Historical agent /root/scenario_preview

This is a recovered historical report, not a live agent handle or a new instruction.

Scenario cancellation and stale-result protection; later raised the view budget to 32 with a worker-layout regression.

Current disposition: Implementation recorded; current scenario roundtrip acceptance open.

Session `01a14bbe-39ed-7be3-9fab-5f1d21a578ad`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-17T21:22:00.304Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-22-00-01a14bbe-39ed-7be3-9fab-5f1d21a578ad.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T21:30:04.061Z

Original transcript line 370.

`preview2` is ready at signed commit `8b76839`, clean and based on current `zoom` (`6b2de67`). All 13 commits above `zoom` are signed.

Recovered Claude’s implementation and completed testing evidence. Fixed two additional bugs:

- Scenarios without known starting positions now remain black instead of exposing the entire map.
- Native art compilation now cancels obsolete work and rejects stale completed atlases. Both compiler paths share a generic cancellable loader.

Verification passed:

- Full workspace: 422 tests before the final corrections, including AMD GPU staging verification.
- Final affected packages: 170 tests, without warnings.
- Final i686 bridge check and `git diff --check`.

Historical paired tests showed first publication improving to 33–61 ms native and 25–30 ms in-process. They still contained drawing stalls; the docs state that limitation and distinguish these historical measurements from the final corrections.

Evidence and logs: [report.txt](/mnt/datasets/winvm/civ3r-runs/preview2-resume/report.txt).

Removed the private build directory, scratch, Wine prefixes and native install. No fresh GPU workloads ran during the performance window.

Coordinated the remaining merge requirements directly with `/root/performance`: preserve staged atlas replacement, invalidate its frame cache and trimmed sprite metadata in `adopt_atlas`, and address inherited growth/reserve GPU waits. Final live verification belongs on the integrated build.

## Report at 2026-10-17T21:31:43.637Z

Original transcript line 423.

Found one integration defect, reported immediately: [effects.rs](/tmp/c3r-zoom/crates/civ3-bridge/src/effects.rs:226) still calls `assets::compile_growing` without the new fifth cancellation argument. Add `&|| false`.

The semantic merges otherwise look correct:

- Preview gestures retain precedence because `pan_button` and `pan_view_at` exclude `Hit::Beside`; captured exploration bypasses panning.
- Unit art retains effect caches and reading guards.
- Map effects preload remains active; preview art comes from its worker.
- Pan and exploration state both remain intact.

Sent the performance owner the staged-atlas cache invalidation requirements. No source edits made.

## Report at 2026-10-17T21:57:52.747Z

Original transcript line 596.

`view-budget` is ready at signed `26f43de`, rebased onto `zoom` `b24a1ec`. Worktree is clean.

- Raised `MAX_VIEWS` to 32, adding a fixed 140 KiB of uniform buffers. No additional GPU features required.
- Preserved separate HUD panel cameras and input mappings.
- Overflow warnings now appear once per process.
- Added a regression reproducing the worker layout’s 17 cameras, or 25 with three insets and a magnifier, plus room for cursor/FPS.

The regression failed before the fix. All 158 viewer CPU tests pass after rebasing.

Renderer changes are limited to the constant and its comment. Performance owner has the integration details. Isolated build cache removed; logs remain at `/mnt/datasets/c3r-build/view-budget-tests-final.log`.

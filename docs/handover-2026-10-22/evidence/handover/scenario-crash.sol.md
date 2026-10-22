# Scenario crash continuation, Sol, 2026-10-20

Original handover preserved in scenario-crash.md. Shared dirty tree preserved; no commits, stash, reset, Mac builds, GUI operations, or screenshots.

## Additional concrete fixes

- presenter.rs tracks the last generation actually announced to the remote renderer. Previously preload_ready waited for `prepared`, including additions deliberately suppressed after a table was abandoned. Such a generation can never be uploaded. In-process readiness follows adopted generations, because prepare does not send assets there.
- live.rs queues empty growths alongside nonempty growths. Previously an empty growth adopted immediately, retiring pending earlier growth records before their sprite tables were applied. Tables now refuses adoption out of order within one whole description.
- Obsolete pending growths drain without waiting for GPU status. This also covers suppressed empty growths which will never have an upload acknowledgment.
- art.rs clears worker state after every failed whole compilation, not only cancellation. Keeping the prior successful table allowed subsequent additions meant for the failed replacement to grow the old table. live.rs abandons any failed whole generation.

## Targeted regression coverage

- Extended external presenter transport test: upload generation 1, abandon it, prepare a suppressed generation 2 addition, verify preload remains ready.
- Extended table ordering test: generation 7 must be refused while generation 6 awaits adoption.
- Existing cancellation table test now asserts obsolete generations can drain without upload.
- Added art worker failure regression: compile base table, fail replacement with malformed TOML, verify old state is gone and subsequent addition adds nothing.

## Remaining total-freeze diagnosis

No root cause proven. Read-only Mac evidence is saved at `/mnt/datasets/c3r-build/scenario-sol-evidence/mac-readonly-status.txt`. At inspection, installed runtime/game/Civilization III Complete/Conquests/civ3-bridge-watchdog.log was zero bytes, dated Oct 20 14:50. Current bridge log began at unix time 1792479007.460 and ended at 2.231s during text font discovery. Existing separate-viewer logs were older sessions 47149 and 41117. No allocator-held or publisher-stall report exists in this evidence. No speculative allocator or host runtime bypass added.

Existing bounded-wait mitigations, busy cursor parsing, and launcher registry optimization preserved. Parent owns combined release and deployment.

## Validation

Pending fresh Papa-only focused tests in `/mnt/datasets/c3r-build/scenario-sol-target`, using `nix develop`, `CARGO_INCREMENTAL=0`, `TILE_RENDER_ADAPTER=radv`. Final results and source hashes follow below.

### Final validation

All passed on Papa. Evidence logs are under `/mnt/datasets/c3r-build/scenario-sol-evidence/`:

- Presenter suite: 5 passed, presenter-tests.log.
- Art suite: 4 passed, including 3 lifecycle tests and matching district-art test, art-tests.log.
- Launcher registry recognition: 1 passed, startup-test.log.
- System wait cursor parsing: 2 passed, busy-cursor-tests.log. Wine user32.dll exists at the default test prefix, so actual Wine cursor coverage ran.
- No-lock enforcement: 2 passed, no-locks-tests.log.
- git diff --check passes on the three modified files.

Initial fresh build overlapped another agent adding Scene.sprite_lifetimes and failed on the transient missing field. Rerun after unit_notices confirmed protocol47 stable rebuilt Scene and passed. Host tests do not compile Windows-only live.rs. Parent combined Windows cross-build must validate that path before shipping.

Whole-file SHA-256, also in source-sha256.txt:

```
7c7f6635d5916f8f1ecde20c6e27a6ff75dac70052b548672a43086d25de91ab  crates/civ3-bridge/src/presenter.rs
00613fd67ecfb2f82ec6a6f57ef0be317601d77c36158dbea806fab1ebc80745  crates/civ3-bridge/src/art.rs
514042d50f3c3037531b581eb90ac8212ed3d7b275b4b7ab4bb0fd56a48ee655  crates/civ3-bridge/src/live.rs
```

These hashes include concurrent edits by other agents. Own build target is removed after testing; evidence and original handovers retained.

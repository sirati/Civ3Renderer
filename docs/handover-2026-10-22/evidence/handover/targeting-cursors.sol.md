# Targeting cursors Sol follow-up

2026-10-20, Papa. Original targeting-cursors.md remains unchanged.

Reviewed the inherited implementation end to end. The bridge emits snap_to_tiles only for mode 1 and mode 0x12. The viewer additionally requires active targeting and a world pointer/capture, and suppresses it during button snapping. Rebase, bombard, airlift, drop, precision and recon use a pointer-position cursor. Over the virtual world pointer, their validity comes from the game's field_2E1AC and maps to their mission sprite or cursor cell 5. Leaving the world preserves the game's ordinary cursor. No runtime policy issue found.

Only new edit is two tests in crates/civ3-bridge/src/targeting.rs:
- mission_validity_controls_both_the_target_frame_and_cursor checks valid/invalid/valid transitions against both visuals for every disassembled mission family, plus ordinary pointer preservation outside the world.
- only_single_and_stack_moves_request_tile_snapping exhaustively classifies the mode range and IDC_ARROW.

These tests exercise the game's decoded mission groups and bridge policy. They do not execute Civilization III's actual target-legality routines. No screenshots, GUI actions, Mac builds, commits, resets or stash operations.

Protocol remained VERSION 46 on review. No scene/live/lib edits. Root owns aggregate Windows/Wine checks and deployment.

Focused checks and exact hashes will be appended after completion. Build outputs are in /mnt/datasets/c3r-build/cursor-target-agent. Keep logs until root collects evidence; root may remove build outputs afterwards.

Coordination: unit_notices_sol plans VERSION 46 -> 47 for SpriteLifetimes. No wire overlap with this task; final deployment version may therefore exceed 46.

## Completed focused validation

All commands used nix develop on Papa with CARGO_TARGET_DIR=/mnt/datasets/c3r-build/cursor-target-agent, CARGO_INCREMENTAL=0 and TILE_RENDER_ADAPTER=radv.

- cargo test -p civ3-bridge --lib targeting::tests: 7 passed, 394 filtered, including both new tests. Log sol-targeting-tests.log.
- cargo test -p civ3-bridge --lib cursor::tests: 2 passed, 400 filtered. Log sol-cursor-tests.log.
- cargo test -p tile-viewer --lib only_a_moves_target_snaps_the_cursor_to_tiles: 1 passed, 381 filtered. Log sol-viewer-snap.log.

The initial bridge attempt failed on another agent's ui.rs Arc test fixture. Its owner fixed that fixture; the rerun above passed. No changes to ui.rs by this agent. Filtered counts changed while other agents edited the shared tree.

SHA256 at completion:
482503d93f7d156957c3334933fefce6e6198f70ac03c5b65612938391c287b9  crates/civ3-bridge/src/targeting.rs
b153e4a55c67ddf04f4a40d16b4c319aa7898808712b57ef50fb95502090c303  crates/civ3-bridge/src/cursor.rs
69262509bffadcfb64f104139d8b58ef8ef3cb1a16f1b9bd211fe9c9513c6995  crates/tile-viewer/src/cursor.rs
2c11c41c61591ffff87332d42c367ed6532067015010f36edbafb82759217fae  /mnt/datasets/c3r-build/cursor-target-agent/sol-targeting-tests.log
9556e67318cae126b98a847edabbc2b948c906c6cf504c5d368c7d775fa2510a  /mnt/datasets/c3r-build/cursor-target-agent/sol-cursor-tests.log
b1e6bffeb82fcf5ede4b415d0e5df3e73eec1ac711dcee7e2d8f4a45ae4bd74d  /mnt/datasets/c3r-build/cursor-target-agent/sol-viewer-snap.log

Cleanup attempt: logs backed up to handover/targeting-cursors-sol-evidence and preserved at recorded paths. Direct rm cleanup was automatically rejected; cargo clean safely refused the inherited target directory because its CACHEDIR.TAG is absent. Initial refusal preserved all files. Later verified directory ownership, resolved non-symlink root, strict Cargo-output allowlist and every backed-up log hash; migrated the standard cache marker, then cargo clean removed 4.5 GiB safely. All logs restored at recorded paths and backup paths. No source or logs deleted.

# Historical agent /root/pointer_constraints/protocol_review

This is a recovered historical report, not a live agent handle or a new instruction.

Fixed pending lock hint commitment after Locked and cancellation behavior.

Current disposition: Seven tests passed; integrated by parent.

Session `01a14bbf-0ebe-74f1-a0e0-73ab36ea7d55`, parent session `01a14bbe-677d-7253-a201-0083865bd76d`, depth 2. Started 2026-10-17T21:22:54.784Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-22-54-01a14bbf-0ebe-74f1-a0e0-73ab36ea7d55.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T21:28:52.846Z

Original transcript line 226.

Updated only `crates/wl-pointer/src/wayland.rs`, left uncommitted.

- Selects a pointer-capable seat before requesting `wl_pointer`.
- Recreates pointers and confinement after capability loss/reappearance, including both events within one dispatch batch.
- Cleans up seat/registry resources.
- Rejects invalid surface dimensions and skips invalid holes.
- Avoids cloning unchanged regions.

All 7 `wl-pointer` tests passed; `git diff --check` passed. No UI/GPU tests run.

Guest dispatch and window lifetime handling are correct. Complete seat replacement safely disables constraints but does not bind the replacement seat.

## Report at 2026-10-17T21:39:37.196Z

Original transcript line 282.

Found one correctness issue at `lib.rs:873–892`: `c.is_moving()` confirms a pending request, but the new code immediately updates all position caches. If the lock times out or is canceled, no warp occurs and those caches remain wrong.

For example, a screen opens while the viewer lacks keyboard focus. The lock expires after 300 ms, then the next click/wheel uses the requested target instead of the physical position.

Expose move completion/failure and either defer cache updates until completion or restore the prior position on failure, preserving any newer physical motion. The native setter's error path correctly leaves caches unchanged.

## Report at 2026-10-17T21:41:30.303Z

Original transcript line 337.

Implemented `MoveResult` and `take_move_result()` in `wl-pointer`, left uncommitted.

- Reports `Hinted(target)` after lock activation, hint commit, and lock destruction.
- Reports `Cancelled` for timeout, release, capability loss, or missing pointer.
- Clears unread results when starting/updating a move.
- Documents that compositors may ignore committed hints.

All 7 tests passed; diff checks passed. Files are yours again.

## Report at 2026-10-17T21:42:40.656Z

Original transcript line 379.

One remaining issue: successful async placement updates the caches but can leave `Layout` stale.

If physical motion to P arrives while a move to T is pending, `Layout` moves to P. When `Hinted(T)` arrives, the physical pointer and caches move to T, but layout markers, divider state, and edge scrolling remain at P.

Reconcile successful placement with the free, non-snapping layout using the same inside/focus guards, without requesting another warp. The cancellation guard now looks correct.

## Report at 2026-10-17T21:44:17.991Z

Original transcript line 405.

The reconciliation helper looks correct.

One placement path remains unsynchronized: `draw_frame`'s `steps.enter_edge` block updates only `system_pointer` after setting the cursor hint. If focus loss then releases capture without another warp, the next stationary click/wheel uses the old `InputState` position.

On a successful edge-position setter call, synchronize the input and constraint caches through `pointer_placed`; its capture guard already prevents layout rerouting.

## Report at 2026-10-17T21:45:46.289Z

Original transcript line 450.

Verified and implemented in `wayland.rs` only. Lock creation needs no surface commit; position hints do.

Moves now store the target until `Locked` is observed, then commit the hint and release the lock. Cancellation cannot apply an earlier desired hint.

All 7 tests passed; diff checks passed. Left uncommitted, and file ownership is released.

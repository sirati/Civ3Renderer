# Historical agent /root/performance/frame_cap

This is a recovered historical report, not a live agent handle or a new instruction.

Reviewed strict cap semantics and draw-start pacing overhead.

Current disposition: Explained avoidable capped shortfall; current behavior must be rechecked before changes.

Session `01a14bc2-6ca4-7b63-914d-6e0a56092bae`, parent session `01a14bbe-20e1-73a0-9309-79642739e53b`, depth 2. Started 2026-10-17T21:26:35.441Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-26-35-01a14bc2-6ca4-7b63-914d-6e0a56092bae.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T21:30:20.423Z

Original transcript line 163.

Implemented in `/mnt/datasets/c3r-wt/perf`, uncommitted.

- Explicit `max_fps` now caps scene updates and compositor/pointer redraws.
- Cap deadlines follow successful presentations; failed draws only schedule retries.
- Input reception continues through `WaitUntil`, without cap-related blocking.
- Uncapped mailbox/immediate still draw directly; uncapped vsync accepts immediate input requests.
- Updated README and made `apply_patches` public as requested.

Six scheduling tests pass, covering update/redraw storms, presentation timing, retries, cap changes, and disabling the cap. `cargo check -p tile-viewer --tests --jobs 2` passes using the shared target directory. No GPU work performed.

## Report at 2026-10-17T21:41:52.662Z

Original transcript line 231.

One P1 finding: intermediate art commits still consume and render incompatible scenes.

At [lib.rs:1704](/mnt/datasets/c3r-wt/perf/crates/tile-viewer/src/lib.rs:1704), `new_assets` bypasses `matches` when consuming changes. At line 1745 it also forces a complete world rebuild from `display.scene()`.

Reproduction: active generation 1; growths 2 and 3 queued; latest scene already references 3. Committing growth 2 returns `new_assets = true`, so the viewer discards its old world and builds scene 3 with generation 2 tables. It then presents that incomplete scene while growth 3 uploads. The same problem follows a whole commit when `scene.assets > version`, which the compatibility check currently accepts.

A resize during an incompatible scene also bypasses cached arrangement reuse at line 1707 and builds layout from the incompatible scene.

Minimal safe fix: gate scene consumption, world/layout rebuilding, and presentation on compatibility. Preserve an art-dirty flag across waiting frames. Keep drawing a cached compatible scene, or retain the last presented picture until the required growths finish. Do not merely require whole-version equality, since the publisher can advance directly to a later growth generation.

No additional defects found in versioned reservations, native atomic publication, cancellation/Busy handling, Invalidated retries, or the frame clock. Read-only review; no GPU use.

## Report at 2026-10-17T21:52:08.823Z

Original transcript line 376.

Implemented viewer fallback in `crates/tile-viewer/src/lib.rs`, uncommitted.

- Incompatible scenes replay the last composed picture without consuming `Changes` or rebuilding world/layout.
- Cached canvas, copies, and input mapping survive resizing.
- Missing composed cache retains the compositor’s previous picture.
- Whole commits prepare cursor art; activation and custom cursor replacement wait for compatible art/scene generations.
- Replay presentations update FPS, frame timing, and cap clocks, with a separate profile event.
- Cached cursor/FPS pixels remain static. Replay produces no pointer-latency samples.
- Updated the 4 MiB transfer allowance comment.

Validation: all 159 viewer unit tests passed, including two resize/mapping regressions. Final `cargo check -p tile-viewer --tests --jobs 2` passed. No GPU work performed.

## Report at 2026-10-17T22:02:15.124Z

Original transcript line 427.

Added four CPU regressions in `looks.rs` covering metadata-only driver updates, changed replacement art with unchanged `LookState`, identical metadata, and moved interactables under a stationary pointer. Tests use real scene updates plus `Feedback` and `splice`. `git diff --check` passes.

No Cargo process started during your build. Run:

```bash
CARGO_TARGET_DIR=/mnt/datasets/c3r-build/perf CARGO_BUILD_JOBS=2 nix develop -c cargo test -p tile-viewer --lib looks::tests
```

## Report at 2026-10-17T22:07:19.568Z

Original transcript line 452.

Add `shown_scene: Option<Scene>` as the render/input source, updating it only when a new compatible Display sequence is adopted. Clone once per adopted sequence, never per frame.

Switch these reads to `shown_scene`:

- `draw_frame`: layout, bounds, interface rectangles, world/cell/free construction, overlay spans, hover metadata, cursor facts/art selection, animation tick, patches, and presentation bookkeeping.
- `update_plan`: screen size and normal layout/scaling on resize. Remove the cached-canvas restriction while a compatible live scene exists.
- `driver_cursor` and cursor-art activation.
- `window_event` → `snap.route`, and `about_to_wait` → `snap.tick`.
- Draw scheduling should continue whenever a shown scene exists.

Keep Display reads for receiving updates, scene-reception profiling, transfer compatibility checks, and deciding whether a newer scene can be adopted.

Adoption should atomically pair the prepared atlas/compiled metadata/cursor art with its compatible scene. Only then drain Display `Changes` and patches. While waiting, render `shown_scene` normally using empty driver changes; local layout motion, hover, cursor, FPS, and resize still rebuild their normal frame data. Keep future patches out of the active patch map.

The renderer prerequisite is essential: preparation must not replace the atlas/compiled tables backing `shown_scene` until the complete required generation is ready. Cached-picture replay becomes a fallback only when no compatible live scene exists.

Regression coverage should hold an incompatible future scene for many frames and verify changing cursor/hover/FPS output, safe resizing with old IDs, untouched Display changes/patches, and one complete handoff when art becomes ready.

## Report at 2026-10-17T22:11:11.824Z

Original transcript line 563.

Implemented `shown_scene` retention in `lib.rs`.

- Draws, hover, cursor, snapping, and resize layout use the retained compatible scene.
- Future Display changes and patch pixels remain queued until adoption.
- Snapshot cloning happens once per adopted sequence.
- Cached-only replay now requires absence of a compatible live scene.
- Upload/PendingArt blocks remain untouched for async integration.

Three new retention regressions and ten hover tests passed through a standalone CPU harness. `git diff --check` passes. No Cargo process or GPU work started.

Async owner confirmed whole preparation will preserve active art until the complete replacement can hand off atomically. Full viewer build remains to run after that integration.

## Report at 2026-10-17T22:25:14.946Z

Original transcript line 585.

It is intentional in the current tests, but adds avoidable pacing overhead.

`allowed()` blocks the **start of the next draw** until `last_presented + interval`. `presented()` then restarts that interval after the draw finishes. Thus:

`presentation interval ≈ 4.167 ms + CPU draw time + timer overshoot`

At cap 240, 0.7 ms gives approximately 205 presentations/s; 1.3 ms gives approximately 183. GPU execution time adds only when it blocks acquisition/submission.

`cap_starts_when_the_frame_is_presented` explicitly enforces this conservative behavior. My cap fix guaranteed the maximum by delaying the entire draw.

A strict presentation cap does not inherently require that extra draw time. Preparation could happen before the deadline, with presentation deferred through the event loop until allowed. Simply anchoring starts to previous starts would weaken the strict presentation guarantee when draw durations vary.

Treat any measured capped shortfall separately from renderer throughput. No edits, tests, builds, or UI sessions run.

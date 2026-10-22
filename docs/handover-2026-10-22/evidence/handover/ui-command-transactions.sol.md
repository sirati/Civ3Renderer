# Successor UI command transactions and viewer GPU actor integration

This is successor work after final46e. No commits/stash/reset, GUI, screenshots, Mac execution or builds, or CPU rendering. Only papa nix builds under /mnt/datasets/c3r-build/ui-prefix-target, CARGO_INCREMENTAL=0, TILE_RENDER_ADAPTER=radv.

## Ownership

- This agent: main App/Gpu metadata + worker integration, scene_prefix.rs, ui_submission.rs, small InputState Clone derive, Layout::adopt_presented_mapping, CPU-only bitmap presentation bridge in presentation.rs.
- startup_progress_sol: render_worker.rs actual GPU ownership, init/atlas/draw/submit/poll/recovery, actor tests.
- preview_smear: triple small GPU control buffers and latency callback bookkeeping.
- mac_fps_music: paired CPU/GPU timings and detailed overlay.
- slider_arrow_keys: OS cursor state and immediate metadata updates.
- army_units: procedural solid fill representation and GPU shader handling.

## Main transaction flow

Main prepares one immutable frame packet while capturing the currently active input mapping. It restores that mapping immediately after publication. A matching Presented(id) activates the candidate Scene/Layout/LookScene/Plan/input mapping; stale configuration/ID replies cannot activate it. Bitmap replies are checked before the OS bitmap present. Only one normal UI packet or cached replay awaits a reply, so Display coalesces incoming scene updates and no unbounded world/history map is retained. App.shown_scene and Mapping.scene are Arc<Scene>; unchanged rendering does not duplicate world layers. TileWorld is Arc and only mutated between worker acknowledgments; actor takes accepted Latest packets to release CPU world ownership before replying.

MainView/Views requests are consumed only while preparing a complete frame and sent with its Presented commit. External OS input/cursor handling continues on main while worker renders. Live revisions include settings, grab changes, OS/device events, and nonempty snap-generated input. An older presented frame installs routing geometry without replacing newer desired split/clean-map/capture/pointer/key state. Pointer remap follows the matching camera-control batch.

## Pending art

Same-base pending growth metadata is applied without pixels for bounds/layout. The full original scene is retained. Only a render copy is sanitized against drawable descriptors: unknown sets/fonts/palettes become explicit EMPTY + zero-clip no-ops, retaining cardinality and positions. Procedural solid commands remain available independently. Future layer LUT replacements are checked against the drawable lookup, so an old numeric value never draws old/wrong art. Named palette row base and contents are checked. Local caches bound repeated set/frame and (layer,raw) comparisons during each transformation; the transformed scene is cached by source scene/version and physical prefix. Whole replacements retain the epoch guard.

Strict actor through-version matching was identified as a possible optional-art starvation source. Root and actor owner are implementing a proven compatible-prefix lower bound; do not remove that guard without the compatibility proof.

## Verification

- Host cargo check -p tile-viewer --tests passed after actor integration.
- Focused initial prefix tests: 4 passed, including synthetic GPU numeric every-pixel group move/color and future set/missing-font no-leak cases. /mnt/datasets/c3r-build/ui-prefix-tests.log.
- UI transaction tests: 3 passed, including Arc sharing/preparation restore, matching ID/configuration control batch, and newer settings preservation. /mnt/datasets/c3r-build/ui-submission-tests.log.
- Full viewer library suite currently running: /mnt/datasets/c3r-build/ui-viewer-full-tests.log. Several preexisting exhaustive GPU layout/scaling tests exceed 60 s; no failure reported as of this note.
- Root owns final Windows/Wine and Apple cross-check. Host cannot validate CpuBitmap/Attached/raw-surface macOS branches.

## Follow-up measured metadata clone audit

Main App and pending Mapping now share immutable Arc<LookScene>. Changed metadata allocates a replacement through refresh_shared under the existing full/seq and contents-equality gates. Captures no longer deep-copy nested state sprites. Exact Presented/configuration and input/settings merge logic are unchanged. Added semantic regression with 10k look-state sprites verifies immutable old geometry/art and shared capture/unchanged refresh.

Release allocator benchmark and source snapshot: /mnt/datasets/c3r-build/ui-clone-audit. Baseline paired captures for 9984 state sprites cost933415ns/2564allocations/2846720requested bytes; shared metadata225ns/zeroallocations. Generated4K Layout::frame geometry with140x140world,3insets,8interface rectangles and retained Frame gives374–387ns/12allocations/3368bytes for two captures. Layout copying retained because measured bounded cost is small. These are isolated synthetic metadata fixtures, not live native control counts or full renderer240Hz evidence. Offline real-art trace lacks live LookScene controls. Runtime QA owner enabled CIV3R_LOOKS_LOG for their separate untimed run; no actual live count yet claimed.

Post-change main checks: ui_submission5passed, looks11passed, presented_mapping1passed. Source stable after these edits. Worker owner independently confirms Frame.world dropped before reply/wake, preserving sole-world ownership on next preparation. No world/assets buffer copy changes.

# Startup progress window

Source ready in shared tree. No commits, resets, Mac builds, GUI operations, screenshots, or deployments performed.

## Implementation

- `crates/civ3r-launcher/src/startup.rs` runs a native winit window on the main thread and runs the existing launcher lifecycle on one worker. std mpsc carries stages, diagnostics, logs, readiness and completion. No application locks or new unsafe code.
- Window starts at 640x300 logical pixels. It shows elapsed minutes/seconds, current stage and reason. Failure expands to 800x540 and supports wheel scrolling through the error and full log paths. Failures after readiness say the game stopped unexpectedly.
- macOS uses Cocoa through winit with Accessory activation policy and no default app menu, preserving the existing LSUIElement bundle behavior. The bundle starter still execs the launcher and retains stop ownership.
- Closing during startup writes the existing cancellation flag. Existing registry/viewer/game waits check it, and existing ManagedChild guards stop/reap only owned process groups. The native window remains hidden after menu readiness while the main thread continues to supervise the lifecycle worker.
- Worker panic is caught after owned-child unwinding and preserves its panic reason. Unexpected channel disconnect becomes a failure once instead of hanging.
- Launcher emits honest stages for private-install preparation, Wine prefix/registry checks, Wine startup, waiting for game, and renderer/art after current-session bridge-log modification. ARM64 waiting text says Wine *may* be translating x86 code, with first-launch/update explanation. No elapsed-time-based translation claims.
- `CIV3R_STARTUP_WINDOW=0`, help, and Linux without DISPLAY/WAYLAND_DISPLAY run the existing blocking CLI path without a GUI. Display initialization failure falls back to the CLI. Errors still go to stderr.
- Removed existing osascript error alert in favor of the native Rust failure panel.
- `CIV3R_STARTUP_READY_FILE` points to `menu-ready` inside the existing per-launch private Session directory. Both Wine/in-process and separate viewer receive this exact session path. No stale readiness can cross sessions.
- `crates/tile-viewer/src/startup_ready.rs` atomically renames a temporary marker only after `draw_frame` successfully presents a nonzero menu-fit scene whose art generation matches. Empty interface shells are rejected; the hook additionally verifies that an interface sprite resolves to positive compiled drawable bounds.
- Game exit 0 before the first complete-menu marker is now code1 with an explicit startup failure cause and log paths. Normal quit after menu and clean viewer close while game remains alive retain normal semantics. Signals/user cancellation are checked before game failure classification.
- Added winit, softbuffer without KMS, and font8x8 dependencies to the launcher, updating Cargo.lock. The existing app bundle/plist are unchanged.

## Validation on papa

Environment: nix develop, CARGO_INCREMENTAL=0, TILE_RENDER_ADAPTER=radv, own CARGO_TARGET_DIR=/mnt/datasets/c3r-build/startup-progress-sol. Existing precompiled SPIR-V supplied for tile-viewer checks.

- Linux launcher cargo check passed.
- Full launcher tests passed: 12 library tests plus 2 bundle/config tests. Includes real owned process-group cancellation/cleanup, unexpected channel disconnect, panic diagnostic preservation, and early-clean-exit classification.
- tile-viewer startup readiness test passed, including no scene, no interface content, unmatched art, and positive menu readiness.
- civ3r-sync no_locks tests passed.
- aarch64-apple-darwin cargo check for launcher/starter passed using /mnt/datasets/c3r-build/release-1019/darwin-cross from papa. No Mac compilation.
- Actual headless `cargo run ... civ3-play-macos --help` passed without creating a display.
- Rustfmt applied only to owned launcher source and the new readiness helper. Existing shared tile-viewer lib.rs received only module declaration and presentation hook.

## Remaining runtime validation

Root/deploy owner should deploy with matching viewer/native/DLL set and check native status appearance, stage updates, menu disappearance, cancellation during prefix startup, and failure logs on TS Neo. Coding agent did not operate GUI. font8x8 supplies an ASCII bitmap body; paths containing unsupported glyphs remain exact in the logs/stderr but unsupported characters are not drawn in the body.

Final rerun after the last diagnostic/accessory-policy adjustment passed: launcher 12 library tests and 2 bundle tests; tile-viewer startup readiness 1 test. Current macOS cross-check also passed.

## Frozen-source Intel macOS offline validation

Completed against frozen source `ef962736b6840284c6dc052d9df989d3539d4b16a83a6a28149ac222e0013754`, protocol47. All624 source manifest files matched. The supplied release shader was exactly `d19e66b8b204f1d9ee0d8c383e81f6180d5e14fa459f76551d168769d201980b`.

Papa cross-compiled the launcher, app starter and CPU test harnesses for x86_64-apple-darwin. Existing clang21/lld21/SDK26.5 support x86_64; a private wrapper selected --target=x86_64-apple-macosx14.0. Rust1.99.1 Darwin x86_64 std was a short Nix binary fetch using the existing overlay. No compiler rebuild or global environment change was needed. Private wrapper/build scripts and Rust link remain under /mnt/datasets/c3r-build/startup-progress-sol-x86* for reproducibility.

The restored work VM ran all tests through its existing SSH helper. Host was macOS14.8.9 build23J631, Darwin23.6.0 x86_64. All executables were confirmed Mach-O64 x86_64. All31 CPU tests passed: launcher12, starter2, readiness1, notice-lifetimes2, scene/wire14. These include real Darwin process-group cancellation/reaping, unreaped group-leader ownership, early clean exit before readiness, worker disconnect/panic diagnostics, notice expiry without incoming scenes, identical-art lifetime renewal, and scene protocol/lifetime round trips. Both production launchers passed headless --help probes with CIV3R_STARTUP_WINDOW=0.

Evidence: /mnt/datasets/c3r-build/startup-progress-sol-x86/evidence.json, vm-results.log, source-verification.json, build.jsonl, build-production.jsonl and cleanup.log. payload/ retains exact executables, with SHA256 identities in evidence.json. No source edits, Mac compilation, GUI operations, game asset access, VM lifecycle changes or changes to other VMs occurred. Owned VM scratch /tmp/civ3r-cpu-ef962736-startup-sol was removed after result retrieval. Work remains running and SSH-accessible.

This validates native Darwin CPU behavior and cross-linkage. Native window appearance, Metal drawing, Wine/FEX startup and Apple ARM64 runtime behavior still require production runtime/GUI tests.

# Native thread initialization successor

Frozen source60931 remains immutable. These changes are in the shared successor source tree only.

`ViewerConfig::native_thread_init: Option<fn()>` defaults to None. The native in-process host supplies `wine_unixlib::unix::block_native_signals`. Viewer GPU and cursor worker threads run the callback before native work; the GPU invocation is inside its existing panic catcher. The event-loop thread does not run this callback. Generic renderer code contains no Wine/platform conditionals or unsafe code.

`block_native_signals` extracts the existing `spawn_native` sigfillset + pthread_sigmask(SIG_BLOCK) body without changing its behavior. `spawn_native` calls the same helper. The helper's documented contract excludes Wine Windows and AppKit event-loop threads and Windows callbacks afterward. Children inherit this mask before driver thread creation.

This closes a source-level contract gap on macOS: viewer-owned workers previously inherited the original AppKit thread's mask rather than the protected native thread mask used on Linux. No signal-induced runtime crash has been proven or claimed.

Tests cover generic initializer ordering/thread isolation and standalone None, the actual cursor worker initializer before completion wake, and Unix blocked signals/descendant inheritance with RAII restoration on an isolated test thread. No locks added. Unsafe test operations remain inside wine-unixlib's existing interop module.

Read-only reviewer scenario_crash found no source contract/order gap. Host/ARM validation results pending.

Validation complete:
- `cargo test -p wine-unixlib --lib`: 1 passed (signal policy, child inheritance, restoration).
- `cargo test -p tile-viewer --lib thread_start`: 2 passed.
- `cargo test -p tile-viewer --lib cursor_completion_tests`: 2 passed.
- `cargo test -p civ3r-sync --test no_locks`: 2 passed.
- Host `cargo check -p civ3r-native`: passed.
- Papa cross `cargo check --target aarch64-apple-darwin -p civ3r-native -p tile-viewer -p wine-unixlib --tests`: passed, including Darwin test compilation. No Mac execution/build performed.

Logs: `/mnt/datasets/c3r-build/handover/native-thread-init-host.log`, `native-thread-init-arm.log`, `native-thread-init-no-locks.log`.
Owned build target: `/mnt/datasets/c3r-build/native-thread-init-sol`.
Shader supplied unchanged via `TILE_RENDER_PRECOMPILED_SPV=/mnt/datasets/c3r-build/army-solid-artifacts/shader.spv`, SHA256 `67f3cefc9e905d7404baf63a3d0a3b54ca775bddc61612f52aa57c9b7de1ceb0`.
First cross command omitted this environment variable and could not invoke shader cargo's nightly selection through the cross toolchain. Corrected cross command supplied it and passed; this was configuration, not source failure.

Stable implementation verdict: ready for the parent's successor freeze. Exact60931 VM baseline remains separate and unchanged.

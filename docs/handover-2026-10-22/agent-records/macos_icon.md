# Historical agent /root/macos_icon

This is a recovered historical report, not a live agent handle or a new instruction.

AppKit icon integration and corrected color-managed native icon oracle.

Current disposition: Targeted native checks passed; installation identity tracked separately.

Session `01a1540c-6c60-7a11-8a5e-3c5bcd2b6dde`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-19T12:04:22.755Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T14-04-22-01a1540c-6c60-7a11-8a5e-3c5bcd2b6dde.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T12:11:59.548Z

Original transcript line 257.

Implemented the original Civilization III icon for the native viewer, including its macOS Dock icon.

Changed:

- New `crates/application-icon/` common API and `crates/application-icon-macos/` objc2 backend.
- `ViewerConfig` and native viewer bootstrap.
- Embedded icon, provenance and documentation in `crates/civ3r-viewer/assets/`.
- Relevant Cargo manifests and lockfile. Launcher and vendor winit unchanged.

Validation passed: 2 host tests, Linux viewer check, Apple ARM64 all-targets check, and exact comparison of all 4,096 RGBA bytes against the executable’s icon.

Evidence: `/mnt/datasets/c3r-build/macos-port/icon-work/icon-validation.json`.

Native AppKit execution remains assigned to the presentation owner after root releases the next build:

```sh
cargo run --locked -p application-icon-macos --example verify -- crates/civ3r-viewer/assets/civ3-conquests-icon.png
```

It checks decode, installation, exact image readback, malformed-data retention and main-thread enforcement. Physical game session untouched.

## Report at 2026-10-19T12:18:36.575Z

Original transcript line 308.

Cleanup complete. The common crate now uses an inline 2×1 PNG fixture; game artwork assertions live in `civ3r-viewer/src/icon.rs`.

All 3 affected tests pass. Production code, icon asset and unsafe contract are unchanged.

Test source SHA256:

- `application-icon/src/lib.rs`: `b60919fd02ecd13a7b6335134fe1cfd4c3d2344c15b6ef5bf8e11ca04ccb0b2c`
- `civ3r-viewer/src/icon.rs`: `24d5502732791360d07c79c8ad3a203f17cb269b23f82b4a589a1ff918220dde`

Added an image dev-dependency to the viewer and updated Cargo.lock. Evidence refreshed in `/mnt/datasets/c3r-build/macos-port/icon-work/icon-validation.json`.

## Report at 2026-10-19T13:31:00.882Z

Original transcript line 819.

Native icon verification now passes. Production backend unchanged.

AppKit converts the original 32×32 icon into a 64×64 display-colour-managed representation. The corrected test captures a standard AppKit control, installs a distinct cyan marker, then requires our wrapper to restore all 16,384 RGBA bytes, ICC profile and dimensions exactly. Invalid-data retention and valid-PNG `MainThread` rejection also pass.

Final test SHA256: `b72613ff4d0f98efd699b01e0f4e4d2cb4a6c39495769aaa74ff1ee4a8787c77`.

Settled changes: backend example, test dependencies and Cargo.lock. Evidence reconciled in `/mnt/datasets/c3r-build/macos-port/icon-work/icon-validation.json`; native report in `tsneo/evidence/early-efd15ad1/icon-acceptance/result.json`.

No further icon edits pending. No screenshot or production deployment claim.

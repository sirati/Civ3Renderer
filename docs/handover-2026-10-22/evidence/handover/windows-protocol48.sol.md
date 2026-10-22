# Protocol 48 native Windows correctness QA

Completed against immutable source `60931ad49f18c0fb08a9ff2ed5710c6953f0f76f564837a8a6389deb4850ca9d`, scene protocol 48, canonical shader `67f3cefc9e905d7404baf63a3d0a3b54ca775bddc61612f52aa57c9b7de1ceb0`. No source edits, game assets, GUI actions, screenshots or Mac access. This is software-adapter correctness coverage, not a performance claim.

## Results

All six native invocations exited 0, with expected executed test counts confirmed and output untruncated:

| Native check | Adapter/API | Result |
|---|---|---|
| procedural_solids | SwiftShader Device (Subzero), Vulkan | 5 passed |
| control_frames | Explicit Vulkan backend | 3 passed |
| procedural_solids | Microsoft Basic Render Driver, Dx12/WARP | 5 passed |
| control_frames | Explicit Dx12 backend | 3 passed |
| game_width_preserves_the_games_lfheight_sign | Native Windows GDI, existing system fonts | 1 passed |
| measured_layout_keeps_unicode_and_raw_ansi_punctuation_pens_equal | Native Windows GDI, existing system fonts | 1 passed |

Total 18 accepted executions. Both procedural-suite logs print actual adapter identity/backend. WARP was limited to the same eight compatibility checks; no new Direct3D features or driver work. GDI checks selected exact names, each ran one test with 411 filtered, and used no copied font fixture.

Raw logs and numeric exits are in `/mnt/datasets/c3r-build/windows-protocol48-sol/evidence/`: vulkan-procedural, vulkan-controls, dx12-procedural, dx12-controls, gdi-height-sign and gdi-punctuation-pens. `results.json` records every requested backend, expected count and exit. `qa-summary.log` retains runner results.

## Source and build provenance

Source path `/mnt/datasets/c3r-build/release-1019/src-60931ad49f18c0fb08a9ff2ed5710c6953f0f76f564837a8a6389deb4850ca9d`. Root explicitly issued GO before compilation. Before that, only the owned guest/driver tooling was prepared. Source verified before build and after testing, final `verified 630 drift []`. Production 46e artifacts untouched; paired successor deployment proceeded independently.

All QA executables compiled on Papa using nix develop from the immutable snapshot, own CARGO_TARGET_DIR `/mnt/datasets/c3r-build/windows-protocol48-sol/target`, CARGO_INCREMENTAL=0, TILE_RENDER_ADAPTER=radv, TMPDIR in owned datasets scratch, and TILE_RENDER_PRECOMPILED_SPV `/mnt/datasets/c3r-build/army-solid-artifacts/shader.spv`.

Commands:
- `cargo test --locked -p tile-render --test procedural_solids --test control_frames --target i686-pc-windows-gnu --no-run --message-format=json`
- `cargo test --locked -p civ3-bridge --lib --target i686-pc-windows-gnu --no-run --message-format=json`

Build JSON/logs retained as gpu-build and bridge-build. Executable provenance records source hash, protocol, original and transported executable SHA256, identical .text hash and canonical shader embed count. Each executable contains the exact 67f3 shader once. Only debug sections stripped in transport copies; original code bytes unchanged. Tested transport executables retained under evidence/test-binaries with canonical evidence/shader.spv and retained-artifact-sha256.txt. No rebuild is needed to inspect/re-run these retained artifacts.

## Guest, driver and environment

Owned throwaway VM `c3r-protocol48-sol`, 2 GiB and 4 vCPU, actual QEMU /proc cgroup verified under `c3r-vms.slice/c3r-windows-protocol48-sol.scope`. Existing Mac work VM remained 3 GiB; shared hard cap stayed 10,000,000,000 bytes with swap 0. Other VMs and golden images untouched.

Trusted prebuilt i386 SwiftShader/native Vulkan loader from official immutable Chromium snapshot:
`https://storage.googleapis.com/chromium-browser-snapshots/Win/1722657/chrome-win.zip`
Archive SHA256 `5ed5ccfc220128a4c3223130c0595ab366758c1e4f57e68125de2dc115ee1667`, identical to the previous validated Vulkan pass. Extracted only vk_swiftshader.dll, vk_swiftshader_icd.json and vulkan-1.dll. Driver hashes and config retained. No driver compilation or system/registry installation.

Direct headless QGA transfer/execution avoided GUI and SSH dependencies. Every GPU invocation used process-only WGPU_BACKEND and exact VK_DRIVER_FILES/VK_ICD_FILENAMES paths. Primary Vulkan resolved to SwiftShader CPU, vendor 6880/device 49374. Minimum WARP resolved to Microsoft CPU, vendor 5140/device 140, backend Dx12. Existing native system fonts provided GDI metadata measurements.

## Cleanup

Only owned VM and ephemeral overlay cleanly destroyed; scope inactive. Owned driver archive, extracted driver/ICD and temporary executable transport directory removed after exact path validation. Cargo clean removed 2.9 GiB of owned build intermediates. Retained tested executables, SPIR-V, source manifest, hashes, scripts, raw logs and exits remain in evidence. Final shared cap unchanged, swap 0; final current memory 3,306,508,288 bytes. No copied game/font assets existed in this guest.

Evidence includes vm-cleanup.log, host-cleanup.log, build-cleanup.log, cgroup-before.log, cgroup-final.log, qemu.cgroup, source-verify-before.log, source-verify-after.log and log-sha256.txt.

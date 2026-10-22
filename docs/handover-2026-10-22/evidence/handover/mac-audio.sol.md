# macOS Wine SS-return compatibility

Current source `beae2e2d982a719cb2821805ed8cf5aa8ecd03fbb84fb32236d5f581936ff3f5` passed the original-vendor Intel macOS VM control, including sustained map/city/advisor use and normal GUI shutdown. Current frozen native-renderer and combined-launcher acceptance remain the root/VM owner's next step. Do not treat the original-vendor control as full renderer acceptance.

## Current implementation

`crates/civ3r-launcher/src/unix/wine_compat.rs` prepares a private Wine view before any registry or game Wine process starts. Exact SHA256 and layout gates accept only the official Wine Stable 11.0_1 Intel macOS images. Linux and Apple ARM64 retain the original Wine path. Unknown Intel images fail before launch with the supported runtime named in the error.

The view owns only three dependency copies, totaling 642,096 bytes:

| Image | Bytes | Source SHA256 |
| --- | ---: | --- |
| Native Wine loader | 27,680 | `61ae8ab79e35f4982d5f6be14a3a26dcc5383578a298d9f767811086b40020d3` |
| ntdll.so | 577,552 | `dfb9d4d4d6e7fd3dc537c1bacd69cb3a6bb46ae98311bb5358df3c4eab470c3f` |
| wow64cpu.dll | 36,864 | `cbbf4d054d5488e19d82ea24e7c280c3408ee43b00fce708d1b559acada9d21e` |

The original bin/wine entry is also exact-gated when selected. Everything else is a symlink to the installed Wine dependency, with a 4,096-entry limit. No game assets, shared Wine files or prefix-global state are changed. Complete private files publish with rename before the executable path is returned. The owned guard removes failed preparations and removes the view after managed Wine children stop. Only Rust startup code ships; patched dependency files are generated locally from the user's installed Wine. No new unsafe Rust, waiting lock or per-frame operation was added.

Installed Wine searches its own dll_dir before WINEDLLPATH. Executing the copied native loader makes its realpath/dlopen select copied ntdll.so and therefore the private dll_dir. Original Unixlibs import `@rpath/ntdll.so` relative to their symlink targets, so `Runtime::command()` also prepends the private x86_64-unix directory to child-local DYLD_LIBRARY_PATH, preserving inherited entries. Both registry and game call sites use that API. Process-global environment and the separate native viewer remain untouched.

The launcher insertion is immediately after Session creation, before registry initialization. ManagedChild, its token environment and strict descendant cleanup remain the cleanup owner's implementation.

## Exact static transformation

The original six return bytes at RVA 0x1296 become `e96508000090`. Stub RVA 0x1b00 receives `418b95c80000008ed24c87f441ff2e`: reload saved 32-bit SegSs from context+0xc8 while still on the 64-bit stack, then execute Wine's original xchg rsp,r14 and far jump. EAX/nonvolatiles and the ABI stack layout are unchanged.

.text VirtualSize grows 0x9d0 to 0xb0f. The stub shares the original dispatcher's non-leaf unwind record at RVA 0x5030. A sorted RUNTIME_FUNCTION entry [0x1b00, 0x1b0f, 0x5030] is appended at .pdata RVA 0x4114. .pdata VirtualSize and the exception directory size grow 0x114 to 0x120. There are 27 changed bytes within six planned ranges and 24 sorted, nonoverlapping unwind entries.

Expected private DLL SHA256, checked after transformation: `d0da82572f7cc2eb3a63a135bb9abe25427916badde18c99b75f547883510d22`.

Independent review is `handover/sol-release-private-wine-review.json`. Review found and fixed missing stub unwind coverage and an intermediate misplaced mutation block. All live-hotpatch modules, capability and presenter hook from the rejected AtomicU16 proposal were removed.

## Validation

Host six CPU tests and the external exact official-image/private-view test passed. These cover malformed/signature rejection, branch target, final permissions and atomic file publication, failed-view cleanup without following links, inherited dyld path order, source immutability, all changed ranges, unwind metadata and output hash. Native Intel probe/tests cross-built on Papa. Apple ARM64 cargo check passed. The actual Intel macOS guest ran all seven tests with matching uploaded binary hashes.

Current source and immutable harness are `mac-audio-sol/offline-snapshot2`. Evidence is `offline-validation2.json`, `offline-build2.log` and `offline-arm-check2.log`. Native payload is `offline-payload2.tar`, containing only Rust-produced diagnostic executables and provenance.

Probe SHA256: `d60f656b58586f6120952c6d9e7a5878d3fd3eaa7ac941f157450d6aa323ad59`.

Native test SHA256: `5e87b00bbcb9e2429dc15fc437d450b4d451fbbce2c053018ba44b316f5a4806`.

Original-vendor v2 VM control passed a 246.3-second monitor after read-only LLDB attach/detach. Actual map, city and F1 Domestic Advisor were healthy, screenshots 075, 076 and 077. Normal GUI Quit then Yes exited 0 without external TERM. There were zero unhandled fault signatures. All owned game/probe/helper/debugger/server processes were gone and the private view was removed afterward. Child-only debugger disable and a first-fatal watchdog bounded this diagnostic.

Read-only LLDB image list and lsof prove exactly one native ntdll.so, from the private view, with no original ntdll mapping. The private wow64cpu.dll file hash and mapped branch/stub bytes matched exactly. Evidence:

- `/mnt/datasets/c3r-build/vm-qa31b-sol/private-view-v2-cpu.{json,log}`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/vendor-offline-v2-path-proof.log`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/vendor-offline-v2-verdict.json`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/vendor-offline-v2-evidence.tar`, SHA256 `646017a5c0ea40e79968643fe3b8876e39fafb772db68ebe88947b8d3ee26aad`

A separate 10-second Miles MP3 control through production private-view v2 passed exit 0. It loaded existing read-only Mss32.dll/redist/Menu1.mp3 in place, with the historical live diagnostic disabled. Playback position advanced, followed by successful halt/unload/shutdown. There were no unhandled faults or remaining owned Wine/helper/debugger processes and the private view cleaned. Evidence `vm-qa31b-sol/miles-offline-v2.{json,log}`. This proves decoding, playback position and clean release. USB-null audio does not prove audible output.

## Diagnosis and rejected controls

Historical bridge60931 and native31b runs reported stack overflow on the first stack-local read after Wine Unix calls. The bridge address was Presenter::preload_ready, not mmdevapi. Two other faults were mmdevapi GetCurrentPadding and ReleaseBuffer. Two unmodified original-vendor runs, with no Rust modules, reproduced after about 50 and 82.8 seconds.

Native LLDB verified ample 32-bit and 64-bit stack capacity. Wine maps native #SS directly to EXCEPTION_STACK_OVERFLOW. Raw trap number was not captured, so this remains an experimentally supported SS return-state diagnosis, not a claim of a proven kernel defect. The guarded live SS-reload diagnostic survived more than 246 seconds and actual city/advisor/normal shutdown, but was diagnostic only. Both 2 MiB and Civ3-matched 1 MiB DirectSound controls passed 240 seconds. Their success did not isolate a reproducer.

V1 private-view startup proved the patched CPU DLL selection but failed before GUI. Symlinked Unixlibs loaded a second original ntdll, followed by repeated native PAGE FAULT reads at 0x36e and a debugger storm. VM owner atomically stopped the exact owned game and 606 escaped debuggers, excluding the shared server. Its probe exit 0 followed external TERM and is explicitly not a pass. The v2 child-local dyld path fixed the duplicate ntdll mapping and passed the scoped runtime control above.

The historical native31b live-diagnostic process reached a map but the guest rebooted during a later QMP pause/resume interval. No normal-quit claim is made for that run. It does not establish current-renderer acceptance.

Machine-readable history is `mac-audio-sol/crash-offsets.json`. Raw original crash controls are under `vm-qa31b-sol/vendor-crash-evidence/` and `vendor-repeat-evidence/`; guarded-live and v1 failure bundles remain with the VM owner.

## Remaining work and limits

Root and `/root/vm_snapshot_sol` must run the current frozen native renderer and combined production launcher through startup, map/city/advisor/UI and normal shutdown, including strict descendant cleanup. Native ARM64 runtime/Metal behavior remains separate from Intel VM compatibility. TS Neo is offline; do not test on the physical Mac. VM owner retains the sole process/UI lease and aggregate VMs stay under 10,000,000,000 bytes.

All completed owned build targets were removed after retaining source, payloads, hashes and logs. Exact official Wine 11.0_1 archive remains at `/mnt/datasets/c3r-build/macos-port/wine-macos/wine-stable-11.0_1-osx64.tar.xz`, SHA256 `b50dc50ec7f41d58b115a6b685d4d1315ba3c797bd3aa0f49213f2703cb82388`. No game assets were copied.

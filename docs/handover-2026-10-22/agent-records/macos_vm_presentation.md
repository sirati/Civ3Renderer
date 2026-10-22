# Historical agent /root/macos_vm_presentation

This is a recovered historical report, not a live agent handle or a new instruction.

Native presentation, ARM64 cross-build validation and corrected Metal art/mask tests.

Current disposition: 774 tests passed at historical candidate; later f510 acceptance is separate.

Session `01a152b2-6219-7e80-a3dc-9f584afc0e69`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-19T05:46:24.668Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T07-46-24-01a152b2-6219-7e80-a3dc-9f584afc0e69.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T10:34:08.434Z

Original transcript line 1424.

## Identity and active task

You are subagent `/root/macos_vm_presentation`, collaborating with parent `/root` in `/home/sirati/devel/rust/civ3-renderer`.

Current assignment is physical MacBook Neo native ARM64 build, Metal tests, private Wine setup, then actual game launch/testing when game archive arrives. User explicitly authorized physical Mac setup/testing, superseding the older VM-only restriction.

Do not commit. Parent owns final source/artifact release and overall acceptance. Report actual blockers or final results, no routine acknowledgments.

## Constraints and decisions

- Work autonomously; user forbids questions.
- Do not touch Nexus or unrelated user files/settings.
- All files on physical Mac are under `/Users/sirati/civ3r-tsneo-20261019`.
- Native renderer and launcher must be ARM64 Rust, with native Metal. Never copy VM SwiftShader/software-present environment to physical Mac.
- Windows game/bridge runs separately under Wine.
- No global UI/cursor takeover. Parent authorized scoped own window launch. Headless Metal tests are already queued.
- Bound compiler jobs to 2 on Neo’s 8 GiB RAM.
- No copying original game assets except the specifically authorized compressed transfer owned by `macos_vm`.
- Skills read earlier: unslop, waiting, NixOS. Avoid repeated background polling; do orthogonal work and wait longer where appropriate.
[Credential-related line redacted from this historical copy.]

## Physical Mac access and inventory

SSH works using ordinary existing config/agent:

```bash
ssh -o BatchMode=yes -o ConnectTimeout=12 siratis-macbook-neo.tailb624ab.ts.net '...'
```

Peer IP `100.110.132.92`; user `sirati`.
Hardware Apple A18 Pro, ARM64, macOS 26.6 build 25G72, Metal 4.
Display 2408×1506 Retina; logged-in console user sirati.
8 GiB RAM, 6 CPU cores, ~368 GiB free.
CLT SDK installed:

- `/Library/Developer/CommandLineTools`
- `/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk`

Nix installed. Cargo `/run/current-system/sw/bin/cargo` 1.98.0 ARM64.
No rustc on ordinary PATH; use:

```bash
/run/current-system/sw/bin/nix shell nixpkgs#rustc -c cargo ...
```

This selects existing native rustc 1.98.1. No global tool installation needed.
`sudo -n` works.

Network is slow/lossy: direct Tailscale WAN to Taiwan ~301 ms, ~6–10% TCP retransmissions. Long SCP flows fall to 10–40 KiB/s. Prefer compression and parallel chunks for large artifacts. `macos_vm` owns optimized transfer.

Local evidence root:

`/mnt/datasets/c3r-build/macos-port/tsneo/evidence`

Inventory logs already there.

## Exact source and runtime candidate

Parent released source **for build, not final live acceptance**:

- source SHA256 `9b8ebbdbd15b8606f5dd02200679d1b002c627138934ced70ecc0d071b648fe5`
- 2,486,038 bytes; 571 files
- immutable local source:
  `/mnt/datasets/c3r-build/macos-port/staging/source-history/9b8ebbdbd15b8606f5dd02200679d1b002c627138934ced70ecc0d071b648fe5/source.tar.gz`
- source manifest alongside.
- Windows bridge SHA256:
  `ae37bac49d0bc84e55e35f94118ceb6648504ebcf824442d8dd4ec9c38f7ef0e`
- shader SHA256:
  `effdc0cd07fa3d2a89a2692362f9019b2789cf1d8af93c225b38f61674a38a2c`
- font archive:
  `/mnt/datasets/macvm/assets/generated-fonts/generated-font-cache.tar.gz`
  SHA `66eec9ddaf8fe342c4c64fa58aa43c1fa6bca12c3f772352550db7d7b236754a`
- full handoff:
  `/mnt/datasets/c3r-build/macos-port/staging/physical-mac-handoff.json`

Preserved source inputs locally:
`/mnt/datasets/c3r-build/macos-port/tsneo/source-9b8ebbdbd15b8606/`

On Neo source is extracted at:
`/Users/sirati/civ3r-tsneo-20261019/source-9b8ebbdbd15b8606`

All **571 source files verified against embedded SOURCE-MANIFEST.json**, no errors.
Evidence:
`tsneo/evidence/source-files-verified.json`

Never claim older source checkpoint ab9 covers current strict rendering policy.

## Current running build/test jobs

Native build is in progress on Neo. Compilation reached `naga`, objc2 dependencies; crate downloads completed enough to compile. The last check showed normal progress, no build failure.

Remote helper:
`/Users/sirati/civ3r-tsneo-20261019/build-native.sh`

Local copy:
`/mnt/datasets/c3r-build/macos-port/tsneo/build-native.sh`

It uses:

```bash
CARGO_TARGET_DIR=/Users/sirati/civ3r-tsneo-20261019/build
TILE_RENDER_PRECOMPILED_SPV=$source_dir/tile_render_shaders.spv
WGPU_BACKEND=metal
TILE_RENDER_ADAPTER=apple
CARGO_BUILD_JOBS=2
SDKROOT=$(/usr/bin/xcrun --show-sdk-path)
unset TILE_VIEWER_SOFTWARE_PRESENT VK_DRIVER_FILES VK_ICD_FILENAMES
nix shell nixpkgs#rustc -c cargo build --locked --release \
  -p civ3r-viewer -p civ3r-launcher -j2
```

On success installs native binaries into `$root/runtime/bin`, then runs `file` and shasum.

Remote log:
`/Users/sirati/civ3r-tsneo-20261019/logs/native-build.log`

A waiting Python helper launched the build only after verified source/shader upload:
`wait-build.py`
It writes `logs/native-build.exit` when done.
Native build SSH tool session **69625** remains active.

A first premature build attempt failed before source upload completed, only with “could not find Cargo.toml.” Corrected by wait helper. Do not misreport as port failure. Old local `native-build-summary.log` contains that obsolete attempt.

Metal tests queued after `native-build.exit == 0`:
Remote `/Users/sirati/civ3r-tsneo-20261019/metal-tests.sh`
SSH session **54712**.

Runs release production GPU integration tests, serial GPU tests:

```bash
cargo test --locked --release -p tile-render \
 --test gpu_matches_reference --test indexed_matches_reference \
 --test text_matches_reference --test cached_picture -j2 -- --test-threads=1
cargo test --locked --release -p civ3r-launcher -p viewer-transport \
 -p civ3r-viewer -j2 -- --test-threads=1
```

Same Metal environment; no VM/software overrides.
Remote log `logs/metal-tests.log`.

Need collect complete logs/results and ARM64 file/hash proofs after finish.
Potential improvement: actual adapter info proof via tiny Rust probe or production viewer log. Existing example `atlas_transfer_profile` prints adapter but requires timestamp support and is more intrusive. Native viewer starts adapter probe before opening a window and can be run with token/ready while no game until 120-second timeout; could collect Metal adapter log without UI. Actual game should provide proof once ready.

## Rosetta installed successfully, just now

Command under parent’s explicit correction:

```bash
sudo -n /usr/sbin/softwareupdate --install-rosetta --agree-to-license
```

Succeeded; `arch -x86_64 /usr/bin/uname -m` returns `x86_64`.

Local evidence:
`/mnt/datasets/c3r-build/macos-port/tsneo/evidence/rosetta-install.log`

There was harmless Apple package-authoring warning about missing installKBytes, but final result:
`Install of Rosetta 2 finished successfully`.

**Notify parent this blocker is resolved.** Haven’t sent this success yet.

## Wine preparation

No preexisting Wine/CrossOver/Whisky found on Neo.

Verified upstream Wine package:
`https://github.com/Gcenx/macOS_Wine_builds/releases/download/11.0_1/wine-stable-11.0_1-osx64.tar.xz`
SHA:
`b50dc50ec7f41d58b115a6b685d4d1315ba3c797bd3aa0f49213f2703cb82388`
size 185,303,032 bytes.

Initial SCP was too slow and explicitly canceled own PIDs 3694321/3694325.
Its partial file remains under uploads and can later remove only owned partial.

Direct `curl` download is running on Neo, SSH tool session **98322**:
destination:
`/Users/sirati/civ3r-tsneo-20261019/uploads/wine-stable-11.0_1-osx64.direct.tar.xz`
log `logs/wine-download.log`.
Last observed ~12 MiB, growing ~100–200 KiB/sec; likely ~15–25 minutes total.

Remote `prepare-wine.py` waits for full upstream hash, then extracts to:
`/Users/sirati/civ3r-tsneo-20261019/wine-runtime`
Wine binary:
`.../wine-runtime/Wine Stable.app/Contents/Resources/wine/bin/wine`
wineserver adjacent.

Preparation SSH session **11350** running, log `logs/wine-prepare.log`, result `logs/wine-prepared.json`.

It currently writes “Rosetta absent, execution prepared only” in JSON because written before install. **Correct this helper/result statement once prepared**; Rosetta now installed.

The VM’s Wine package required a GStreamer framework for winegstreamer, but game may work without it. Avoid system GStreamer installer unnecessarily. Host dependency metadata:
`/mnt/datasets/c3r-build/macos-port/wine-macos/macho-dependencies.json`.
Can inspect after extraction with `otool -L`. Wine base package bundles many libs. First run `wine --version` under Rosetta, then 32-bit smoke using existing PE:
`/mnt/datasets/c3r-build/macos-port/wine-macos/wine32-smoke.exe`
SHA from old helper:
`3ec5aaefb2fce49e2e1345a77c4a74a8d3afd2cfbccc58ad9f918ba866b692b2`.

Wine runtime should remain private; prefix under scoped root or runtime/wine-prefix.

## Artifact transfers owned by macos_vm

Agent `/root/macos_vm` owns compressed original game transfer, DLL, fonts.

The slow **uncompressed bridge SCP was canceled**, specifically local PIDs **3725060 and 3725061**, and agent notified.
Partial remote `$root/runtime/bin/IFC23.dll` was ~2.2 MiB, not valid. Do **not** launch until macos_vm replaces it and verifies ae37 digest.

macos_vm said they will:

- Compress bridge ae37 in `.tar.xz`, parallel chunks, atomically replace exact remote runtime/bin/IFC23.dll while preserving partial.
- Upload generated font archive into `$root/runtime/fonts`.
- Compress canonical full game (8.6 GB) using zstd -19 long27 T3 with bounded 3 GiB/300% CPU, resulting archive ~1.1 GiB and still compressing.
- Upload game in resumable parallel checksum chunks.
- Build zstd CLI natively from official 1.5.7 source **as transfer tooling only**, inside their `game-transfer/tools`, since no zstd on Neo. They got 2.4 MiB source transfer complete.
- Game data should be extracted under scoped root’s game-transfer area; coordinate exact Conquests path.

Send agent Rosetta success too when useful.

## Launch configuration after readiness

Native Rust launcher:
`$root/runtime/bin/civ3-play-macos`

Set:

```bash
CIV3R_HOME=$root/runtime
CIV3R_VIEWER_BIN=$root/runtime/bin/civ3r-viewer
CIV3R_BRIDGE=$root/runtime/bin/IFC23.dll
CIV3R_WINE="$root/wine-runtime/Wine Stable.app/Contents/Resources/wine/bin/wine"
WINEPREFIX=$root/runtime/wine-prefix
WGPU_BACKEND=metal
TILE_RENDER_ADAPTER=apple
# no TILE_VIEWER_SOFTWARE_PRESENT, VK_DRIVER_FILES, VK_ICD_FILENAMES
"$root/runtime/bin/civ3-play-macos" --game "$actual_conquests" --window
```

Launcher makes private symlink-backed game install and mutable state, no original game copies/modifications. Initializes Wine registry. Default strict replacement suppresses original pixel rendering and kills owned game when viewer closes. `--compare` explicitly enables original rendering. `--compatibility-fallback` explicitly allows fallback.

`CIV3R_FONT_CACHE` defaults to Windows Z: mapped `$CIV3R_HOME/fonts`, same generated sheets used by both processes.

No full-screen default for initial physical launch; pass `--window`.
Parent owns GUI handoff if needed; coordinate before pointer/capture checks. Headless GPU tests safe and already queued.

## Earlier completed work owned by this agent

1. Added optional VM presentation path:

   - `crates/tile-viewer/src/presentation.rs`
   - `crates/tile-viewer/src/presentation/bitmap.rs`
   - narrow init/draw/refactor lib.rs
   - macOS softbuffer dependency 0.4.8 with default features false.
   - `TILE_VIEWER_SOFTWARE_PRESENT=1` retains exact production TileRenderer GPU shaders and app logic, readback then CoreGraphics via softbuffer.
   - reusable target/staging buffer, 64 MiB readback cap, 30 sec bound.
   - `#![forbid(unsafe_code)]`, softbuffer uses Rust objc2 typed framework bindings.
   - focused tests + Darwin cross-check passed.
   - Do not use this diagnostic path on physical Neo.

2. Fixed transient “Missing art” on first city founding:

   - Cause mandatory notice plate requested optional art; scheduler thought mandatory requests complete.
   - `ui_screen::mandatory_items` now same list for art registration and readiness.
   - generic `add_art` supports iterators; live.rs uses unified mandatory set.
   - seven readiness regressions, i686 production check passed.
   - Parent has integrated and rebuilt already.

3. Fixed cargo-gpu nested build deadlock:

   - Real cause global Cargo `build.target-dir`; CargoCmd removes env CARGO_TARGET_DIR but still global config points at outer locked target.
   - `crates/tile-render/build.rs` spawns itself as Rust child with scoped `CARGO_BUILD_TARGET_DIR=codegen source install_dir/target`.
   - shader builder explicit original selected target `/spirv-builder`.
   - new `crates/tile-render/build_support.rs`, forbid unsafe; bounded actual nested Cargo regression and env scoping test.
   - control reproduced deadlock after 2 sec, regression passed, normal no-SPV-override source check passed 32 sec. staged shader same effdc hash.
   - README explains.
   - Parent incorporated in released source.

4. Linux external full smoke passed with previous candidate:

   - title, new game setup/map, Rome founding/city, F1, Alt, viewer shutdown/fallback.
   - 274 art generations/188067 sprites.
   - all private processes/overlay cleaned; no canonical modifications.
   - evidence `/mnt/datasets/c3r-build/macos-port/linux-external-smoke/evidence/result.json`
   - Don’t claim this proves latest strict-policy release; latest tests parent/other agents handle.

## Next steps

1. Notify root/macOS archive agent Rosetta installed and x86 translation verified.
2. Let native ARM build finish; inspect errors if any, fix only scoped relevant issue with root coordination.
3. Collect `file`/hashes, native Metal GPU tests + launcher/transport/host tests logs. Ensure test process actually runs on ARM64 and Metal.
4. Wait for verified private Wine download extraction; run Wine version and 32-bit smoke in private prefix under Rosetta. Resolve actual runtime dependencies locally if possible.
5. macos_vm supplies correct DLL/fonts/game path; verify hashes/readiness before launch.
6. Launch exact native ARM64 viewer+Rust launcher+Win32 bridge in own window, native Metal. Test actual game according root GUI coordination, preserve logs/evidence; no global takeover.
7. Report concrete success or actual blocker, distinguish native Metal/headless tests from game/UI validation.
8. No premature final claim; task remains active.

## Report at 2026-10-19T10:34:10.837Z

Original transcript line 1434.

I’m checking the recovered session and current build state so I can continue without repeating work. I’ll keep testing in the macOS VM unless the recovered history confirms that you later authorized the physical Mac.

## Report at 2026-10-19T11:29:24.037Z

Original transcript line 2939.

## Identity and current assignment

You are subagent `/root/macos_vm_presentation`, working with parent `/root` in `/home/sirati/devel/rust/civ3-renderer`.

The physical Mac work is complete enough for the parent’s accepted scope. Finish evidence collection, send the tested command to `/root/rust_launcher`, and deliver your owner report. Do not commit.

Latest parent instruction:

> Preserve the active physical game session and all user input; do not restart/close it for screenshots or extra instrumentation. Existing Metal54reference+393platform/UIchecks, actual bridge/FEX launch, presentation/frame telemetry and observed interaction are valid scoped evidence. Record screenshot refusal as a limit, not product failure or visual-pass claim. Collect exact final launch/env/runtime architecture/default policy logs read-only, give rust_launcher tested command, and finish owner report. No new screenshot permission prompt or global desktop changes.

## Constraints and authorization

- Work autonomously. No user questions.
- Later user authorization explicitly permitted physical Mac testing, superseding the initial VM-only restriction.
- User now prefers native ARM64 Wine with FEX translation. This is working.
- Preserve the active session. A human appears to be interacting with settings and the window.
- No global cursor/input takeover, permission prompts, or desktop changes.
- Do not touch Nexus or unrelated files.
- Mac task root is `/Users/sirati/civ3r-tsneo-20261019`.
- Bound native compiler jobs to 2.
- Native renderer uses Metal, never VM SwiftShader/software presentation.
- Skills already read/applied: unslop, waiting, NixOS. Avoid progress polling.

## Mac access

```sh
ssh -S /mnt/datasets/c3r-build/macos-port/tsneo/ssh.sock \
  siratis-macbook-neo.tailb624ab.ts.net '...'
```

Use the existing control socket. Source transfers previously saturated SSH admissions.

Host: Apple A18 Pro, ARM64, macOS 26.6, user `sirati`, UID 501. Aqua domain is `gui/501`.

## Installed production artifacts

Under `$root/runtime/bin`:

| File | SHA256 |
|---|---|
| `civ3r-viewer`, Mach-O ARM64 | `833fe3b60c3a67f088c9b0bac5e0feaa02edddf98b2e3f6392ef71b42b039fe9` |
| `civ3-play-macos`, Mach-O ARM64 | `6a49f310367d4c885bbdfe6d691d502736fb9b17fdd1f62bff8f58982ebdd230` |
| `IFC23.dll`, PE32 i386 | `e932ab8083d9b7c67529a91e692e30ca5ab0cdeb500af8b0dda952640936885e` |

Production shader SHA:

`252dcce1c11c3a5179fcdec99bdfda25a8102736e2bc4dfbf4336671bbbc2805`

Local immutable shader:

`/mnt/datasets/c3r-build/macos-port/tsneo/candidates/05-wide-unrolled/shader.spv`

Native binaries built from source archive:

`3411ae82cb3e986ed0befe553d2496190f16424eb2a65c45fe51ebc017e5c393`

All 572 source files verified before building. Remote source directory:

`$root/source-3411ae82cb3e986e`

Final source package was rebound to:

`2d084a0e657cb39b3f34d81ccc6e21b15aafb12052d97848b4c282725ba7db5f`

We independently verified 435 native source/dependency files, including launcher files, unchanged between packages and matching actual remote build sources. Only Windows bridge/sys files and README differed.

Local rebind evidence:

`/mnt/datasets/c3r-build/macos-port/tsneo/evidence/final-native-source-rebind-2d084a0e.json`

## Completed checks

- 54/54 production GPU/reference integration tests passed on real Apple A18 Pro Metal.
- Same 54/54 passed on Linux RADV.
- 17/17 shared arithmetic tests passed on Linux.
- 367 native platform/UI/transport/launcher/render-desc tests passed.
- Additional `menu` suite: 23/23 passed on ARM.
- All 3 explicitly ignored tile-viewer GPU tests passed on ARM Metal:
  - `delayed_required_growth_does_not_activate_an_intermediate_whole`
  - `tight_peak_limit_replays_directly_into_mandatory_final_capacity`
  - `gpu_draws_intermediate_menu_rows_with_matching_scaled_clips`
- Native platform/UI total is therefore **393 passed**, with those previously ignored GPU cases now exercised.
- No test tolerances or drawing thresholds were relaxed.

Logs are under:

`/mnt/datasets/c3r-build/macos-port/tsneo/evidence/`

Relevant files:

- `metal-wide-unrolled-05.log`
- `linux-wide-unrolled-05.log`
- `shared-wide-unrolled-05.log`
- `final-native-3411.json`
- `final-native-build-3411.log`
- `final-native-host-tests-3411.log`
- `final-native-menu-3411.log`
- `final-native-viewer-gpu-3411.log`
- `metal-loop-diagnosis.json`
- `metal-scale-probe-final.log`

## Metal fixes completed in shared text code

Only current new source edit owned here:

`crates/tile-render-shared/src/text.rs`

Final file SHA:

`7ca79e3e9cdf77bc5a1b8dc9b47d3747bf607c5867b4c199dc6c78f19b968084`

Naga SPIR-V-to-MSL lowering exported stale loop values:

1. Curve crossing used five refinements instead of six. Compute the scalar result within the final sixth iteration.
2. Keyed text dropped blue-only fringes or final blue output. Compute three channels explicitly.
3. Wide multiplication/division returned the quotient after 31 steps. Explicitly unroll all 32 long-division steps with a macro.

Independent reviewer `/root/ui_feature_audit` confirmed arithmetic, saturation, macro count, and unchanged semantics.

Direct Metal compute proof after the fix:

- `[1024,957,1024,1000]`: GPU 5879, CPU 5879
- `[16384,4095,4096,2048]`: GPU 49140, CPU 49140
- `[1024,4095,4096,1000]`: GPU 6289, CPU 6289

Temporary fast-math disabling did not help and was restored. No production vendor Metal backend changes remain.

Real font fixture used for equivalent native tests:

`$root/tests/real-font/LSANS.TTF`

SHA `c8684db174c17046c92f60eed5e4356cc14a2469526be24232254b6cd5712254`

## Preferred native Wine is working

Owner `/root/ui_feature_audit` installed native Wine 11.18 with Hadron 0.3.1/FEX, preserving the signed upstream loader. Rebuilt same pinned Wine plus 38 patches for macOS 26.6 using `ac_cv_func_pipe2=no`.

Both 32-bit and 64-bit console runtime smokes passed. FEX modules were observed in logs.

Preferred runtime:

```sh
CIV3R_WINE="$root/native-wine/runtime/dist/bin/wine"
WINEPREFIX="$root/native-wine/prefix"
DYLD_FALLBACK_LIBRARY_PATH="$root/native-wine/runtime/dist/ext/lib"
FEX_DISKCACHE=1
WINEARCH=win64
WINEDLLOVERRIDES='mscoree,mshtml='
```

Native core architecture/hash proofs:

- Wine ARM64 loader: `e28e5a13e3f4f3cb36c5467b64fd6f83ad36c0c19b9e07b4d86528ba83c833c5`
- ARM64 wineserver: `e76249aefcd2326ef04050875ae6643dd1621efccef6c0baf7baa406848d424f`
- ARM64 `ntdll.so`: `e4127d03956284be70739849c26f50e9667b78505348059f73deee95046daa9a`

Do not say “only game code is translated.” Some x86 PE compatibility DLLs also use FEX.

Old x86 Wine 11 under Rosetta remains a tested explicit fallback at `$root/wine-runtime/...`. It is not the preferred stack.

## Full game is installed

Owner `/root/macos_vm` verified all:

- 77,020 files
- 9,023,775,933 bytes
- 4,391 directories
- zero mismatches, all mods/saves retained

Canonical game:

`$root/game-transfer/extracted/Civilization III Complete/Conquests`

Archive SHA:

`a785bb6198c6089af5ebef22a4d5651e9f771ac98dad6191faa90be0213e2e20`

Fonts are installed at `$root/runtime/fonts`, all 626 files verified. Launcher makes its private symlink-backed game under:

`$root/runtime/game/Civilization III Complete/Conquests`

No additional full game copy.

## Active physical game session: preserve it

Successfully launched preferred native Wine/FEX plus native ARM64 Metal viewer through a temporary Aqua LaunchAgent:

- Label: `org.civ3r.qa.tsneo-20261019`
- Domain: `gui/501`
- Plist: `$root/launch-native-game.plist`
- Launcher PID originally 52580
- Viewer PID 52936
- Main window ID 1862, fullscreen toolbar window 1882

**Do not boot out the job, close the viewer, restart Wine, or inject input.**

Actual command:

```sh
"$root/runtime/bin/civ3-play-macos" \
  --game "$root/game-transfer/extracted/Civilization III Complete/Conquests" \
  --window
```

Environment additionally included:

```sh
CIV3R_HOME="$root/runtime"
CIV3R_VIEWER_BIN="$root/runtime/bin/civ3r-viewer"
CIV3R_BRIDGE="$root/runtime/bin/IFC23.dll"
WGPU_BACKEND=metal
TILE_RENDER_ADAPTER=apple
TILE_VIEWER_SOFTWARE_PRESENT=0
DYLD_LIBRARY_PATH=''
CIV3R_GAME_RENDER=off
WINEDEBUG=+loaddll
TILE_VIEWER_FRAME_TIMES=1
CIV3R_PIXEL_AUDIT=on
```

Use the preferred Wine variables above too. `WINE_MAC_OPENGL` was left unset.

Observed live proof:

- `using adapter Apple A18 Pro (Metal)`
- Successful surface presentation/frame telemetry
- Growing native art, over 8,500 sprites
- `xtajit.dll` loaded, followed by the correct private `ifc23.dll`
- Native policy:
  `original pixels=false, compatibility fallback=false, map pixels off=true, pixel audit=true`
- Repeated settings changes and edge-scroll events. A human appears to be using the session.
- Nonfatal Wine EGL library warnings occurred, but actual game/native presentation works. Do not switch graphics drivers or rebuild runtime now.

## Screenshot limitation

Scoped `/usr/sbin/screencapture -x -l1862 ...` and current toolbar capture both returned:

`could not create image from window`

Do not request permissions or attempt global screenshots. Parent accepted this as a limit, not a product failure. Do not claim visual screenshot acceptance.

## Evidence just collected; next steps

A read-only snapshot directory was created on the Mac:

`$root/logs/physical-acceptance/`

Contains:

- `result.json`, exact launch environment, hashes/architectures, backend/policy/FEX evidence
- snapshots of launcher, viewer, bridge and Wine logs

SCP of that directory is currently tool session **49265**, destination:

`/mnt/datasets/c3r-build/macos-port/tsneo/evidence/physical-acceptance/`

Another small evidence SCP session **2490** may need collecting completion, though files likely already finished.

Next:

1. Await SCP 49265 and verify local `physical-acceptance/result.json`.
2. Optionally change only evidence metadata `visual_acceptance` from `passed:false` to a clearer unverified/not-performed status. It currently explicitly says capture refused, no permission prompt. Do not treat it as product test failure.
3. Send `/root/rust_launcher` the actual successful command/environment and Aqua context. Its guide draft already uses preferred Wine but awaited GUI proof.
4. Send parent `/root` the final owner report, linking evidence, stating 393 native tests + 54 Metal checks, working native Wine/FEX game/Metal launch, source rebind, screenshot limit, and active session preserved.
5. Finish this subagent’s work. No more tests, UI actions, restarts, or source edits are needed unless parent directs them.

Current helper/window observer is `$root/list-owned-windows`, test-only Swift outside product source. No product launcher/render code was written in Swift.

Owned Wine download fragments totaling 186,086,392 bytes were cleaned up after verifying the retained upstream archive. No game/runtime files were removed.

## Report at 2026-10-19T11:34:48.506Z

Original transcript line 3166.

Current evidence is complete and verified:

- [Owner report](/mnt/datasets/c3r-build/macos-port/tsneo/evidence/physical-owner-report.json)
- [Actual launch, architectures and policy](/mnt/datasets/c3r-build/macos-port/tsneo/evidence/physical-acceptance/result.json)
- [Fresh Conquests/menu failure logs](/mnt/datasets/c3r-build/macos-port/tsneo/evidence/menu-state-20261019T113051Z/result.json)
- [Native source verification](/mnt/datasets/c3r-build/macos-port/tsneo/evidence/final-native-source-rebind-2d084a0e.json)

393 native tests and 54 Apple Metal reference checks passed. Native ARM64 Wine/FEX launched Civ III with the ARM64 Metal viewer. Screenshot acceptance remains unverified because scoped capture was refused.

Fresh logs identify mandatory menu art cancellation before preview publication. The viewer subsequently closed, and the launcher stopped its owned game. I did not stop or restart anything.

The scoped build/staging helper is ready for your exact source release and affected test filters. It verifies source and embedded shader, builds with two jobs, and stages the viewer separately. No normal runtime binaries have changed.

## Report at 2026-10-19T11:50:11.967Z

Original transcript line 3415.

Installed source `04d7edae` on Neo:

- ARM64 viewer `2f992f97` and matching bridge `dc782f8c` are at the normal runtime paths.
- All 572 source files and embedded shader were verified.
- Eight affected native regression tests passed.
- Immediate process checks found no active game, viewer or launcher before atomic installation.
- Exact launch instructions are installed at `/Users/sirati/civ3r-tsneo-20261019/RUN.md`, hash `f02da418`.

[Final owner report](/mnt/datasets/c3r-build/macos-port/tsneo/evidence/release-04d7edae08e79234/owner-report.json) includes build logs, tests, installation checks and guide verification.

No physical GUI session or input was started. Earlier full-suite and Wine/FEX presentation evidence remains explicitly tied to the baseline artifacts.

## Report at 2026-10-19T12:00:51.190Z

Original transcript line 3461.

Camera-specific helpers are ready. They require the new camera regressions, all 54 Metal reference checks, exact embedded shader verification and a matching DLL source binding.

The same guarded installation policy will preserve any active Mac session. Awaiting the exact source freeze.

## Report at 2026-10-19T13:32:21.828Z

Original transcript line 4541.

## Identity and current assignment

You are subagent `/root/macos_vm_presentation`, working with parent `/root` in `/home/sirati/devel/rust/civ3-renderer`.

The early native renderer/icon validation is complete and passed. Parent has received the evidence. Await the parent’s exact final aggregate source release before building or installing a production candidate.

Do not commit.

## Constraints

- Work autonomously. No user questions.
- Preserve any physical Mac game/viewer session and all user input.
- No game GUI launches, screenshots, window activation or injected input on Neo.
- Native offscreen Metal tests and separate noninteractive icon processes were explicitly authorized.
- Use at most two compiler jobs.
- Do not touch Nexus or copy game files.
- Do not modify immutable checkpoints or production files before the final release authorization.
- Parent’s boss instructions forbid acknowledgement-only messages. Send findings, blockers, answers or final evidence.
- Skills already read: unslop, waiting, NixOS and boss.
- No background commands remain active.

## Mac access and installed production state

```sh
ssh -S /mnt/datasets/c3r-build/macos-port/tsneo/ssh.sock \
  siratis-macbook-neo.tailb624ab.ts.net '...'
```

Host is Apple A18 Pro ARM64, macOS 26.6, user `sirati`, UID 501.

Task root:

```text
/Users/sirati/civ3r-tsneo-20261019
```

Neo went offline during evidence collection. Bounded read-only recovery succeeded after 19 attempts at `2026-10-19 12:58 UTC`. It is reachable again. Recovery evidence:

```text
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/early-efd15ad1-connectivity.json
```

Current normal production files remain from source:

```text
04d7edae08e792341a334e4850877ac14ea62afc95c870870b0933f19b68fc40
```

Installed hashes:

- `runtime/bin/civ3r-viewer`: `2f992f973e4d24ffc104a1051c13e4cc77beb4bd86e9c242843d05d08aa9f648`
- `runtime/bin/IFC23.dll`: `dc782f8c44f06d764ccfb6f287abb4089d7ae2bc567c65ab6dda3a930727a9c9`
- `runtime/bin/civ3-play-macos`: `6a49f310367d4c885bbdfe6d691d502736fb9b17fdd1f62bff8f58982ebdd230`
- `RUN.md`: `f02da41873b0cbf5dca422c33c20b6afea8107248950100490dba306b7092689`

Source04 was installed atomically only after immediate process checks found no game, launcher or viewer. No new production installation occurred during the current early validation.

Previous installation evidence:

```text
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/release-04d7edae08e79234/owner-report.json
```

## Early checkpoint and renderer validation

Immutable early source:

```text
/mnt/datasets/c3r-build/macos-port/ui-background-dither/metal-checkpoints/efd15ad119d4d2977853b4f047648cd0b8afe5c6ce4db1dfbc80dd3d81ebbba4/source.tar.gz
```

- Source SHA: `efd15ad119d4d2977853b4f047648cd0b8afe5c6ce4db1dfbc80dd3d81ebbba4`
- Archive: 2,501,821 bytes, 584 manifest files.
- This captures WIP viewer/settings code. It was authorized only for stable renderer/icon checks, not a production viewer build.

New RustGPU module:

```text
/mnt/datasets/c3r-build/macos-port/ui-background-dither/candidates/12a8ce7134808767486cd9b5fe3e01932827eb855d89b4d29f7834f419a1a38f/shader.spv
```

- SHA: `12a8ce7134808767486cd9b5fe3e01932827eb855d89b4d29f7834f419a1a38f`
- Size: 377,616 bytes.
- Shared text source remains `7ca79e3e9cdf77bc5a1b8dc9b47d3747bf607c5867b4c199dc6c78f19b968084`.

Passed on real Apple A18 Pro Metal:

- Four camera unit tests.
- 59 integration cases across `cached_picture`, `gpu_matches_reference`, `indexed_matches_reference`, `text_matches_reference`, and `ui_background_sampling`.
- All five integration executables are ARM64 and contain the exact new SPIR-V.
- Adapter logs explicitly identify Apple A18 Pro, Metal.
- Canonical real LSANS font SHA remains `c8684db174c17046c92f60eed5e4356cc14a2469526be24232254b6cd5712254`.
- All 584 source files and production hashes remained unchanged.

Critical cache finding: the old shared `$root/build` reused a stale `tile-render-shared` artifact and caused missing-field compilation errors. Those results were discarded. The passing run used the fresh isolated target:

```text
/Users/sirati/civ3r-tsneo-20261019/validation/efd15ad119d4d2977853b4f047648cd0b8afe5c6ce4db1dfbc80dd3d81ebbba4/target
```

Do not reuse contaminated shared-target results.

## Local completed evidence

Main handoff:

```text
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/early-efd15ad1/owner-report.json
```

Independent renderer evidence, already locally hash-verified:

```text
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/early-efd15ad1/renderer-acceptance/result.json
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/early-efd15ad1/renderer-acceptance/camera.log
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/early-efd15ad1/renderer-acceptance/metal-reference.log
```

Rebinding aids:

```text
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/early-efd15ad1-source-closure.json
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/early-efd15ad1-renderer-lock-closure.json
```

The source closure records 119 conservative files, including a separate 103-file runtime subset. The lock closure records 207 transitive package entries for `tile-render`, useful if unrelated workspace changes alter the whole Cargo.lock hash.

## Icon diagnosis and final passing check

The checkpoint accidentally omitted the PNG binary. Root knows and will verify inclusion in the final archive. An external fixture was used without changing the checkpoint:

- PNG: 329 bytes, 32×32.
- SHA: `bd0bc503c55ea68d53d471694a35cd1b129e75a31f8bf9704f19974ed5273afc`
- Matches immutable `icon-provenance.json`.
- Original decoded RGBA SHA: `6fbb8e4a5066160380c721f9ca864c26c40d2c0f0e191313a34daa50c0b08e6a`.

The original TIFF-byte comparison failed because AppKit returns a 64×64 Retina bitmap with the display ICC profile for a 32×32 logical icon. Root explicitly accepted this as expected native behavior. Do not force a 32-pixel representation or change production behavior.

Several exploratory raster comparisons were retained but are not acceptance results. Parent stopped that investigation and approved an exact standard AppKit control oracle.

The final passing candidate:

```text
/mnt/datasets/c3r-build/macos-port/icon-work/verify-native-control-candidate.rs
```

SHA:

```text
b72613ff4d0f98efd699b01e0f4e4d2cb4a6c39495769aaa74ff1ee4a8787c77
```

It:

1. Verifies all original PNG RGBA pixels.
2. Captures standard AppKit icon output directly.
3. Installs a distinct opaque cyan marker and proves it differs.
4. Calls the production wrapper.
5. Requires exact restoration of all 16,384 native RGBA bytes, ICC profile, backing dimensions and logical size.
6. Checks malformed-data retention.
7. Uses a valid PNG on a worker and requires exact `Error::MainThread`.
8. Verifies the installed icon remains intact.

Passed natively. Binary SHA:

```text
06199e02a2da1aa3f598fb76b7c0cdb07d78abb5bbe7a4a7f49b41ad436d02bb
```

Production backend is unchanged:

```text
66ead5daa0fd6d9e1aa2333ceb3e1763968004e60ebbceb2505a84540351b251
```

`/root/macos_icon` has now copied the passing candidate exactly into repository `verify.rs`, adding only minimal image/AppKit dev dependencies. It explicitly says no further icon runtime experiments are needed.

Local, hash-verified acceptance:

```text
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/early-efd15ad1/icon-acceptance/result.json
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/early-efd15ad1/icon-acceptance/native-control.log
/mnt/datasets/c3r-build/macos-port/tsneo/evidence/early-efd15ad1/icon-acceptance/test-source.rs
```

Historical TIFF/RGBA diagnostics, helpers and failed oracles:

```text
/mnt/datasets/c3r-build/macos-port/icon-work/native-diagnostic/icon-diagnostic-evidence/
```

Parent and icon owner have received the final local acceptance paths.

## Parent’s latest final-release requirements

Await exact aggregate source freeze. Then:

- Build the native ARM64 production viewer using an isolated/dedicated target.
- Coordinate a matching Windows DLL with `/root/macos_bridge`.
- Final viewer and DLL must use the same source/protocol. The aggregate now includes scene wire version 38 and menu-fit changes.
- Preserve the 59 early GPU passes by proving actual renderer dependencies/module unchanged. Do not rerun all 59 unnecessarily.
- Run the newly added masked-strip GPU regression in `ui_background_sampling.rs` on final source. Obtain its exact filter from the released source/owner.
- Run affected final menu metadata, capture, settings and native viewer tests as parent specifies.
- Do not assume final GPU count is exactly 59.
- Original 34-file shader binding includes `menu/src/screen.rs`, which intentionally changed to tag 128 native crops across ten files. Do not claim all 34 still match.
- The new test file also intentionally changes. Distinguish runtime source equivalence from test/producer changes.

Stage under:

```text
$root/runtime/releases/<full-source-hash>/
```

Promotion is allowed only after an immediate check finds no task game, viewer or launcher. If a user session exists, preserve it and keep the candidate at the versioned path. Never launch a new GUI session to verify it.

Existing helper scripts are under `/mnt/datasets/c3r-build/macos-port/tsneo/`. Older build/install helpers contain fixed shader252/source04/eight-test assumptions. Adapt them before using for the aggregate; do not run them unchanged.

Current early checks did not build or install any production viewer. Screenshot, Dock appearance, exhaustive historical-feature and 240 FPS claims remain out of scope.

## Report at 2026-10-19T13:50:06.891Z

Original transcript line 5034.

Final ARM64 candidate passed 774 tests, native AppKit icon validation and exact font verification. Corrected Metal tests cover all 20 art variants and 30 tiny-mask variants.

Viewer SHA: `abaadcf56b43eafad442aa8cab21f2eeb76ecf675c7477c5b72d4c636e1e41e4`.

[Acceptance report](/mnt/datasets/c3r-build/macos-port/tsneo/evidence/release-3a91e2e9dcc38bb3584ec24c6b16eae4f0bf74fcb9a56a943c7f89fed47b8c0f/owner-report.json)

Paired viewer, launcher and DLL are staged under the versioned release directory. Production remains unchanged. No desktop screenshots, window activation or input actions occurred.

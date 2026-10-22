# Native Windows VM validation

Frozen source ef962736b6840284c6dc052d9df989d3539d4b16a83a6a28149ac222e0013754, from release-1019/src-ef962736b6840284c6dc052d9df989d3539d4b16a83a6a28149ac222e0013754.

Own ephemeral VM c3r-sol-frozen started with winvm up --mem 2G --cpus 4 --share /mnt/datasets/c3r-build/windows-vm-sol/share --no-wait, wrapped in systemd-run --user --scope --unit=c3r-windows-sol --slice=c3r-vms.slice. Actual QEMU cgroup verified under c3r-vms.slice/c3r-windows-sol.scope. Existing Mac work VM has 3GiB; new Windows VM2GiB plus overhead fits existing aggregate cap. Slice MemoryMax=10000000000, MemorySwapMax=0 unchanged. Recorded launch-time MemoryCurrent=1155661824. QEMU args and cgroup in windows-vm-sol/evidence. Golden untouched, ephemeral overlay only. No GUI operations, screenshots, game copies or Mac access.

Guest verified Windows11 build26100.1 by QGA cmd.exe /c ver. Own test exes built on Papa using nix develop from immutable snapshot, unique CARGO_TARGET_DIR=/mnt/datasets/c3r-build/windows-vm-sol/target, CARGO_INCREMENTAL=0, TILE_RENDER_ADAPTER=radv, TMPDIR=/mnt/datasets/c3r-build/windows-vm-sol/tmp, TILE_RENDER_PRECOMPILED_SPV pointing at release snapshot artifacts/.../shader.spv. Initial build interrupted early to add canonical shader and TMPDIR, then restarted with those settings. Build command cargo test --locked -p inproc -p civ3-sys -p civ3-bridge --lib --test hooks_x86 --target i686-pc-windows-gnu --no-run --message-format=json. Build evidence in build.jsonl and build.log.

Results and cleanup pending.

## Frozen native results

All tests ran headlessly inside Windows11 build26100.1, compiled i686 on Papa. Executables copied with winvm push. QGA guest-exec commands used --nocapture --test-threads=1. Some later diagnostics used winvm ps over SSH. No GUI operations.

- Required inproc immediate_keys_finish_before_return_and_reject_other_threads: PASS1,26filtered. immediate-keys.log.
- Frozen inproc library:25passed,1failed,1ignored. inproc.log, exit101. The failure releasing_on_focus_loss_precedes_the_original_callback_and_retains_other_keys reproduces alone, focus-isolated.log. Unsupported fixture expectation, not production mutation: scratch keyboard-diag logs show B is already0 immediately after successful SetKeyboardState, before any hook or SendMessage. Same with unintercepted control window. Caps1/NumLock0x81 persist; forwarded modifiers/focus do not change B. Logs keyboard-diag-intercept.log and keyboard-diag-control.log.
- Frozen hooks_x86:1passed. hooks.log, exit0.
- Frozen civ3-sys:119reported passed. sys.log, exit0. Two embedded-executable native binary/accept contract checks self-skip because no game installation, so119reported does not mean119contracts exercised.
- Frozen bridge:399reported passed,3failed,1ignored. bridge.log, exit101. Multiple installation/asset-dependent tests print self-skip; no game assets mounted or copied. Three actual failures listed below. Required targeting/cursor, slider, notices, stale-input and transaction CPU tests otherwise executed within this library run.

Failures:
1. city_labels capital label: synthetic CPU fixture, NOT missing assets. Test filters black alpha255 edge fills, while native smooth_lines=true rasterizes width1 edges at alpha127. Wine/Linux use nonsmooth edges. Geometry itself is unchanged.
2. ui_transaction late art: native GDI fallback loads fake face "No outlines", leaving additional uncompiled font work. Isolated unchanged fixture fails, process-only CIV3R_FONT_CACHE=C:\c3r-tests\empty-font-cache passes. Evidence late-art-isolated.log and late-art-empty-cache.log. text_fit_atomic_ui owns fixture/sign correction.
3. tiny_background_masks_preserve_trimmed_nominal_pixels_at_fractional_phases_and_opacity: genuine GPU comparison mismatch on Microsoft CPU DX12 adapter. Reproduces isolated and with WGPU_VALIDATION=1. Dither rectangle[9,1,1,24], zoom1/2, hairlinefalse,132differing pixels; first(4,6)actual[0,0,0,255],expected[0,62,62,255]. No validation error emitted. Evidence tiny-mask-isolated.log and tiny-mask-validation.log.

Adapter probe: Microsoft Basic Render Driver, CPU, DX12, driver10.0.26100.1, vendor5140 device140. Guest display RedHat VirtIO GPU DOD100.103.104.30200. gpu-info.log and guest-display.log. GL has NoAdapter, tiny-mask-gl.log. Host QEMU uses AMD EGL rendernode; no NVIDIA use. Never label this GPU test as CPU coverage or successful unsupported validation.

Original snapshot reverified after builds: verified624 drift[], frozen-verify.log. One QGA JSON framing collision/stray U+FFFD failed to launch a command; preserved bridge-qga-collision.log. The bridge test was rerun successfully afterward, yielding the399/3/1 result above. GPU adapter probe used PowerShell after another transient QGA framing error.

## Authorized corrections and derived validation

Root explicitly authorized narrow test-only corrections to shared keyboard.rs and city_labels.rs. Keyboard now records the state Windows actually accepted after SetKeyboardState, asserts CapsLock and held NumLock were accepted, then verifies all unrelated256 key entries remain unchanged after modifiers/focus. City verifies semantic line endpoints and explicit smooth/nonsmooth raster output; plate shade assertions exclude partial-alpha edge lines. Production behavior untouched by these edits.

Derived scratch snapshot windows-vm-sol/patched-tests copies original ef source, replacing only keyboard.rs, city_labels.rs, plus text_fit owner's text.rs and ui_transaction.rs. This is NOT the frozen source and NOT a release snapshot. Built with same target/toolchain/shader settings. Original source remains immutable.

Native derived results, patched-cpu-tests.log:
- full inproc26passed,1ignored, exit0;
- city capital1passed, exit0;
- text::fit_tests4reported passed, exit0. New game_width_preserves_the_games_lfheight_sign executes/passes; asset tooltip case self-skips;
- ui_transaction3passed, exit0.


## Broader frozen GPU isolation

Compiled frozen tile-render --test ui_background_sampling --test gpu_matches_reference on Papa, no source changes. Ran through guest PowerShell:
- ui_background_sampling:0passed,7failed, exit101. The mismatch affects general direct UI rendering, not only tiny masks. Example zoom2/1 hairlinetrue:2303mismatches, first(1,0)GPUclear[17,23,31,255]CPU[0,0,0,255]. Halfscale Dither1022mismatches. Full exact camera/first-pixel information in gpu-integrations.log.
- gpu_matches_reference orthogonal_world_is_pixel_exact:1passed,37filtered, exit0.

This isolates a Windows Microsoft CPU DX12 direct UI sprite issue while basic world rendering works. No claim that the GPU backend is unsupported, and no broad renderer changes made. preview_smear/root have results.

Two additional exact frozen GPU probes also fail:
- drawing_at_the_zoom_shows_what_the_scaled_picture_does:zoom3/2,31603pixels differ, first(10,5)GPU[57,0,33,255]CPU[49,0,33,255].
- interface_views_show_only_their_own_sprites:35776pixels differ, first(0,0)GPU[8,8,16,255]CPU[0,8,8,255].
Full gpu-direct-smoke.log. Root/preview_smear investigating coordinate conversion using scratch-only GPU diagnostic; own VM temporarily retained on their explicit request. No release/shared shader edits.

## Scratch arithmetic diagnostic

preview_smear owns scratch diagnostic shader source outside frozen/shared tree. Two attempted graphics probes with Rgba32Uint/Rgba32Float could not create the DX12 pipeline, parameter0x80070057. The compute probe succeeds, outputs x0..15 fast_floor=cast_floor=x and fast_twice_frag=2x+1. fast_alpha=0,1,1,2,2,3,3,3,4,4,5,5,6,6,7,7. This arithmetic probe does not reproduce a bit-rounding defect. Full generated HLSL/debug output and rows preserved coordinate-probe-compute.log. Successful compute executable and SPIR-V preserved in evidence. Diagnostic source crate keyboard-diag and inproc-diag are scratch copies only; inproc-diag exposes from_handle publicly solely to drive message-only test windows, no production source edit.

Further scratch diagnostic results:
- Earlier graphics-probe creation errors were caused by the scratch Cargo root omitting frozen workspace vendor/wgpu-hal patch for32bit DX12 pipeline stream alignment. Applying exact frozen path patch to scratch crate fixes them. Production/frozen test binaries already used that patch.
- Vendor-patched coordinate raster probe succeeds with every x0..15 output[x,x,2x+1,255], coordinate-probe-patched.log. Compute and fragment coordinate bit-rounding are correct in isolation.
- output-probe loads original frozen production SPIR-V fullscreen_vs/output_fs against known16x4 gradient. Zoom1copies correctly; zoom2repeats each texel/row; zoom1/2takes everysecond texel within8x2 and returns zeros outside source bounds. output-probe.log. Production output shader sampling works in isolation.

Dynamic-output-probe also loads original production SPIR-V and binds OutputParams via dynamic32byte uniform views at offsets0,256,1024,4096. All offsets and zoom1,2,1/2 produce correct gradient readbacks. dynamic-output-probe.log, exit0. This rules out simple nonzero dynamic OutputParams offsets in isolation; does not prove the complete renderer path correct.

Actual frozen TileRenderer gradient matrix, scratch renderer-probe.log, isolates direct-at-zoom path:
- Direct and Quantized5bit both match atzoom1with either hairline and atzoom2/half with hairlinefalse.
- Hairlinetrue zoom2has127mismatches, half 31, identical Direct/Quantized. GPU draws only first gradient pixel at0,0 and leaves all other intended sprite pixels clear. CPU expects full scaled gradient. This rules out final quantization as root cause and identifies direct-at-zoom rendering as failing path. Full generatedHLSL and readback rows retained24MB log.

Scratch scaled-branch production shader diagnostic bypasses texel fetch and discard, encoding local offsets/zoom in output. It still draws origin only. Direct hairlinetrue zoom2firstpixel[0,0,33,255], half[0,0,18,255], allremaining intended pixels clear. renderer-scaled-diagnostic.log. Correct encoded zoom atorigin; evidence points to geometry/viewport/scissor/later-stage rather than sampling/discard. This shader is diagnostic ONLY, never deployed; original production shader evidence/exes retained separately. Build override applied only to that scratch invocation, not exported globally.

Vertex diagnostic forces fullviewport quad and encodes actualVSnum/VSden/computedrightendpoint. Entire quad draws correctly. Zoom2uniformoutput[2,1,1,255], half[1,2,1,255], while rightendpoint should be32/8. Thus VS reads zoom correctly but scaled_span endpoint computes1. Viewport/scissor/fetch not cause. renderer-vertex-diagnostic.log, scratch only. preview_smear now owns narrow arithmetic cause/correction.

Narrow scratch shader correction from preview_smear changes only scaled_span ceil calculation to direct signed positive/negative arithmetic, avoiding -scaled(-value). With original frozen renderer Rust, all12gradient matrix cases now match exactly, including Direct/Quantized,zoom1/2/half and hairlinefalse/true. renderer-ceil-correction.log. Frozen source unmodified; correction shader archived ceil-correction.spv with SHA256 in artifact-sha256.txt. Further original tiny/background/smoke executables compiled with only external corrected shader override; archived ceil-exes, validation pending.

Corrected externalshader native validation completed:
- original tiny-background-mask regression PASS1,47.77s;
- entire original ui_background_sampling suite PASS7,58.62s;
- original draw-at-zoom and interface-views exact GPU smokes still fail IDENTICALLY to uncorrected shader,31603/35776pixels and samefirstpixels. These are independent unresolved failures, not claimed fixed.
Evidence ceil-correction-tests.log with fourexitmarkers. Original frozen source still untouched; all tested corrected binaries preserved ceil-exes. Root/preview_smear informed.

Independent golden nominaltexel oracle plus unchanged CPU reference passes all24negative-position/negative-clip/fractionalzoom cases with corrected externalshader, golden-probe.log. preview_smear owns porting exact regression into shared fitted_preview test for successor freeze.

Separate quantized-color investigation, mac_fps_music owner:
- Existing correctedshader native quantized_scale_rounds_down_after_every_blit fails382pixels, first(0,1)GPU[0,8,0,255]CPU[0,0,0,255].
- quantized_translucency_mixes_and_rounds_down fails884pixels, first(0,0)GPU[0,57,181,255]CPU[0,57,189,255]. quantized-isolation.log.
- Independent scratch WGSL blend-probe, no renderer sprite shader/layout/readback decode, compares raw8bit targets against integerhost oracle. Two half alpha blends mismatch384/1024; two k/16shade blends mismatch486/1024. Examples alphaqsrc3/qdst0got17want18; shadeqsrc28/k2got17want16. blend-probe.log. This reproduces fixedblend precision differences independently of renderer/geometry.

Blend probe host model refined to round incoming sourceRGB and alpha to8bit before each blend. This source8 model matches all 2048pixels: alpha0/1024model mismatches, shade0/1024model mismatches. Original integeroracle still384/486mismatches. blend-probe-model.log. Exact cause identified as DX12/Microsoft fixedblend source quantization, independent of sprites, texel fetch, geometry and output decode. mac_fps_music/root own production decision.

Root-requested independent FP16target viability probe with 1536+q encoding, blend-f16-probe.log, currently fails alpha768/1024 and shade786/1024. Example alphaqsrc0/qdst0raw1535wanted1536; shadek0raw1535wanted1536. Owner investigating encoding/tie/conversion specifics; this does not establish FP16unsupported.

FP16floor-corrected encoding removes-.5bias to match observed FP16 store truncation. Independent raw blend nativeprobe now passes all 2048pixels, alpha0/1024mismatches and shade0/1024, blend-f16-floor-probe.log. Viability established on Microsoft CPU DX12; mac_fps_music owns integration/pairedRADV validation.

Expanded FP16alpha viability nativeprobe passes all 10240source/destination pairs across factors1/32,7/32,16/32,31/32,1/255,15/255,17/255,127/255,128/255,254/255, including exactintegerboundaries. Each factor0/1024mismatches, blend-f16-alpha-probe.log. Scratch only, no renderer source changes by this agent.

Actual scratch renderer fallback from mac_fps_music quantized-snap builds against shader c1ae7f49a190658a9f579382035051041f062bdb3d15f401c76f4077bc8b18ab. Native exact adapter identity selects FP16 composition. Original four focused regressions now pass: exhaustive shade, exhaustive alpha, quantized world, draw-at-zoom. quantized-fallback-focused.log. This is a scratch successor, NOT the original frozen source and NOT a deployment. Full source copy preserved evidence/quantized-fallback-source with exact build inputs; its inherited SOURCE-MANIFEST.json describes the ancestor, not modified scratch contents. Full38GPU+text suites pending.

Scratch actual FP16 fallback full native gpu_matches_reference suite now PASS38/38,143.49s, quantized-fallback-full.log. This fixes the original interface-view/draw-at-zoom/exhaustive-shade/alpha failures without changing reference behavior. Text suite running serial afterward. Owner subsequently renamed encoding flags and corrected memory admission accounting in its own scratch source, so this38test result refers to archived precleanup source/shader c1ae7f49, not a later deployment artifact.

Latest cleanup scratch source original quantized_blending regression PASS1, all31fivebit alpha steps,3.42s, quantized-blending-f16.log.

Text suite without game font:8passed2failed after144.21s. Both failures occur after GPU/CPU comparison checks pass, at fixed minimum drawn-pixel counts. Optional LSANS.TTF absent. To diagnose, copied ONLY existing59,468byte read-only hostfont into guest read-only fixture and process-only CIV3_INSTALL. outline_coverage_is_bit_exact then PASS1,41.39s, confirming its missing-font liveness threshold. mac_fps_music then reported a separate no-new-assets constraint not present in this agent's initial task and asked to pause. Root notified; stopped only owned diagnostic PowerShell/test process tree. Second outline_text test interrupted and NOT counted. Evidence quantized-fallback-text-font-fixture.log and font-fixture-pause.log. No shipped/source assets changed. Fixture scope decision pending root.

Root resolved fixture scope: no further game-asset copies. Removed the owned59KB copy. Root authorized a temporary hardlink from existing guest C:\Windows\Fonts\arial.ttf into owned scratch fixture LSANS.TTF, process-only CIV3_INSTALL, then link cleanup. Two density guards rerunning with that Arial fixture. This is system Arial optional-face coverage, NOT Civ III font coverage. Actual Arial identity/hash recorded in arial-fixture-text.log.

Arial fixture density coverage complete: both original outline_coverage_is_bit_exact and outline_text_matches_the_reference PASS with existing guest Arial hardlink. Actual font C:\Windows\Fonts\arial.ttf,1,047,208bytes,SHA25674D696E666F696E93DB685A85C94BFBFCC0796DFCF8A2E2E2E2C908A54E82949. Times39.61s/27.10s. Original game-fontcopy removed before this run. Arial link removed afterward, ARIAL_LINK_REMOVED=True. These replace the earlier absent-optional-font liveness failures as prerequisite-resolved coverage; no claim of actualCiv3font coverage. arial-fixture-text.log.

## Final completion and cleanup

Final scratch native quantized_blending PASS2/2 including new independent successive-blends31,744pair oracle,6.63s. quantized-blending-final.log.
Final scratch native text_at_target PASS3/3,44.46s, no font fixture. text-at-target-f16.log.
Root-requested actual-adapter memory-cap regression retained_fp16_texture_bytes_are_reserved_before_atlas_transfer PASS1/1,24filtered,0.08s, memory-cap-f16.log. That same guest check confirms FONT_FIXTURE_ABSENT=True.

Only own c3r-sol-frozen VM cleanly shut down and destroyed, including ephemeral overlay. Scope c3r-windows-sol.scope now inactive. Other VM/golden untouched. MemoryMax remains10000000000,MemorySwapMax0. vm-cleanup.log and cgroup-final.log.
Own Windows Cargo target cleaned with cargo clean,4083files8.2GiB removed. Evidence, exact executables/SPIR-V and scratch sources retained separately. Original ef snapshot reverified final624files drift[], frozen-verify-final.log. Source audit JSONs for archived initial/final FP16 scratch trees complement inherited ancestor manifests. No game-fontcopy or Arial link remains in guest; guest overlay destroyed afterward.

Final artifact hashes: evidence/artifact-sha256.txt. Final log hashes: evidence/log-sha256.txt. No commits/stash/reset, no Mac access/builds, screenshots or GUI operations.

Subsequent primary-API clarification: this earlier native pass exercised DX12 compatibility. A separate owned Windows Vulkan pass using trusted x86 SwiftShader is documented in windows-vulkan.sol.md, including actual Vulkan adapter identity, exact final46e checks and independently labeled companion GPU/blend/text suites. Do not present the earlier DX12 pass as Vulkan validation.

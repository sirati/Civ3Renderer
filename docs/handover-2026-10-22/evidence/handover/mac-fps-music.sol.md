# Sol continuation, 2026-10-20

Current work in shared tree, no commits:
- `civ3-assets/src/files.rs`: GameFiles clones share lock-free directory listings but own their root vectors. Added clone/root regression.
- `civ3-bridge/src/live.rs`: Compiler caches latest GameFiles keyed by complete ArtFolders. Scenario/search changes build a fresh resolver; repeated publishing reuses installation root resolution and directory listings.
- `tile-render/src/world.rs`, renderer.rs, reference.rs: first_zoomed retains view/category visibility instead of a global per-zoom first pass. Cameras switch to target resolution only for zoomed sprites they can actually draw. Marker visibility retained too. Minimal camera.rs callback signature update. Added hidden/unrelated/except-view regression.
- Existing Opus incremental GPU timing is retained and deploy-ready.

Verification currently building in `/mnt/datasets/c3r-build/perf-agent/target` with nix develop and CARGO_INCREMENTAL=0. No Mac GPU run performed. Root coordinates deployment and any GPU execution. No new performance claims yet.

Baseline evidence retained in original handover and `/mnt/datasets/c3r-build/perf-agent/live.wine.log` plus `live1.sample`: map GPU span roughly 20-25 ms, some full-screen UI spans 70-110 ms. Publisher about 72% of a core, directory syscalls about 15%. X87 probe verified FEX_X87REDUCEDPRECISION=1 reduces 20M-iteration probe 4801 ms to 287 ms; production launcher sets both FEX_X87REDUCEDPRECISION and HADRON_HARDWARE_TSO. Sound threads mostly idle in that sample. Music-loop reproduction and final deployed FPS still unverified.

Text inspection clarification: each RGB fragment entry point repeats subpixels traversal; inside each traversal all channels share crossings. Eliminating two final filter outputs alone does not save the expensive repeated crossings. No shader optimization applied without appropriate fidelity validation.

Verification update:
- `cargo test -p civ3-assets files::tests --lib`: 2 passed, including cache sharing and independent search-root priority.
- `git diff --check` for all files touched by this continuation passed.
- Read-only Mac query found no Civ3Conquests process, and runtime-dualstack's newest wine log remains the baseline file. No new Mac profiling data exists there yet.
- Root authorized short AMD RADV correctness tests; no exclusive timing run requested or performed.
- Release owner will validate Windows-only live.rs in the frozen i686 combined build.

Final implementation/validation status:
- All continuation edits complete and deploy-ready. No shader edits, no lowering text resolution, no visual-fidelity settings changes.
- View-aware zoom starts are deduplicated by zoom/view/except-view/category. A UI glyph no longer switches unrelated world cameras to full target resolution; shared and except-view sprites retain shader visibility rules. Actual relevant text still switches its camera at the same pass.
- `cargo test -p tile-render --lib zoomed_start_tests`: 1 passed.
- `TILE_RENDER_ADAPTER=radv cargo test -p tile-render --test text_at_target --test per_view_layers --test gpu_matches_reference`: all passed, respectively 3 + 2 + 38 tests, including pixel-exact target-resolution text, zoom-art twin replacement, stacked groups, category filters, and cached-frame behavior.
- Commands used `nix develop`, `CARGO_INCREMENTAL=0`, own `CARGO_TARGET_DIR=/mnt/datasets/c3r-build/perf-agent/target`. Source shader compilation completed on Papa. No compile or GPU run on TS Neo.
- Root and release owner notified that edits are final. Windows-only Compiler cache will receive its actual compile validation in combined i686 release gate.
- Performance limitations remain explicit: no clean post-fix TS Neo gameplay recording yet, no claim of 60 FPS or resolved music loops, and no measured delta for these changes. Root owns final combined deploy/session recording. This change removes a concrete cause of unnecessary high-resolution fragment work, but genuine text-heavy views and post-label world rendering may still dominate.
- `gpu+` timing is a completion-increment proxy on overlapping tiled GPU passes, not a measurement of isolated marginal pass cost. Compare total span and paired real scenes before assigning dominance solely from one increment.

Frozen-release follow-up:
- Root froze source `ef962736b6840284c6dc052d9df989d3539d4b16a83a6a28149ac222e0013754`; no continuation source edits after freeze.
- Coordinated with release_deploy_sol to analyze its single diagnostic game launch, with profile and per-pass timing requested. No separate Mac game launch planned.
- Preview owner independently reran all three fitted_preview source-shader RADV tests after the view-aware change, all passed. Evidence `/mnt/datasets/c3r-build/preview-smear-sol-integrated-tests.log`.
- Read-only analysis tooling saved 408 original profile windows to `perf-agent/baseline-profiles.csv` and compact lines to `baseline-summary.txt`. `analyze_profiles.py` takes source log and destination CSV for final-session parsing. Timing means are reconstructed as mean-per-round * rounds / sample-count; raw mean-per-round is not a valid mean GPU duration when timing sample counts differ.
- Baseline nuance: 05:48:46Z title/menu profile reports 19.5 draw attempts/s despite GPU p50 1.95ms, acquire0.05ms, CPU draw0.79ms, interval42.75ms. Low FPS in static menu was not GPU saturation in that window. Conversely 06:06:05Z reports GPU73.6ms, acquire77.95ms, CPU draw82.8ms. Separate static-menu pacing from scene-heavy GPU costs in final results.

Native DX12 color investigation, independent of Mac and geometry release:
- Preview's direct-at-zoom geometry correction validated on warm RADV source shader. Final serialized complete log `/mnt/datasets/c3r-build/perf-agent/geometry-final-radv.log` has fitted_preview4, gpu_matches_reference38, text_at_target3, text_matches_reference10, ui_background_sampling7, all62 passed.
- Initial parallel test run had an unexplained all-zero768pixel readback in the nonwrapping unitzoom test; precise failure retained in `perf-agent/geometry-initial-anomaly.log`. Two complete subsequent serialized gpu-reference runs passed. Cause not established.
- Shader source lib.rs SHA256 c6430da0ac9ba27050cf0c42112b1130573a5f70a5c271e59506db7805d2b8fd; fitted_preview test78299499d5b54ddacc54c64324abeed6d2d84ce9e84bb87bbe0654b6ff006e42. Built SPV1e8f22da55bf9ff6855ca9da0724da95388b7456e94dd22a5bb0be8ca73fa6ef, `/mnt/datasets/c3r-build/perf-agent/target/spirv-builder/spirv-unknown-vulkan1.2/release/build/tile-render-shaders/2c4b24212c33b8db/out/tile_render_shaders.spv`.
- Separate quantized scale/alpha native failures are real fidelity defects. Independent raw WGSL full-screen blend probe bypasses sprite shader, atlas, transforms, and final decode. Native DX12 Microsoft Basic Render Driver two alpha blends fail384/1024 raw pixels; two k/16 shade blends fail486/1024. A host model that rounds fragment RGB AND alpha to8bit before fixed blending predicts every pixel exactly,0/2048 errors. Logs `/mnt/datasets/c3r-build/windows-vm-sol/evidence/blend-probe.log` and `blend-probe-model.log`; scratch probe source `windows-vm-sol/keyboard-diag/src/bin/blend-probe.rs`.
- Current fractional-source bias formula is incompatible with that preblend8bit quantization. No production correction yet. Root proposed FP16 target encoding1536+q where stored ULP1 preserves per-draw rounding. Raw paired native/RADV probe in progress before considering any renderer change.

FP16 workaround established in isolated scratch, main still untouched:
- Root's1536+q idea works on measured native DX12 when the old -0.5 offset is removed. FP16 target conversion truncates, so a small positive epsilon preserves the exact floor. Raw halfalpha/shade16 tests pass0/2048 on both APIs.
- Wider rawfactor test proves this encoding must be selected narrowly. Native DX12 passes all10240pixels for factors1/32,7/32,16/32,31/32 and1/255,15/255,17/255,127/255,128/255,254/255 with two successive blends. RADV FP16 source behavior differs and most widercases fail. Keep proven RGBA8UNORM on RADV/Metal.
- Actual renderer fallback scratch `/mnt/datasets/c3r-build/perf-agent/quantized-snap` selects only exact observed tuple Dx12/Cpu/vendor0x1414/device0x8c/nameMicrosoftBasicRenderDriver. Changes are generic tile-render code only, no game code, test switch, locks, CPUreference change, or asset change.
- Float flag uses existing GPUuniformwordpositions, now named quantized_float. FrameParams176bytes/OutputParams32bytes unchanged. Quantized canvas/source/base usesRGBA16Float only for selectedadapter and nonzeroquantizedbits; directcolour internalpictures remainRGBA8. Shaded/alpha, invert, subpixel, outline/keyed text all encode appropriately; blits preserve rawvalues, output/copies decode1536+q.
- Actual native originalfour focused quantizedregressions PASS. Full native gpu_matches_reference PASS38/38 in143.49s; native text still running. RADV scratch original38 pass, final additionaltext/atlas suites running.
- Native proofsource/shader/exes preserved by targeting owner under `/mnt/datasets/c3r-build/windows-vm-sol/evidence/quantized-fallback-source`. Source-built matchingSPV SHA256c1ae7f49a190658a9f579382035051041f062bdb3d15f401c76f4077bc8b18ab at `perf-agent/quantized-fallback.spv`.
- Reviewer tile_popup_render_shadow found and corrected actualmemory admission omission: common resident accounting now includes actual4/8byte canvas/source/base textures and directatlasadmission uses it too. Added focused budgetguard verifies a retained4x5 FP16 image adds160bytes and rejects a previouslyjustfitting transfer. Guard passesRADV. Review found no other encoder/copy/decode omissions.
- Current five-file candidate diff `/mnt/datasets/c3r-build/perf-agent/quantized-fallback.patch`, manifests `quantized-source.sha256`. Main integration awaits final native text/regression results and root review.

Final scratch verification additions:
- Native original `gpu_matches_reference`38/38 passed; original all31alpha-step single-blend test passed31744source/destination pairs.
- Native text suite without optionalfont was8/10; both failures were density/liveness guards after GPU/CPU comparison, not pixel mismatches. Root authorized only a guest-system Arial hardlink fixture for complete originalloops and unchangedguards. Label this Arialcoverage, not Civ III fontcoverage. Root directed removal of the brieflycopied59KB gamefont; do not ship assets.
- Added permanent stronger two-overlay31744pair integeroracle regression in scratch `quantized_blending.rs`; old+new tests bothpassRADV. No CPUreference/sharedmath dependency in newexpectedvalues. Native run pending.
- Latest scratch RADV atlas12 + fittedpreview4 + alpha1 + text_at_target3 + text_matches10 + background7 allpass. Budgetguard1pass. Earlier scratch GPUreference38pass. Logs `quantized-scratch-final-radv.log`, `quantized-two-blends-radv.log`, `quantized-budget-radv.log`.
- Candidate now sixfiles, includes explicit GPUencodingwordnames and unchangeduniformbytepositions. Matching SPV remains byteidenticalc1ae7f49... after fieldrename/importcleanup. `git apply --check perf-agent/quantized-fallback.patch` passes against sharedmain.

FINAL integrated state after root review:
- Root reviewed/authorized exactsixfilepatch and it is integrated into sharedmain. No commits/stash/reset. CPUreference unchanged SHA256f700ede4527bc8a423cc156a99540bf6384fa2d1926941877621e320bad803af.
- Integratedsourcebuilt RADV passed77checks,76integration +1budgetunit. Logs `perf-agent/quantized-integrated-final-radv.log` and `quantized-integrated-budget-radv.log`. Canonical main shader explicitlyrebuilt afterintegration, bothalpha tests passedagain, `quantized-canonical-source-radv.log`.
- CanonicalSPV SHA256c1ae7f49a190658a9f579382035051041f062bdb3d15f401c76f4077bc8b18ab matches nativeproofexactly. Stableartifact `/mnt/datasets/c3r-build/perf-agent/quantized-fallback.spv`. Fullsixfile sourcehashmanifest `quantized-source.sha256`; reviewedpatch `quantized-fallback.patch`.
- Native final54cases passed: GPUreference38, text10, text_at_target3, alpha2, budget1. Evidence/source/exes retained in windows-vm-sol/evidence; finalhandover windows-vm.sol.md.
- Fontlimitation explicit:8textcases syntheticfaces,2 unchanged densityguardloops completed with guestArial hardlinknamedLSANS.TTF and process-onlyCIV3_INSTALL. Arial SHA25674d696e666f696e93db685a85c94bfbfcc0796dfcf8a2e2e2e2c908a54e82949,1047208bytes. This is Arialcoverage, not CivIII fontcoverage. Brieflycopied59KB gamefont and temporaryAriallink bothremoved. No shippedassets or originaltexttests changed.
- Targeting cleaned only its ownedVM/overlay/Cargotarget. This agent cleans only ownperf-agent target through cargo clean; stableSPV/logs/sourceevidence retained.
- Nativecolor resolved for exactobserved adapter via defaultruntime selection. No generalDX12/Metalcapability claim. MacFPS/music aftercombineddeployment unmeasured by thisagent; no60FPS/fixedmusicloop claim.

# Army units Sol status

Implementation and focused tests complete. Original Opus handover is unchanged.

- Removed growth-added UnitArmy named palettes. Army flag colours (64..127) are baked into separate image palettes; only 0..63 use the stable base ntp row. Variant names include the real owner, civ-zero table and member count. Hidden nationality changes the body row but preserves real-owner flag colours.
- Kept the inherited tables-versus-pixels regression and extended it to sprite IDs, animation sets and cell values. The same real-data test checks exact palette entries, whole compilation and growth, CPU scene output and GPU growth rendering.
- Preserved Opus army HP/strength/movement aggregation and complete army-member collection.
- Fixed scientific leaders losing leader_kind between native snapshot and renderer. The generated native offset is Unit_Body+472 (0x1D8). Scientific leaders resolve <type>_SCI; military leaders resolve era art. Trace lines append leader_kind and old traces default to zero.
- Real-data regression resolves military leaders and armies in all four eras, scientific leader art, and transport art/HUD with cargo. Added native record read coverage for leader_kind.
- Shared edits: preview.rs art_ready only; live.rs unit conversion only. scenario_crash was informed. unit_views.rs test constructor includes the new field. No commits or deployment.

Evidence logs: /mnt/datasets/c3r-build/army-sol-tests.log, army-sol-sys-tests.log, army-sol-layout.log. Source hashes: /mnt/datasets/c3r-build/army-sol-sources.sha256.

Known pre-existing limits retained: movement uses the standard 3 points per tile and type movement rather than native civ movement bonuses. Parent owns aggregate Windows/Wine checks and deployment.

## Final validation

Implementation and focused validation are complete. All checks passed on Papa with real Civ III data present; no art or GPU skip occurred.

```
CARGO_TARGET_DIR=/mnt/datasets/c3r-build/army-sol CARGO_INCREMENTAL=0 TILE_RENDER_ADAPTER=radv TILE_RENDER_PRECOMPILED_SPV=/mnt/datasets/c3r-build/release-1019/artifacts/7055148cf5c47a28d6ccb8cf3fa1e6b484fa582cbe7069877227b0c5999c35bf/shader.spv nix develop -c cargo test -p civ3-bridge --lib units::tests -- --nocapture
```

11 passed, 0 failed, 0 ignored; 391 filtered out; 89.36 seconds. Includes exact army palettes in whole and grown atlases, tables/pixels sprite/set/cell agreement, GPU growth rendering, HUD aggregation, four military leader/army eras, scientific leader and transport/cargo.

```
CARGO_TARGET_DIR=/mnt/datasets/c3r-build/army-sol-sys CARGO_INCREMENTAL=0 nix develop -c cargo test -p civ3-sys --lib world::tests::unit_record_keeps_animation_flags_positions_and_army_members -- --nocapture
```

1 passed, 0 failed; 117 filtered out. Native record test includes leader_kind=2.

```
TMPDIR=/mnt/datasets/c3r-build nix develop -c tools/gen-civ3-layout
```

Generated 465 base constants and 93 C3X constants. Only new generated offset: UNIT_BODY_LEADER_KIND=472. C3X generated file unchanged.

Owned-file git diff --check passed. sha256sum -c /mnt/datasets/c3r-build/army-sol-sources.sha256 passed for all ten files after tests.

- units.rs SHA256:  97e2df3b86e3f757b24f0eb2ce9d0c821f01bb4ee2c284273790789e7b0dfcda
- civ3-sys/world.rs SHA256:  a0c49c2c0611f251634ebf1478a2f103b508173199b32fbb7aa72c86f0841dcb
- SPIR-V SHA256:  d19e66b8b204f1d9ee0d8c383e81f6180d5e14fa459f76551d168769d201980b, independently hashed; parent verified source provenance.

The initial source-shader build was stopped before tests and restarted with parent-verified SPIR-V. No Mac builds or GUI/screenshot work occurred. Parent owns aggregate Windows/Wine checks and deployment. Logs and hashes remain in scratch; own Cargo targets were cleaned after validation with cargo clean.

# Successor task: procedural solid fills, protocol 48

Completed parent-approved typed SolidRect design. menu Element::Fill immediately emits an ordered scene FreeSprite with solid dimensions and RGBA. It requests no art and generates no CPU fill pixels. ScreenArt no longer generates fill images. Scene wire48 appends the optional solid record after category. Existing glyph/ordinary sprite records retain their behavior.

Renderer FreeSprite has an explicit 16-byte FreePrimitive descriptor. Shader vertex/fragment paths branch before sprite-set/atlas/font lookup, emit rectangle geometry and RGBA, and reuse existing exact alpha/quantized blending. Solids remain in the original free command stream between text and sprites. Clipping, fractional zoom and KEEP_SIZE behavior for world-positioned UI plates are preserved. set_world can bind minimal empty GPU resources before any atlas is installed, without CPU pixel generation.

The existing CPU reference oracle understands solids for numeric tests. No new production rendering fallback was introduced. Viewer availability and bounds handling accept solids independently of future sprite/font/palette metadata. Collaborated with tile_popup_render_shadow on those sections, preview_smear on renderer triple-control changes and text_fit_atomic_ui on bridge fixtures/atomicity tests.

## Focused results

All passed on Papa. GPU tests report AMD RADV RAPHAEL_MENDOCINO, Mesa 26.2.4, Vulkan.

```
CARGO_TARGET_DIR=/mnt/datasets/c3r-build/army-solid-sol CARGO_INCREMENTAL=0 TILE_RENDER_ADAPTER=radv nix develop -c cargo test -p tile-render --test procedural_solids -- --nocapture
```

5 passed, 0 failed in 0.44 s. Independent numeric goldens cover atlas-free invalid set/palette metadata; direct alpha and clipping at 1x, 2x, 1/2x, 3/2x; every RGB555 source/destination/alpha boundary with repeated translucent solids; ordinary sprite order; fixed-size world plates under downscale; atlas-free outline text between solids. Reference-oracle output is also checked against the independent goldens.

```
CARGO_TARGET_DIR=/mnt/datasets/c3r-build/army-solid-host-sol CARGO_INCREMENTAL=0 TILE_RENDER_ADAPTER=radv TILE_RENDER_PRECOMPILED_SPV=/mnt/datasets/c3r-build/army-solid-sol/spirv-builder/spirv-unknown-vulkan1.2/release/build/tile-render-shaders/2c4b24212c33b8db/out/tile_render_shaders.spv nix develop -c cargo test -p menu --lib screen::tests -- --nocapture
```

11 passed, 0 failed, 20 filtered out. Includes no-art immediate fill emission and unchanged mixed screen order.

```
CARGO_TARGET_DIR=/mnt/datasets/c3r-build/army-solid-metadata-sol CARGO_INCREMENTAL=0 nix develop -c cargo test -p scene -p tile-render-shared --lib
```

Scene 14 and shared 17 passed with 0 failures. Scene roundtrip includes a solid record. GPU descriptor alignment tests pass.

Fresh source SPIR-V artifact, copied outside target directories: /mnt/datasets/c3r-build/army-solid-artifacts/shader.spv
SHA256: 67f3cefc9e905d7404baf63a3d0a3b54ca775bddc61612f52aa57c9b7de1ceb0.
Shader main source: 309b8118ef0a44c589bd812417d0fc926c9ef81a4e50652f8278cf4d5b1f6da2.
Shared main source: a3827f074c6dea98d7d47fbf6f211126a918f3416755f3b23a9ceadefa1abee8.

Exact shader dependency hashes: /mnt/datasets/c3r-build/army-solid-artifacts/shader-sources.sha256.
Implementation/test hashes: /mnt/datasets/c3r-build/army-solid-artifacts/implementation-sources.sha256.
Logs: /mnt/datasets/c3r-build/army-solid-sol-{gpu,menu,metadata,build}.log.
Owned-file diff whitespace check passed. Parent owns all 5 successor builds, aggregate Windows/Wine/WARP checks and deployment; no Mac/GUI/screenshot work occurred.

Own solid-task Cargo targets were cleaned after validation with cargo clean. The durable shader artifact/manifests and logs remain. To rerun commands, use TILE_RENDER_PRECOMPILED_SPV=/mnt/datasets/c3r-build/army-solid-artifacts/shader.spv after checking its source manifest, or rebuild shaders from source. Parent owns any further bridge/aggregate checks.

Follow-up bridge fixture correction: preview::optional_preview_art_can_remain_unavailable_while_the_menu_is_complete now asserts procedural Fill labels require no art, then uses an actually missing Sprite label for the unavailable-art assertion. Complete-menu assertion retained. No production behavior changed. text_fit_atomic_ui owns the isolated rerun after the aggregate reported 403 passed, 1 stale-fixture failure, 1 ignored.

## Movement successor source completion

Live HUD movement now uses native max movement sampled on the game thread after animation update. Pure publisher copying checks native address, id, type, owner and army membership/count. Memory-only sys readers never invoke native code. A moving last-step unit stays yellow until animation ends; zero sampled maximum remains valid. Legacy/cold previews retain the explicit 3-point fallback.

Evidence: `/mnt/datasets/c3r-build/army-acceptance-sol-evidence/` contains the current six-file source manifest, native movement disassembly and passing logs. Final unit_views suite: 13; units suite: 14, including actual 3840x2160 independent-camera far-army GPU regression with bounded changes; sys native-record regression: 1; i686 bridge library check passed. This is functional coverage, not 240 fps proof.

Those GPU tests used retained historical 67f3 shader. Performance work later changed shader main; rerun affected GPU acceptance when root supplies a stable successor shader. Frozen31b does not contain this movement change and has no successor native runtime movement evidence yet. Preserve its historical gap separately from this source-fixed, targeted-tested status.

## F006 city work targets, source in progress

Source now adds corrected-city-camera Button targets for actual worked tiles and center autoassignment, gated by city background form, its clip, modal canvas, ownership/debug bit and map band. IDs use city canvas and native work-area index. No yield image dependence. Existing city Anchor inverse maps clicks back to native coordinates. New city_screen tests cover removal/center actions, no camera coupling, hidden/modal/foreign/outside tiles, seam/edge handling and authoritative unworked eligibility masking.

Full unworked native eligibility recovered from C3X CSV/source and native4BB560. New sys city_work module checks City_controls_tile and draw_yields signatures. city_screen::native samples controls_tile(index,true) only on game thread after original native city yield paint, never from publisher or a periodic timer. ui_trace retains hook and makes pending eligibility visible at matching Present. Snapshot identity checks map pointer/dimensions/seed, player and city pointer/id/coords/owner. Mouse-down invalidates both copies and advances epoch before native dispatch; old racing captures reject. Existing workers may always be removed, while unworked tiles require authoritative eligibility and another city's workers remain excluded.

Evidence `/mnt/datasets/c3r-build/city-worked-alt-sol-evidence/`: native eligibility disassembly and notes. Tests not yet passing/complete: concurrent renderer smoke/schema changes first blocked compilation; old retained shader now fails new text_dual_fs entry validation. Root requested stable successor SPV. Windows check reached new native code without errors but failed elsewhere (blend literal additions and trace image parser output).

Files touched by F006: new sys city_work.rs, sys lib.rs module declaration, city_screen.rs helper/native sampler/tests, live.rs targets and input invalidation, ui_trace.rs small hook retention and present handoff. No snap.rs edits; split/scale regression requested from snap owner tile_popup_menus. No GUI operations performed.

F006 validation update: final CPU city_screen suite passes 8/8, adding strong map/city/player/owner/position identity and click epoch rejection. Actual Alt-click mapping through Layout/scaling passes its new city-worked-tile regression across 4 split modes, pixel scales1/2/3/6 and frame scales0.5/1/2 at3840x2160, preserving the native tile centre after city inverse. Files `crates/tile-viewer/src/snap/city_work.rs` and test-only module declaration in snap.rs added after snap owner completed. No snap production changes. Tests are CPU-only, compiled with staging current-dual13f4, not a GPU shipping-shader acceptance. Pending native runtime QA remains explicit.

## UI atomicity independent diagnostic follow-up

Root assigned historical31b guardedSS Preferences frame analysis. Report and logs: `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/diagnosis.md`.041 is host-status banner over unchanged Title, not partial Preferences;042 full command tree with pending artwork does not prove partial native Present. Initial18.65s publisher ui-items+labels hotspot unresolved; no native lifecycle guess/asset-wide gate/timer added. Positive-source/mask lookup memoization removes concrete repeated sync IO, with1lookup regression+9readiness tests passed. Finer existing profiler marks added.

045 has a separate confirmed tab/body mismatch from viewer Select::Pick prediction up to1s. Real Prefs-to-Feedback regression failed before fix. Final source disables Preferences Pick prediction across the draft panel while retaining hover/pressed art; generic panels suppress only choices referenced by show_if/enable_if, with dependencies precomputed at parse. Added tab, derived pixel-scale/readout, and independent generic choice coverage. After-fix tests are pending compatible shader entry (subpixel_dual_fs) and FPS broker quiet release. No after-pass claim. Windows check25.63s passed before final Preferences-wide policy/precomputed dependency edits; final small edits still needcheck. Last source hashes in ui-atomicity evidence. No guest GUI operations by this agent.

## Allocator fallback independent review

Reviewed root's completed alloc.rs fail-closed patch, sourceSHA31a6590d4ecde7c239412e53b5f2d2dcb126cb713a944c972a400add89d0b785. Source verdict passes: only HeapCreate private handles publish; failure returns null orAcquire-loaded private racingwinner. raw_alloc_with exits before guard/FFI onnull fornormal/overaligned/zeroed allocation; normalrealloc hasnullguard; failedoveralignedrealloc retainsoldblock; rawfree loads publishedhandle andnevercreates replacementheap. GetProcessHeap is test-only. Racewinner test owns genuine emptyprivateheap, and releases it only afterlastuse. No allocator strategy replacement and no allocator changes by army_units.

Queue afterFPSbroker release: build inproc i686 Windows libtest executable (`cargo test -p inproc --lib --target i686-pc-windows-gnu --no-run`, scratch targetunder/mnt/datasets/c3r-build), run focused creation_failure_tests under isolated Papa Wine, plus existing private_heap_is_not_the_process_heap_and_honours_alignment and concurrent_allocation_and_cross_thread_frees_stay_consistent. Avoid ignored latencybenchmark orwatchdog3s sleep unless separately needed. Tests assert null/handle/busy/failure-race semantics; current seams do not instrument Heap* callcounts orforcevalid-pointerrealloc failure. No Windows/Wine testpasses claimed while quiet.

## Frozen866 CPU checkpoint, exact executable evidence

Source archive86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e, canonical dualf9968edd2867ca9857731cd0b1c09d2172cb019e5523065b942507b249d7dc75 andfallback7ff6d943a54e7255df2cd160e7239aee067f89470156e5a50b3c1e5079857a5d. Reused release-owned no-run executables directly; verified executable hashes before andaftereachrun. CPU41/41passed: tab1,pixelderived1,city8,lookup1,readiness9,transaction6,menu-panel14,city-split1. Full paths/hashes/commands/durations in `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/cpu-results.json`. This closes earlier queued tab/pixel aftertests only for866 source. No GPU claims from CPUcity/splitrouting tests.

Root then identified dependentToggle prediction remaining in866. Separate successor source work is ongoing, with dependent checkbox+independent toggle regressions; frozen866 source/artifacts remain unchanged and their CPUproof is not successor acceptance. No sharedreleaseCargo targets used for local failing-before reproduction.

## Frozen866 brokered RADV and Windows tests completed

Exact frozen866 ifc23 host executableSHA345e45152118462bd50d44d70ae77b8126eae19968587ab9cf54f20a3eaac842, samecanonicalf996/7ff6dual/fallback embeddedonce perrelease manifest. Full units suite15passed,1ignored,0failed in65.88s, including actual3840x2160independentfar-army HUD/selection test logging AMD RADV RAPHAEL_MENDOCINO/Vulkan/Mesa26.2.4. Explicitignored overlapping-smoke layerprefix/body/cursor/banner/independent-view test then passed1/1 in6.44s with CIV3_INSTALL pointing to read-onlydatasets gameinstall. First explicitlaunch omittedCIV3_INSTALL andfailed beforeGPU setup; both first/final logs retained. No FPS/240Hz claim and no cityGPU raster claim.

Frozen866 inprocPE SHAa4e6fdf9ceb25225e8d11ef7632083409b814a1b9255e9270ae6164af3a3a542 passed creationfailure2, alignment/realloc1, cross-threadfree1 under own isolatedPapa Wineprefix. Prefixinit15.84s; actual memorytests0.00/0.00/0.12s. Frozen866 bridgePE SHAc5aa9a5fb4c647b5781306fa7e582e2c06a9bce668e89caaab0efda075eb4443 passed Windows tab1andpixelderived1. Exact commands/hashes/results in army-checkpoint-86663ea8/{allocator-results,windows-ui-results,cpu-results}.json; GPUlogs units-gpu.log andsmoke-gpu-final.log. Wineprefixserver killed, no compositorstarted, brokerlease explicitlyreleased.

## Successor dependentToggle correction, separate from866

Root identified remaining dependentCheckbox prediction. Real Prefs-to-Feedback test failed beforefix: `Scale menus cannot appear enabled while its committed slider is disabled`. Generic menu test independently failedbefore because dependent checkbox still advertisedToggle. Minimalfix extends bothPrefstransactionpolicy andgeneric content-dependent policy fromPick toPick|Toggle. Both hover/pressed art and independentgenericToggle prediction remain. Ownscratch afterfix tests fullPrefs54/54andmenuPanel15/15passed. These are currentlocalsource proofs againstcanonical866shader, not immutable866acceptance. Files changedforToggle only prefs.rs andmenu/panel.rs. Collectwithshaderowner'stest-onlyoracle correction into nextsourcefreeze, then finalfrozenfocused reruns are needed. No further builds/GPU workrunning.

Source-only18.65s audit complete: ui-atomicity-sol-evidence/stall-source-audit.md. No new production edits/builds/GPU/VM. Concrete remaining cold IO is uniqueimage/mask resolution with percomponent exists beforedirectorycache, plus first-style fontcache enumeration. For this specificfirstPrefs sequence normalandboldstylecache are alreadywarm fromTitle+unconditionalcapturehint, so changedfontheights do not implynewscan. Optionalpreload uploadedstatus still crossesWine even withhistoricalPRELOADoff; Unixbodyonlyatomic and no18s attribution. Existing866profile submarks plusone uploaded-query boundary andboundeduncachedlookup summary are the nextdiagnostic, not anasset/readinessgate. Exact historicalhotspot remains unresolved.

Root authorized source-only optional-preload query short-circuit. live.rs now queries presenter.preload_ready only when planning.is_some() or !preload.finished(), and the existing profile records `preload: upload acknowledgment` around actualquery. Finished/off+no planner returnsfalse withoutWineboundary. Existingplanning alwayskeeps query, including completionlaterthisround; newlyspawned plan froma finishedstate maydefer firstoptionalchunk oneiteration. Reservation and mandatoryart paths unchanged, and everyadmittedchunk still requires actualGPU-upload acknowledgment with alloriginal art-thread/pending-growth/schedule gates. No builds/tests/VM/GPU duringAppquiet; successorcompiletestpending. Hashes successor-ui-source-hashes.sha256 retained inui-atomicity evidence. No18s-cause claim.

## Capture AI keep-visible and combat arrival successor

Capture AI semantic focus previously unconditionally recentered Follow::Driver/Focus targets on every native serial. New FreeView::expose_tile computes minimum movement from full diamond extrema and raster-visible span, including fractional zoom/world scale/downscale, clamped camera position and nearest wrap copy. Captured visible tiles stop old glide momentum. Relevant Focus B uses the semantic request as well. Uncaptured normal focus retains its centering policy. No scenario/world bounds policy was changed.

Failing-before regression shows a fully visible AI request(3064,2032) moving center(3000,2000) to the request. Baseline was compiled from isolated source with only policy disabled, not a compile-error claim. Current layout suite138/138 passes, including A/B, DriverA, horizontal/vertical, all four edges/corners over11zooms*3scales*3downscales, clamped-map/wrap, smooth stop, offscreen-required movement and uncaptured behavior.

Combat arrival is real native simulation backpressure. Separate Scene.camera_arrival carries serial and complete visible attacker/defender tile bounds; protocol50 Update25, Input19 CameraArrived and20 CameraUnavailable. Focus-only requests now participate in live.rs publication key before unchanged early-out, avoiding a deadlock where a paused game could never publish its camera request.

New sys camera_arrival uses serial atomics and existing Wake park/unpark, no new production Rust mutex, spin or sleep. Scope guard is !Send/!Sync. Matching request is armed before immutable native snapshot publication. First accepted classifier query per native combat pair waits before returning to the animation caller; repeated queries within the scope do not wait again. Speed owner owns sole C3X-preserving Fighter_fight outer hook and both scopes. Existing Unit_move/bombard wrappers scope ZoC/interception as well. Bombard caller audit found fire animation before later classifier, so existing visible bombard path now gates before original. Hidden shooters/endpoints remain excluded by checked visibility/detection and preferences. Combat acceleration arms only after arrival/fail-open.

Watchdog is2s only for failure recovery. Success/failure/timeout/inactive/superseded outcomes CAS-retire their own pending serial, never a later combat. Matching native arrival metadata clears on finish, and map replacement clears area while focus serial stays monotonic. Disconnect and explicit device failure wake/release; impossible/no eligible view returnsUnavailable immediately.

Viewer computes receipts only from accepted Completion::Presented immutable pending.mapping plus final pending.frame, before activating newer mutable input/layout. A is required when Driver/Focus, else Focus B, else first Focus inset. B cannot acknowledge clipped required A; smaller auxiliary views do not force false failure of a complete primary view. Bounds include final viewport and target scissor. Preparation, failed acquisition, obsolete config and replay cannot acknowledge arrival. Actual GPU semantics are queue.present enqueued, not scanout/GPU completion; queue order puts that accepted camera frame before subsequently generated combat frames. No GPU fence or readback wait was added.

Evidence `/mnt/datasets/c3r-build/camera-arrival-sol/`: design.md, source-hashes.sha256, host-viewer-test-exe.sha256, protocol-tests.log1pass, sys-tests-final.log26passes including real other-thread Wake release/stale/future/duplicate/superseded receipts and retirement, layout-tests-final.log138passes, submission-tests-final.log5passes including immutable scene serial ownership, viewer-tests-final.log3focused passes. Current i686 civ3-bridge library check passed; exact duration/log in windows-check-final.log. CPU-only tests use retained3be SPV for compilation; no GPU shipping shader acceptance and no successor native runtime battle proof yet. VM owner has exact serial/frame markers and pending successor QA requirements.

## C3X missing popup resolution ownership

Installed3be first C3X click reached native callback immediately. Modal stayed active331.221..547.929s, TEXT_TRACE contained C3X title,17interface sprites arrived348.470s, but all53distinct frames showed opaque Preferences. Escape dismissed the invisible active modal. This disproved missing native header routing as cause. Modal owner confirmed frame_without_world used scene ID order, drawing popup257 before Preferences258. They own no-world/world popup ordering and protocol50 modal stack correction. No prefs.rs production changes were made by army_units for C3X. Exact runtime evidence vm-qa3be-sol/c3x-first-click-evidence; static audit c3x-preferences-sol-evidence/static-audit.md.

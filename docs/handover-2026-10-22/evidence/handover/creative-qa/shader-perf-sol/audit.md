# Shader and GPU pass audit

The native 3840 by 2160 workload is GPU limited on Papa's AMD 1002:13c0 iGPU. The initial offscreen dynamic-pan run rendered 240 complete frames with no reuse at 80.252 FPS. GPU frame median was 12.385 ms; CPU build/encode median was 0.473 ms. Actual surface presentation was 70.88 FPS with GPU median 12.109 ms. The measured run reached 2200 MHz, 100 percent GPU busy, 3000 MHz memory and 52 degrees C. The target budget is 4.167 ms. This evidence does not establish 240 FPS.

The benchmark uses a retained real hd-ww2 trace with terrain culling expanded for the 4K viewport, 76 layers, 1,095 free sprites, and all recorded map overlays and UI retained. The benchmark owner is adding vector-text and actual-current-runtime coverage. Historical trace text can choose raster Low twins at native zoom.

A diagnostic run split the normal merged map steps to obtain individual timestamps. Its median GPU frame was 12.310 ms, close to the ordinary 12.109 ms sample. Split timings are diagnostic rather than a production configuration:

| GPU stage | Median milliseconds |
| --- | ---: |
| Terrain, 3 layers | 3.393 |
| Rivers under | 0.062 |
| Objects, roads and borders, 49 layers | 2.654 |
| Rivers over and map free sprites | 0.305 |
| Fog, 2 layers | 2.174 |
| Units, 20 layers | 0.203 |
| UI free sprites | 1.201 |
| Final output decode | 1.784 |
| Clear | 0.438 |

Source inspection confirmed existing optimizations: ordinary map steps share one render pass, free runs for other views are skipped, terrain uses indexed vertices, and sprite margins and suitable octagon corners are trimmed. The inventory estimates 6.95 screen equivalents of sprite rectangles but 2.18 screen equivalents of nonblank pixels. Terrain and fog each leave about half their rectangles blank, so transparent fragment work matters.

Candidate work is restricted to exact shader arithmetic. It does not change resolution, rendering features, layers, controls, palette content, alpha precision, CPU rendering, or frame counting.

## Final-output decode experiment

The existing RGB555 output path converts each fetched normalized channel into an integer, subtracts the stored bias, replicates five bits to eight and converts to a normalized float. The experiment uses float arithmetic over the valid biased UNORM8 range while preserving the generic path for every higher source byte, other colour depths and integer-float composition.

For source byte s in 16 through 47, q = s minus 16 is an integer from 0 through 31. Its expanded eight-bit result is 8q plus floor(q/4). A UNORM8 float fetch introduces only a small representation error. Adding 0.125 before the division-by-four floor keeps that operation away from a boundary, and the target's UNORM8 write rounds the final result back to the exact eight-bit value. Source bytes below 16 clamp to q zero, preserving the existing decoder. Sources above 47 select the existing generic implementation.

A CPU float32 emulation matches all 32 ordinary channel levels. The dedicated direct-GPU oracle covers all 256 input bytes, depths 0, 4, 5, 6, 7 and 8, integer-float fallback, critical alpha bytes, fractional output zooms, and shifted source and destination origins. Existing ordered RGB555 blend tests remain necessary. Performance and GPU correctness are pending at this document's first write.

All normal reachable RGB555 UNORM8 intermediate values are biased bytes 16 through 47: clears quantize eight-bit colours then add 16; opaque output does the same; alpha composition has a verified integer result between its input five-bit values; scale blending produces a nonnegative floor of the previous five-bit value; inversion maps q to 31 minus q; source blits preserve stored bytes. Transparent draws skip RGB modification, retaining that invariant. Cached output keeps the canvas's own colour-depth descriptor. The experiment nevertheless retains generic decoding above the valid range, rather than relying on this invariant alone.

## Further candidates

The indexed texel path currently converts packed palette bytes to normalized floats, then the RGB555 fragment path converts them back to bytes for quantization. A narrow opaque indexed path can quantize the palette bytes directly, preserving transparent skips and sending partial alpha, tint, background filtering, other colour depths and integer-float composition through the generic path. This targets the measured terrain stage after the output experiment.

On the CPU, FreePipelines::of allocates a Vec per free run, view and frame even for a single ordinary pipeline. The worker CPU owner was informed. It is not the primary bottleneck in this measured workload.

The renderer also captures an overlay-free full-canvas copy when frame reuse is explicitly disabled. That copy can be omitted when reuse is disabled, but this benchmark has no trailing FPS/cursor overlay camera, so that issue does not explain its current timings. Default moving views still need a correct retained background for a subsequent static frame.

## Output experiment result

Rejected. The direct GPU byte oracle passed 85,888 pixels in 0.79 seconds, including all 256 possible input channel bytes. Three matched corrected production-layout scale2-to-4K runs measured output medians 1.697, 1.671 and 1.680 ms with the canonical decoder, against 1.768, 1.740 and 1.758 ms with the candidate. The roughly 0.07 ms output regression is consistent across these runs. Total frame rate remained about 25.5 FPS. Native 4K results were also within run variation. The experimental output block was removed from the shared source. The candidate artifact, immutable source and byte-oracle evidence remain here.

The corrected production scale2-to-4K workload has 690 active outline glyphs, 1,785 free sprites and six cameras. Its GPU median is about 37.3 ms. Most time is in zoomed text stages, about 21.3, 6.1 and 3.5 ms. The historical cheap-upscale numbers are a lower bound and do not represent this actual viewer layout. A separate owner is optimizing exact GPU outline text and coordinating dual-source blending, which can calculate shared curve coverage once instead of repeating it in three colour passes.

The next indexed opaque RGB555 candidate uses canonical output decoding. Candidate shader SHA-256 is d0a2e242201bd96c1942673d18d24a5610d5b8572c0919fca246893579441405. It directly quantizes opaque indexed palette bytes, retains generic partial-alpha behavior, and excludes UI background filtering, opacity tints, scale modes, other colour depths and integer-float composition. Numeric and performance checks are pending. It includes no smoke or text changes, so isolated comparison remains possible.

## Indexed opaque numeric result

Eight tests passed, no failures, with exact candidate shader d0a2e242201bd96c1942673d18d24a5610d5b8572c0919fca246893579441405 embedded once in each executable. The new independent palette-byte oracle checked 81,920 pixels, including all palette byte values, critical alpha values, two ordered blends and restoration of deduplicated instances after opaque-to-translucent palette changes. Four existing indexed/reference tests, two ordered blend tests and the 85,888-pixel direct output oracle also passed. Bound executable and raw-log hashes are in indexed-opaque555-oracle-results.json. Matched performance remains pending.

## Dedicated tile-cell RGB555 experiment

A separate tile555_fs entry fixes the colour model and removes modes the terrain vertex shader never emits: inverse-opacity tints, UI background filtering and procedural solids. It retains palette remapping, RGBA art, alpha steps, scale layers, inversion output and the existing exact mixed_source arithmetic. Palette channels stay integers through quantization, including partial alpha. Other colour depths and integer-float composition retain the generic pipeline. There is no resolution or feature reduction.

The immutable source is source-tiles555 and candidate shader is tiles555.spv, SHA-256 205615534df6338a51dc46e8a2d11591bfe6569aff9f081104da39fe8acd4e9f. It does not include the separate opaque-indexed shortcut in generic tile_fs. The generic function remains unchanged. Static SPIR-V instruction counts are 422 for tile555_fs versus 1,669 for tile_fs, with zero versus one loops. These are not machine-code or timing measurements.

All 58 numeric GPU tests passed with zero failures. They include 38 general GPU/reference cases, 10 text/reference cases, per-view bindings and filtering, indexed remapping and zooms, ordered quantized blends, 163,840 manually calculated palette/alpha pixels through both free and cell-layer paths, and 85,888 direct output pixels. Each executable was checked to embed the candidate shader exactly once. The complete per-process exits and bound raw-log hashes are in tiles555-oracle-results.json.

The same-binary production-layout benchmark is perf240-tiles555, SHA-256 43a41dee8dc4ab3e63eb93084af97f909a62b21ccbb5e870856e13488b1b596a. TILE_RENDER_TILES555=0 selects generic cells; 1 selects the dedicated entry. The setting is read once during renderer construction and retained as a boolean. It adds no per-frame environment lookup or synchronization. Public quality geometry, sharp-text arrangement and outline zoom selection match the performance owner's current harness. It is compiled and ready for matched performance measurement; no shared production Tiles555 selection has been applied.

## Text annotation control investigation

The text owner initially saw one plate omission in an isolated three-tap full parallel test run. Canonical and current-dual selected controls passed. Read-only comparison found identical free_vs, tile_vs, tile_fs and output_fs instruction streams, including recursively canonicalized referenced types, constants and layout decorations. The unchanged three-tap artifact subsequently passed the selected annotation test and complete 18-test suite when run serially. The original concurrent failure is retained; there is no established compiler-causation finding and no shader workaround was added for it.

## Shared source contributions

The rejected output decode experiment was removed. The pending generic opaque indexed branch remains until its matched performance result. Added the native_palette_alpha_fs helper supplied by the exact-smoke owner, without changing ordinary paths; added the glyph_cache shader module export requested by the text owner. Those owners own their correctness and integration. Two independent output/palette GPU oracle files were added to the shared test suite; the current output oracle selects the dual or fallback module according to device capabilities.

Completed rejected output host target was cleaned with cargo clean, reclaiming 1.5 GiB. The checked byte-oracle executable, immutable shader/source identities and evidence remain.

## Native-size subpixel dual-source candidate

Parent authorized source-only implementation after the prior candidates. The shader now fetches the coverage texel once and emits independent RGB coverage through dual-source blending. Quantized colour reuses the independently validated text helper, including zero-coverage channels. Direct colour retains the fetched UNORM coverage to avoid an additional byte-to-float conversion. The existing three-pass entries remain. The renderer and build helper owners are coordinating the selector, RGB-only write mask, feature fallback and SPIR-V output decorations. No build, GPU run, numerical pass or performance result is claimed yet. Broker permission is still required. The independent oracle source is `subpixel_dual_oracle.rs`.

## Matched shader candidate decisions

The FPS broker completed three 240-frame, zero-reuse full-redraw runs per workload. Same-binary Tiles555 native 4K median FPS rose from 70.510 to 76.505. GPU p50 fell from 12.249320 to 11.114560 ms, p95 from 12.548040 to 11.488880 ms. Corrected production scale2 GPU p50 fell from 37.044720 to 36.470400 ms. Shared source integration was explicitly approved: tile555_fs was inserted byte-for-byte from the numerically certified candidate, with the renderer owner adding only the proven RGB555 cell selection and retaining generic fallback. Exact current-union checks remain required.

The independent generic opaque indexed shortcut regressed native GPU p50 from 12.283960 to 13.186120 ms and scale2 from 37.042920 to 37.938760 ms. Its whole branch was removed from shared generic tile_fs. Forced quads also regressed, so existing octagons are retained. Raw matrix summary: `/mnt/datasets/c3r-build/perf240-sol/evidence/candidates/candidate-summary.csv`. Unrelated host CPU activity was retained in the evidence; these are repeated relative GPU measurements, not an isolated final 240 FPS acceptance result.

Source union now includes native palette smoke, text dual-source/cache, native-size SubpixelDual and the accepted Tiles555 cell shader. The release owner will build one coherent checkpoint SPV after broker release. SubpixelDual oracle uses only documented TileWorld colour depths 0/4/5/6/7. Its 19,552,836 pixels per path cover every RGB555 destination level, source coverage and tint byte, indexed remapping, ordered overlaps, target alpha, zoom phases and clipping. Raw output depth8 remains covered by the separate output decoder oracle.

## Current combined checkpoint gate

The release owner built immutable source 86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e. Full SPV is f9968edd2867ca9857731cd0b1c09d2172cb019e5523065b942507b249d7dc75; feature-free fallback is 7ff6d943a54e7255df2cd160e7239aee067f89470156e5a50b3c1e5079857a5d. Both strict Vulkan 1.2 validators and five SPIR-V helper regressions passed before GPU execution. All requested host executable hashes and shader embedding counts are bound by the release manifest. No duplicate host build was made here.

Under an exclusive RADV lease with the x86_64 AMD ICD, the regular tile-render library and nineteen integration executables passed serially: 157 passed, zero failed, one explicitly ignored native captured-table test. The ignored test was then run with its actual table and original ExplosionPalette PCX paths, passing 524288 exact pixel comparisons. The fresh three glyph-cache tests and asynchronous timing token, budget overflow and ring-pressure regression passed without the unsupported-timestamp self-skip. Package and captured-smoke result JSON files retain per-executable hashes, counts and raw-log hashes. There were no graphics processes left when this GPU lease was released.

The separately new exhaustive Subpixel oracle initially failed on its three-pass fallback because the test expected alpha 255 in quantized views. Both RGB-only pipelines correctly preserve the target clear alpha 37. This is a test expectation error, not a demonstrated implementation failure. The initial failure record, raw log and capture remain under the alpha-expectation-failure prefix. Only the shared test's two alpha expectations were corrected; frozen 866 source was not modified and production shaders did not change. A rebuilt corrected test executable and its fallback-versus-dual comparison remain required. Green unchanged suites should not be repeated because of this test-only amendment.

## Broader-depth strict oracle finding, still open

After the alpha expectation correction, the exhaustive oracle passed all RGB444 and RGB555 native cases, then failed in the production three-pass RGB666 fallback. At source coverage[5,109,243], tint[0,0,255] and clear[0,0,255,37], actual output is[0,0,251,37] instead of integer-expected[0,0,255,37]. The immutable original corrected test, executable, failed raw log and capture remain retained. This is established on the combined866 fallback path; unchanged historical31b shader control has been compiled but not executed yet, so historical classification remains pending.

The source's exact documented packed-FP16 guarantee is: "Translucent RGB444 and RGB555 sources use a binary fraction that survives packed FP16 fragment exports exactly." It further states, "Larger colour spaces retain the floating-point source calculation." However, the public ColorModel supports4..7 and describes integer rounding. The higher-depth fidelity discrepancy therefore remains an explicit open finding rather than a successful generic fidelity gate.

The default oracle retains strict integer assertions for all supported depths. Only an explicit diagnostic inventory environment flag allows6/7 mismatches to be counted and logged while capturing all pixels for the separate fallback-versus-dual byte comparison. The comparison JSON will distinguish those results. Root authorized a minimal isolated higher-depth format candidate and transition tests, with RGB555 fast paths unchanged. No production format policy has been altered.

## Historical control and exact Subpixel parity

The minimal one-pixel test reproduces the same RGB666 error with the unchanged historical canonical shader67f3cefc9e905d7404baf63a3d0a3b54ca775bddc61612f52aa57c9b7de1ceb0 and current866 shader. Both new selectors were explicitly disabled. The host renderer source and all dependency rlibs were unchanged866 in both controls; only the compile-time shader artifact differed. FrameParams, FreeSprite, SpriteDesc and OutputParams field layouts were verified identical between historical31b and866 source. Each expected-failing strict control returned the same[0,0,251,37] instead of[0,0,255,37]. Historical provenance, executable hashes and logs are bound in historical-subpixel-control-results.json and its build manifest. The issue is now proved to exist before the new shader optimizations.

The inventory-mode exhaustive comparison then passed byte-for-byte equality for19,552,836 pixels per path, including direct and every supported quantized depth, RGBA/indexed/remapped coverage, ordered overlaps, target alpha, clipping and zoom phases. RGB444 and RGB555 independent integer assertions remained enabled and passed. The independent higher-depth inventory remains OPEN, with61,325 RGB666 and134,041 RGB777 mismatched pixels in each path. Identical counts and captures establish preservation by SubpixelDual, not correctness of the broader formats. Default tests retain strict4..7 assertions.

The renderer owner prepared a one-file isolated FP16 format-policy candidate from immutable866, SHA966860722f6e9779c6f0a793b650f97018642381e7353cea8904ff70130208bb. It separates immutable adapter policy from effective world encoding, resets format-dependent pipelines/workspaces without recycling queued controls, and retains a picture's own decoding mode. Static review found matching color included in same_shape and explicit format checking in scratch_for. The candidate is unbuilt and unadopted during the broker's actualApp timing hold. A minimal rustc-only build script and independent strict/color-transition/cached-picture tests are prepared. No numerical conclusion about this candidate is claimed yet.

## Rejected unit-spaced FP16 format-only candidate

After the broker released the App window, the isolated renderer candidate was compiled with rustc against exact866 rlibs and canonical full/fallback shaders, without Cargo or any shared/live source mutation. The first strict RGB666 one-pixel check failed with the same[0,0,251,37] instead of[0,0,255,37]. The full exhaustive and transition gates were deliberately not run after this decisive failure. The candidate was not adopted, and the compiler/GPU lease was released immediately. Build and failure evidence are in `/mnt/datasets/c3r-build/higher-depth-fp16-sol/rustc-candidate-manifest.json` and `strict-candidate-results.json`. Enabling WARP's existing format policy alone does not correct this RADV path. No particular driver conversion mode is inferred from that result.

The parent requested source-level design only for a portable complete fallback. `portable-wider-integer-fallback-design.json` describes the existing destination-snapshot mechanism extended only to6/7. The shader would compute final per-instance integer RGB before emitting exact FP16-representable stored integers1536..1663 with blending disabled. Instance order, GPU text coverage and source sampling must remain exact. A tiled snapshot can bound additional texture memory to128KiB; command-count/memory accounting and alpha-channel semantics still need explicit gates. No implementation or adoption is claimed.

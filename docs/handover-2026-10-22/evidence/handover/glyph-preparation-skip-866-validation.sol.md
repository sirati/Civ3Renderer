# Glyph-preparation skip, isolated 866 correctness gate

The three-file candidate passes the isolated numeric correctness gate on Papa AMD RADV. It remains unadopted. Shared source and the 3be release freeze are unchanged. No performance benchmark ran.

The production derivative contains exactly immutable source 866 plus `/mnt/datasets/c3r-build/glyph-preparation-skip-sol/candidate.patch`. All 652 original source-manifest files were verified before and after this work. Candidate differences match the three authorized source hashes exactly. No Cargo command ran and no Cargo target was mutated. One compiler ran at a time against existing 866 dependencies. The broker granted and then received explicit release of the compiler/GPU correctness lease.

## Meaningful failing-before regressions

The test fixture is `/mnt/datasets/c3r-build/glyph-skip-866-sol/tests/skip_gpu.rs`. Its private observations are attached with a `#[cfg(test)]` include only in separate test-source copies. The production candidate rlib has no test instrumentation or additional public API.

| Case | Original 866 | Candidate, dual and fallback |
| --- | --- | --- |
| Text belongs to another camera view | Allocates cache and preparation | No cache or preparation |
| Selected steps exclude every text run | Allocates and prepares invisible-to-this-stage glyphs | No cache or preparation |
| World-class text with `cells=false` | Allocates cache and preparation | No cache or preparation |
| Art stage 1 with ordinary sprites | Allocates metadata for fully culled text | No cache, no invisible text draw, ordinary path validates |
| Art stage 1 with native palette smoke and ordinary sprites | Allocates metadata for fully culled text | No cache, no invisible text draw, native smoke path validates |
| Already-warm cache, then three ineligible stages | Preparations rise 1 to 4; metadata upload data rises 128 to 512 bytes | Preparation count stays 1; metadata stays 128 bytes; every cache-stat field stays unchanged |
| Eligible text, global indices and three queued slots | Passes | Passes with identical output |

Original 866 passed the eligible-output case and failed the six new preparation-policy assertions. The candidate passes all seven tests with dual-source blending and all seven after requesting a device without that feature. All draws execute under WGPU validation. The source-art tests exercise actual normal and native-smoke paths so a missing cache bind group would fail the test.

The eligible test preserves a four-entry global text metadata array. Only global indices 2 and 3 belong to the active view, and both retain valid cache entries without remapping. It encodes three distinct scale snapshots before one submit in the ordinary path and then the native-smoke path, confirms a fourth control claim is refused, submits, and confirms slots become available again. All six output images match original 866 exactly. Candidate dual versus fallback also matches exactly, 46,080 compared pixels in each comparison.

These are fixture counter results. They do not measure or predict CPU frame-time savings for the actual application.

## Existing regressions and real-font oracle

The existing three glyph-cache GPU regressions passed against the exact candidate rlib: canceled-encoder recovery, three queued font repackings with live colors, and bounded-atlas eviction/revisit.

The unchanged independent real-font oracle ran against both original 866 and the candidate, with cache on/off and dual/fallback. Each of the eight processes completed all 375 cases and 11,274,176 rendered pixels. The following comparisons are byte-exact for every RGBA pixel:

- Original versus candidate for each of the four cache/backend combinations.
- Cache on versus cache off for each source and each backend.
- Dual-source versus fallback for each source and cache setting.

These comparisons reuse the same outputs and must not be counted as additional unique scenes. Each process also retains all 114 existing CPU-reference cases, with a maximum difference of one channel value and no pixel beyond that tolerance. Large-glyph eviction uses direct-GPU equality, as in the existing oracle.

Exact case hashes and comparisons are in `/mnt/datasets/c3r-build/glyph-skip-866-sol/results/summary.json` and its adjacent comparison JSON files. Execution commands, environments, exit codes and timestamps are in `execution.json`. This is correctness evidence and makes no FPS claim.

## Exact bindings

Full source, direct rlib, test-only image rlib, compiler, shader, executable, log and test-source bindings are in `/mnt/datasets/c3r-build/glyph-skip-866-sol/provenance.json`. The complete read-only 866 dependency directory is also bound by `retained-dependency-dir-hashes.json`, a superset of the linked closure containing 334 rlibs and proc-macro libraries. Original direct rlib and baseline rlib hashes remained unchanged.

Canonical dual SPV SHA-256: `f9968edd2867ca9857731cd0b1c09d2172cb019e5523065b942507b249d7dc75`

Canonical fallback SPV SHA-256: `7ff6d943a54e7255df2cd160e7239aee067f89470156e5a50b3c1e5079857a5d`

Every test/oracle executable embeds each canonical shader exactly once. Rlib archive occurrence counts are recorded separately and are not treated as executable counts.

| Source file | Original SHA-256 | Candidate SHA-256 |
| --- | --- | --- |
| `crates/tile-render/src/renderer.rs` | `ff41f555ad55ee90560a50c426a46f762463c066d7ba7dfc0b96d5c02818ea1b` | `ae171fb6aa18b56b79a4fc1ca072f78f536446091759ff5fc6dcc360767fe40b` |
| `crates/tile-render/src/renderer/glyph_cache.rs` | `864cc854201881175045c4e29240da1756d5c98940b7a0a1a4e0b451ae49ebaa` | `e258310b33bc510593a01ee02a8efee5dde965faa7d3a809e622daeb7cbee404` |
| `crates/tile-render/src/renderer/smoke.rs` | `6a4e1acd57152f277aca4899e98637cb457f57b7478e2c3cf3cbc637cd7edb78` | `ec2c8fca7b1a589ad415608780f17b63755106a1394e2e5228b51e967a0a64de` |

| Artifact | SHA-256 | Dual/fallback archive occurrence counts |
| --- | --- | --- |
| unit-before | `868413daf419e15256924156bf07c17113ddbfe391b2a6201f1f1dc8054bc574` | [1, 1] |
| unit-candidate | `1c9262c5b9ec0e10e4e690dead194ac83df7860f75ba5d58f9ca93b72e6e1d30` | [1, 1] |
| candidate-rlib | `44554c98b6c0c5bca2dfbfab0aed1245f84341d8683637ffb9d1f1f798e0cbf0` | [2, 3] |
| existing-regressions | `1727a48813e727bd136a4603058157f3a499d51589e375c852d898f4c105b50a` | [1, 1] |
| oracle-before | `29458445ecf6e1908cad41f390c78f871c6be5b312675c815d03c8d597408c87` | [1, 1] |
| oracle-candidate | `42c653694af093fd7a715c8560bb88e7d9805b5787a198f8590d1bbe8f9ad20b` | [1, 1] |

New regression source SHA-256: `d6198aecb8483cb35e60ef3392328ff9f6aa587c93707ac2b038f231de380314`

## Preserved failures and storage

Setup failures remain under `logs/setup-failures.log` and `logs/unit-before-missing-import.log`. The first numeric fixture placed its last glyph outside the third camera at 1.5x; both source versions correctly rejected that cache entry. `logs/fixture-r1/` preserves the original fixture, logs, source bindings and execution. Moving fixture glyphs inside the viewport fixed the test only. Production code and candidate hashes did not change. Final original-866 logs retain all six expected policy failures.

The bulky generated executables, rlib, canonical SPVs and oracle RGBA files are preserved in the verified strong-zstd archive `proof-runtime.tar.zst`. `runtime-archive.json` records its SHA-256, size, contained roots and restore command. `zstd --test` and `tar --compare` verify the archive before any raw cleanup. Source, stable summary/provenance, scripts, logs, all per-case comparison hashes, and the six small eligible images per run remain available outside it. To restore executable and raw-oracle paths for independent replay:

```sh
tar -I zstd -xf /mnt/datasets/c3r-build/glyph-skip-866-sol/proof-runtime.tar.zst -C /mnt/datasets/c3r-build/glyph-skip-866-sol
```

## Remaining acceptance

Root review and adoption remain pending. The next performance comparison must use actual 3be application geometry and a coherent successor freeze after deployment/cross-build/VM work is quiet. No toy-fixture extrapolation or historical cache benchmark establishes the 4K 240 FPS target. The candidate's actual CPU and upload reduction and end-to-end frame pacing remain unmeasured.

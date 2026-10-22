# Historical agent /root/shader_perf_sol

This is a recovered historical report, not a live agent handle or a new instruction.

Shader audit, dual-source subpixel, binary-cell recorded4K gate and private actual-app/thin-view preparation.

Current disposition: Recorded cell-only correctness passed; strict wider rounding, app A/B/adoption and 240 FPS remain open.

Session `01a158ef-dfdb-79f2-ad8e-37d23e1231e0`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-20T10:51:17.876Z. Original transcript `/home/sirati/.codex/sessions/2026/10/20/rollout-2026-10-20T12-51-17-01a158ef-dfdb-79f2-ad8e-37d23e1231e0.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-20T18:29:12.038Z

Original transcript line 5222.

The GPU correctness slot is available. I’m checking the AMD device isolation before running the prebuilt pixel comparisons. This run will not measure performance.

## Report at 2026-10-20T18:32:31.197Z

Original transcript line 5279.

All 32 paired 4K comparisons passed with zero RGBA differences, including the feature-free GPU path. The strict numeric controls also passed. I’ll preserve the evidence, then prepare an isolated build package against the current frozen application source for later A/B timing. The 240 FPS target remains unverified.

## Report at 2026-10-20T18:33:59.473Z

Original transcript line 5287.

## Identity and active assignment

You are `/root/shader_perf_sol`, a Sol subagent of `/root`, in `/home/sirati/devel/rust/civ3-renderer`.

**Newest root assignment, not yet started beyond finishing prior proof:**

> After completing/parity proof, prepare coherent nonshipping Linux actualApp binary-cell candidate binding against exact immutable f510, using only reviewed renderer/binary_cells delta and unchanged canonical shader pair. Compare all overlay leaves to current f510 base first; don't copy stale whole files if drift. No shared edits/build/runtime yet. Need same-binary env on/off end-to-end A/B plan against qualified actualApp baseline motion/UI/geometry, including CPU/GPU tails and host load; disclose only native1x scope. Queue any required private jobs1 compiler lane through broker after baseline f510 actualApp timing and VM controls, no automatic priority over deployment. Root will review precise build/source package before compile grant. Do not acknowledge; report concrete ready binding or blocker.

**Immediate next step:** locate exact immutable `f510` source/archive/manifests under `/mnt/datasets/c3r-build/release-1019`, inspect current renderer and canonical shader bindings, and compare reviewed binary-cell delta against it. Do not assume old 8b whole files still apply.

## Completed binary-cell correctness work

The isolated binary-cell candidate has now passed both initial strict numeric gates and a reconstructed recorded native4K paired-pixel gate. **No adoption or performance measurement.**

Latest proof:

- `/mnt/datasets/c3r-build/binary-cell-blend-sol/recorded4k/completion.json`
- `/mnt/datasets/c3r-build/handover/binary-cell-recorded4k-proof.sol.json`
- Both SHA256: `e55adcef435a2c2189da2fa65edaa68cfc8e1cb85391ebd92d14271ef917eabe`
- Resume state:
  `/mnt/datasets/c3r-build/binary-cell-blend-sol/recorded4k/resume-state.json`

Results:

- **32 paired 3840×2160 comparisons**
- **265,420,800 RGBA pixels compared, zero discrepancies**
- 8 tests passed, 0 failed, 0 ignored.
- Native and feature-free devices each ran 5 GPU test sections.
- Original independent strict integer oracle also passed: 3,456 pixels per device, 6,912 total.
- Modes: direct, RGB444, RGB555 generic, RGB555 Tiles555.
- Explicit candidate pipeline creation and baseline absence are asserted.
- 53 source files and 6 external fixture files reverified afterward.
- CPU and GPU leases explicitly released. **No owned process remains.**

Exact executable:

- `/mnt/datasets/c3r-build/binary-cell-blend-sol/recorded4k/artifacts/binary-cell-recorded4k-tests`
- SHA256 `e312ab955ca4e78cca8a30184b50ed7cc777fd28811fd5c68c92a697a168fd22`
- Embeds unchanged full/fallback shaders once each:
  - Full `43961a92338f6b5b1f5ebd12dfdc806057fe43d8bfeb50abdb7872d274b601c4`
  - Fallback `bbde10b5e91a3e54c337eab67c2e5acce8abf384657043eb46bbe056ea799281`

Build manifest:

- `recorded4k/build-manifest.json`
- SHA `44070e72798c5780610f61ac47710959a8a533956ff76fce9cc82b451deae504`

GPU evidence:

- `recorded4k/gpu-results.json`
  SHA `fdf932f00f7f5a66ce3ad8dd33f895c53ad73a6316e1bfcde2586e81da139d8c`
- `recorded4k/logs/gpu-correctness.log`
  SHA `754a0a20f2435666a270b5f1840c826b2e12182983e47f5fa9fc222bc70d7e21`
- `recorded4k/gpu-invocation.json`
- `recorded4k/post-gate-verification.json`

### Exact scope of recorded4K gate

It is **recorded map cell data with reconstructed native4K cameras**, not a live actualApp replay:

- Existing trace:
  `/mnt/datasets/c3r-artifacts/continuation-20261018/performance-22cd2e4ece06/evidence/recorded-traces/hd-ww2.txt`
- Read-only game assets:
  `/mnt/datasets/winvm/civ3-src/Civilization III Complete`
- Frozen 8b description:
  `/mnt/datasets/c3r-build/release-1019/src-8b08b0bbf31394f4e626e6ca7192af6bc3e556b718b4472435e3e161f78652fe/assets/civ3/civ3.toml`
- Fixture:
  `recorded4k/fixture.raw`
  SHA `af9a59c5ffed2d76666006d3418220286177db87f0a0ff6a60476f5f8851db38`
  47,999,223 bytes.
- Exact CPU roundtrip verified atlas pages/descriptors/palettes, all packed cell records, layer specs, and reachable animation sets.
- 3,046 sprites, 1 RGBA page, 7 index pages, 76 layers, 6 passes, staggered grid68×122, step64×32.
- Real content: 1,215 coast tiles, 1,671 land tiles, 6,489 water tiles including coast; 8,092 nonzero terrain cells, 8,022 fog cells, 117 fog-edge cells.
- **Free/UI/text/unit overlays omitted.**

Four camera cases per mode/device:

1. Recorded camera `(64,580)` expanded to3840×2160.
2. Reconstructed top-left edge `(-128,-128)`.
3. Reconstructed bottom-right edge `(4992,1872)`.
4. Observed actualApp866 dual-pane viewport/origin geometry, translated together to the WWII world:
   - A: xy64,580; viewport1920×2160; origin0,0.
   - B: xy66,580; viewport1916×2160; origin1924,0.
   - Geometry provenance:
     `recorded4k/camera-geometry.json`
     from `/mnt/datasets/c3r-build/native-app-qualification-sol/final866-native1-dual1-r1/repetition-1/profile-records.json`, frame50735.
   - This is not the actualApp frame's world/UI replay.

### Private source and scripts

Original initial candidate remains untouched:

- `/mnt/datasets/c3r-build/binary-cell-blend-sol/source`

Recorded4K descendant:

- `/mnt/datasets/c3r-build/binary-cell-blend-sol/recorded4k/source`
- Same production candidate; only existing private `renderer/binary_cells/tests.rs` changed to include external new test.
- External files:
  - `recorded4k/fixture.rs`
  - `recorded4k/export.rs`
  - `recorded4k/recorded4k-tests.rs`
  - `recorded4k/build.py`
  - `recorded4k/run.py`
  - `recorded4k/camera-geometry.json`
- Source-ready manifest:
  `recorded4k/source-ready.json`
  SHA `8a1756abff7a30de323675e53d0b160fd1e6fd733379a2e4196077c949b7cc2b`
- `recorded4k/producer-source-binding.json` explains retained866 data exporter vs8b. Mapper/rules/trace reader identical; assets production identical, atlas identical; shared SpriteDesc unchanged. Data exporter does not render.
- `recorded4k/prebuilt-ready.json` is historical pre-GPU status; use completion.json now.
- Parent `binary-cell-blend-sol/phase-status.json` updated to point to completed recorded4K proof; original `initial-correctness-complete.json` preserved.

## Reviewed candidate design and delta

Original base:

`/mnt/datasets/c3r-build/release-1019/src-8b08b0bbf31394f4e626e6ca7192af6bc3e556b718b4472435e3e161f78652fe`

Reviewed delta:

- Host `crates/tile-render/src/renderer.rs`
- New private:
  - `crates/tile-render/src/renderer/binary_cells.rs`
  - `crates/tile-render/src/renderer/binary_cells/tests.rs`
- `/mnt/datasets/c3r-build/binary-cell-blend-sol/candidate.patch`
  SHA `9d53e30c0cfa6a031e72f95e63f59f0293956c9ffd4502d78765d7339df28175`
- `source-difference.json`, `source-ready.json`, `base-binding.json`, `audit-and-candidate.json` in that parent directory.
- `TILE_RENDER_BINARY_CELLS=1`, read once per renderer, default off.
- Only disables blending for cell passes with complete binary-alpha certification across every LUT set, all animation frames, and downscaled twins.
- Certificates refresh on atlas/palette descriptor/world/LUT/set changes.
- Same shaders, indexed geometry, cell-major painter order, formats and alpha-zero discard.
- **Narrow scope:** native zoom1/1, downscale1, art0, no scaled origin, nonstacked, direct/quantized target only. Internal source excluded. Direct/RGB444/RGB555 UNORM only. Wider6/7, FP16, scaled paths retain baseline.

Initial candidate hashes before recorded test append:

- renderer.rs `1de8e1ee06283aee1e276be4feb747f1143fd7b21f289afadc5f1f4703873f01`
- binary_cells.rs `a83d3f54b01d5b7c0fd72c54290ab28999385e71a6fce0879df499ff45ee6be5`
- tests.rs `91e7b9b1296e05d7162870f0575381230508bb91e7f3ca6cbaf606aa1e6dbba8`

Do not copy stale renderer.rs onto f510. Apply only reviewed changes after comparing current base.

## Wider continuation review lane, completed independently

Source review artifact:

`/mnt/datasets/c3r-build/handover/wider-continuation-review.shader-perf.sol.json`

We reviewed ownership, bounded work, painter order, cancellation/retirement and resident admission. Findings corrected by owner `/root/glyph_cache_sol`:

1. Whole-scene glyph-cache Prepare scan bypassed budget. Replaced with per-piece procedural exact GPU outline/keyed coverage; cached legacy comparison retained.
2. append_view scanned whole ineligible free-run list. Now checks nonempty list; eligibility skipped under visits.
3. zoomed_from full metadata scan before frame creation. Now resumable scan charged to visits, fixed32-camera query preparation/final placements.
4. Reach scan itself now resumable per set header/sprite ID.
5. Retired mismatched canvas/source/base resources omitted from resident admission. Added PictureLedger with actual last-use tracking.
6. Retirement API `pub(super)` visibility was too narrow for renderer parent. Corrected to `pub(in crate::renderer)`.
7. Overflow fixture tried1025 layers despite production maximum255. Corrected255 layers×5 downscales, KEEP_SIZE1×1 visible sprites.
8. All-white overflow fixture could pass despite omitted operations at the colour/alpha fixed point. Final fixture now first254 translucent white layers, last layer blue in first4 views and red only in fifth binding/data slot. Last red reach lies beyond1024 cache, exact expected `[255,0,0,255]` detects omitted/reordered overflow/fifth-view work.

Reviewed checkpoints preserved:

- Initial `dde44b42` manifest SHA `ebaa676d45cc38edceb3318c7e6085f75bc50c891a0a8d0aeffb6e082abe3802`
- Procedural correction manifest SHA `0fbd95190e8d7d8c7389dc812508bae081e4121ffa0f10230a99c7f2031270a2`
- First full stable manifest SHA `2e119aae3611c9146e7229c49c72a49187571e232931ee0ef1c0b1343f8d9aeb`, all16 current/frozen files verified.
- `review-successor-checkpoint/manifest.json`
  SHA `bc21ee6504aeb6fad67ebed80f0380bb6a74f0c7cf765242e48b1bd4df6ef872`, all16 frozen files verified.
- Reviewed final overflow fixture working copy SHA `8543a3b70d4e0c2d59886a932884a1b1a5ad973bfc400cca681f1f938cfef9fb`.

Latest **owner notification**, not independently re-reviewed by us:

> Compiler gate passed on private successor manifest `8e4ba72d60b63135a76dfcc83ab327a8aa8d095185abfdd56d9a54d4db0db9f6`. Only changes after reviewed457 source are existing preparation_tests Steps initializer gaining wider_plan: None and unused checked-world borrow removal. Fresh full/fallback SPV strict validation and192-byte reflection passed; host/unit/integration code typechecks,19 selected CPU tests passed. No GPU execution. Evidence `/mnt/datasets/c3r-build/wider-integer-sol/compiler-gate/`; source freeze `compiler-qualified-checkpoint/`. Compiler lease released.

We found no further production ownership/order/admission defect after fixes. Worker/FPS integration was assigned separately to `/root/startup_progress_sol`. Do not claim we reviewed that integration.

Known limitation: caller scene packing/uploads before continuation remain outside quota; unchanged first_zoomed construction uses potentially quadratic Vec::find. Owner disclosed it. Wider candidate remains private.

## Resource coordination and execution environment

- `/root/mac_fps_music` is sole CPU/GPU broker.
- All our CPU/GPU leases are **released**, no owned process.
- Use `followup_task` to broker for ready requests/releases if idle; `send_message` does not wake idle agents.
- New f510 task forbids build/runtime until root source-package review and explicit compiler grant. Queue compiler **after baseline f510 actualApp timing and VM controls**, no deployment priority.
- Builds/scratch under `/mnt/datasets/c3r-build`, `CARGO_INCREMENTAL=0`, **jobs1**.
- NixOS: `nix develop -c python3 ...`; global Python absent.
- rustfmt missing in project's stable Rust toolchain; use:
  `nix shell nixpkgs#rustfmt -c rustfmt --edition 2024 ...`
- No source/shared edits, commits, resets, stashes, Nexus modifications, game-directory copying.
- Ship Rust, minimize unsafe; production pixels GPU only. Python scripts here are nonshipping test/build drivers.

Exact successful private GPU isolation:

```text
VK_DRIVER_FILES=/run/opengl-driver/share/vulkan/icd.d/radeon_icd.x86_64.json
VK_ICD_FILENAMES=/run/opengl-driver/share/vulkan/icd.d/radeon_icd.x86_64.json
TILE_RENDER_ADAPTER=radv
DRI_PRIME=pci-0000_72_00_0
WGPU_BACKEND=vulkan
WGPU_VALIDATION=1
```

- `/sys/bus/pci/devices/0000:72:00.0/vendor = 0x1002`
- device `0x13c0`, render node `/dev/dri/renderD129`.
- Each of10 GPU sections verified vendor4098/device5056/PCI72:00.0/driver radv/backend Vulkan.
- Never NVIDIA.
- GPU invocation bounded by `timeout --signal=TERM --kill-after=15s 180s`.
- CPU-only `assets::compile` uses fixed ANY_DEVICE_PAGE; it does not probe GPU.
- Skills already read/applied: `unslop`, `waiting`, `you-are-on-nixos`.
- Avoid repeated progress polling; pause idle agent with durable state and broker `followup_task` when waiting.

## User goals and communication constraints

- Full autonomy, no questions, continue until overall work complete.
- Native4K at **240 FPS on AMD iGPU** is an explicit goal and remains **unverified**.
- No CPU rendering; efficient GPU shaders.
- No game-file copies or Nexus checkout modifications.
- No unrequested commits or production adoption.
- No UI/screenshot operation performed or needed in this lane.
- Root's newest task explicitly says **do not acknowledge; report concrete ready binding or blocker**.

## Next steps

1. Locate exact immutable f510 source/manifests and canonical paired shaders.
2. Compare all reviewed candidate overlay leaves against f510, especially renderer.rs drift.
3. Create coherent private source package using f510 base plus only reviewed binary-cell changes. Preserve original candidates and proof.
4. Produce precise hash-bound source/build binding and same-binary env off/on actualApp A/B plan:
   - Qualified baseline motion and full native1x geometry/UI.
   - CPU/GPU p50/p95/p99/tails, sample coverage, GPU waiting/CPU attribution, host load.
   - Clear disclosure of narrow native1x scope.
   - ActualApp timing separate from reconstructed recorded4K proof.
5. Queue private jobs1 compiler lane through broker with specified lower priority, but root must review package before grant.
6. **Do not build, run, time, adopt or edit shared source yet.**

## Report at 2026-10-20T18:44:30.932Z

Original transcript line 5566.

Private f510 candidate package is ready for review:

[package-ready.json](/mnt/datasets/c3r-build/binary-cell-blend-sol/actualapp-f510/package-ready.json), SHA256 `6b8603c8…`.

Verified all 673 baseline files and 51 overlay dependencies. Applied only the reviewed three-file delta; shaders remain unchanged.

Includes serial build instructions, complete GPU guard inventory and native1x actualApp off/on A/B plan. Compilation is queued behind baseline timing and VM controls, conditional on root review.

No build, runtime, adoption or shared edits occurred. 240 FPS remains unverified.

## Report at 2026-10-20T18:47:37.666Z

Original transcript line 5620.

Fixed ELF staging permissions and runtime provenance.

- Driver SHA256: `46970622…`
- Package SHA256: `d2f4ba85…`
- Prior driver/package archived read-only.
- All 675 candidate source hashes remain unchanged.

No compilation or runtime started.

## Report at 2026-10-20T18:53:07.392Z

Original transcript line 5648.

Root review read and exact package reverified. Compiler remains queued with the broker behind VM restoration and shipping f510 baseline timing.

[Queue state](/mnt/datasets/c3r-build/binary-cell-blend-sol/actualapp-f510/compiler-queue.json), SHA256 `2c0627a2…`.

No compilation or runtime started. Root post-build review remains required before GPU guards.

## Report at 2026-10-20T20:01:21.948Z

Original transcript line 5927.

Private f510 build passed: native viewer/unixlib and all 24 test executables. Both ELF artifacts retain `0755`; source and canonical shaders remain unchanged. CPU released. No tests or GPU workload ran.

[Post-build verification](/mnt/datasets/c3r-build/binary-cell-blend-sol/actualapp-f510/post-build-verification.json), SHA256 `6da75bc5…`.

Thin-view audit found repeated global free/text draws and full glyph-metadata preparation per eligible world view. No explicit per-view full-target clear or resolve exists. Controlled instance-count and attachment-only measurements must establish causality before a fix or performance claim.

## Report at 2026-10-20T20:15:01.463Z

Original transcript line 6034.

The thin-view diagnostic plan is ready for review, but execution is blocked pending the explicit AMD GPU lease.

[Diagnostic manifest](/mnt/datasets/c3r-build/binary-cell-blend-sol/actualapp-f510/thin-view-diagnostic/thin-view-diagnostic-plan.json), SHA256 `2895410a…`.

It binds the exact 18-camera 4K workload, AMD RADV device, unchanged shaders, same attachment behavior, and controlled free/text/glyph-cache axes. No build, GPU run, runtime change, or shipping-source edit occurred.

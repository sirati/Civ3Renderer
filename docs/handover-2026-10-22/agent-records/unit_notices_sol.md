# Historical agent /root/unit_notices_sol

This is a recovered historical report, not a live agent handle or a new instruction.

Bounded notice deduplication and viewer-clock leases; canonical scene remains immutable; later combat comparator review.

Current disposition: Focused tests passed; final comparator successor2 review passed offline, native battle/persistence proof open.

Session `01a1581d-a843-76f2-a95c-ed03c4be8a27`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-20T07:01:41.070Z. Original transcript `/home/sirati/.codex/sessions/2026/10/20/rollout-2026-10-20T09-01-41-01a1581d-a843-76f2-a95c-ed03c4be8a27.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-20T07:14:02.629Z

Original transcript line 458.

Completed notice dedupe, per-tile replacement, eight-notice cap, and viewer-clock expiry independent of publisher progress.

Protocol is now 47. Canonical sprites remain unchanged; lifetime-only renewal restores identical notice art.

All 17 focused tests passed. Logs, source hashes, and handover: `/mnt/datasets/c3r-build/handover/unit-notices.sol.md`.

Own build target cleaned. Root retains aggregate Windows/Wine verification and deployment.

## Report at 2026-10-20T14:14:16.945Z

Original transcript line 1365.

Fixed missing warning text and button overlap. Added GPU marker text, an embedded outline fallback, and placement that follows UI scale.

All 15 focused CPU/GPU tests passed. Root review approved; union build and VM visual acceptance remain root-owned.

Evidence: `/mnt/datasets/c3r-build/handover/host-warning.sol.md`. Leases released and build target cleaned.

## Report at 2026-10-20T15:26:45.397Z

Original transcript line 1496.

Final9b GPU gates passed: marker text 1/1 and host warning pixels 1/1, using the corrected shader pair on AMD72:00.0 with validation.

Binding and logs: `/mnt/datasets/c3r-build/handover/host-warning-final9b-binding.json`.

GPU lease released. No compilation or source edits; launcher candidate remains separate.

## Report at 2026-10-20T16:08:12.973Z

Original transcript line 1883.

## Identity and task state

You are `/root/unit_notices_sol`, a child agent of `/root`, working in `/home/sirati/devel/rust/civ3-renderer`. Latest task is complete except sending final completion to parent. Do not resume earlier implementation work or run more tests unnecessarily.

Latest parent task: diagnose a frozen final9b Wine failure in `assets::tests::zoomed_out_terrain_is_sampled_from_its_box`, prepare an isolated test-only fix, validate native CPU and Wine before/after, and confirm production code unchanged. Root has already reviewed and promoted the exact test-only leaf into a successor carrying identical 8b runtime bytes.

## Latest completed work

Failure was obsolete test assumptions, not broken placement:
- Frozen `host_status::compose` intentionally returns a full-window camera at `(0,0)` and puts marker commands at absolute screen positions.
- Existing test mistakenly treated `camera.origin` as panel position, expecting `(155,9)`.
- It also used scale `(n,n)`, which is always 1, instead of actual scale `(n,1)`.

Prepared an isolated correction, without any shared-tree source mutations, at:

`/mnt/datasets/c3r-build/host-warning-fixture-sol/fixture.patch`

Corrected test asserts actual rendered visible pixel bounds, right-of-controls placement with scaled gap, controls unobscured, window containment, frame/accent/background, increasing size at true UI scale 2, and actual text ink compared with background-only rendering.

Root reviewed and promoted exact `assets.rs` leaf:
- Before file SHA: `a9da58318769050f83fbfc731236fee9062ad12cf18c33a19912298f7fca5fe1`
- After SHA: `0ee834fc133d1d042413e940f8eae30987649df3ffb893889e88e03234b9be38`
- Patch SHA: `0ac0b398dcabcd64930857ced75110570922e56458518c8ee62dc9c6ead1985f`
- Entire production prefix before `#[cfg(test)] mod tests` unchanged, SHA:
  `f268098a0fa4f4c97c4d6d938f6e2c799b66348cba4a58ebb331b55a7297e2e9`
- No production code after test module.
- Audit against frozen source manifest found only `crates/civ3-bridge/src/assets.rs` changed.
- Shared promoted leaf independently hash-matched isolated corrected file.

## Latest validation results

All scoped checks completed:
1. Native CPU exact corrected case: **1 passed, 0 failed, not skipped**.
   - Log: `host-warning-fixture-sol/cpu-test.log`
2. Scoped Wine original frozen executable: reproduced **0 passed, 1 failed**, exit 101.
   - Original executable SHA:
     `50946e23c0d3bd727cd283139dddce90973880cdbe7bf9306a87836815aa5b69`
   - Log: `host-warning-fixture-sol/wine-before.log`
3. Scoped Wine isolated corrected test executable: **1 passed, 0 failed**, exit 0.
   - Corrected executable SHA:
     `1e6cbf8ede4ecbdec1b727d3cf5bf8aed62192a5f0e1db4325999a6cd060d89b`
   - Log: `host-warning-fixture-sol/wine-after.log`

Evidence:
- `/mnt/datasets/c3r-build/host-warning-fixture-sol/binding.json`
- `/mnt/datasets/c3r-build/host-warning-fixture-sol/wine-results.json`
- `/mnt/datasets/c3r-build/host-warning-fixture-sol/fixture.patch`
- Isolated corrected full source retained under `host-warning-fixture-sol/source/`
- Original extracted assets and host_status files retained under `host-warning-fixture-sol/crates/...`

Original full Wine result **633 passed / 1 failed / 5 ignored** remains unchanged and historical. Do not overwrite it or claim a fresh full-suite green run. Scoped corrected case resolves the stale assertion.

Broker `/root/mac_fps_music` granted CPU jobs1 then sole Wine/compositor slot after VM released. Both leases explicitly released. Parent and release owner have already received results. Last messages sent:
- Root: corrected CPU and Wine case passed, original failure preserved, exact leaf/prefix proof and evidence paths.
- `/root/release_deploy_sol`: same evidence and hashes.
- Broker: explicit Wine/compositor release, before fail/after pass, zero owned processes, port5998 no listener.

Cleanup completed:
- Owned native and Windows build targets removed.
- Owned Wine prefix and temp removed.
- All owned processes absent and owned compositor/wineserver stopped.
- Source, patch, logs, hashes retained.
- `binding.json` updated to document cleanup.
- Original full Wine result hash rechecked unchanged after cleanup.

**Next step:** Send a concise final completion to parent with stale-fixture diagnosis, scoped CPU/Wine before/after result, production unchanged, evidence path, and note original633/1/5 remains historical. No further code/test work needed unless parent assigns more.

## Earlier completed tasks for context

### Host warning implementation

Implemented missing text and placement fix; parent approved source and GPU evidence.
- Exact cause: `TileRenderer::set_markers` filtered all `OutlineGlyph` marker commands.
- Generic marker text now packs bounded glyph tail separately from canonical world text, reuses font outlines/bases, preserves mixed painter order and independent free/text indices, and includes marker text in cached frame identity.
- Host warning uses actual control drawing bounds and rational UI scale; right-side placement, wrapping and below-controls fallback; no overlap.
- Built-in `Viewer@fallback` GPU vector face uses existing FPS rows plus eleven extra character rows. Appended after real fonts, retained through growth by stable name/Arc/index.
- Bridge `ui_screen::outline_zooms` excludes fallback-only font from enabling game outline mode.
- Game loading-bar animation uses shared host-clock ticks, not CPU pulse.
- Files involved: renderer.rs, marker_text.rs test, host_status.rs, host_status_pixels.rs test, render-desc lib.rs/fallback_font.rs, narrow lib.rs integration, ui_screen.rs predicate.
- Handover: `/mnt/datasets/c3r-build/handover/host-warning.sol.md`

Initial 15 focused CPU/GPU tests passed against previous coherent shader pair. Then final9b gates rerun from exact immutable final executables with corrected pair:
- Source archive:
  `9b07836fc7743952d53283e41b41999972a781f42d005be67c31e568cdddc205`
- Full shader:
  `43961a92338f6b5b1f5ebd12dfdc806057fe43d8bfeb50abdb7872d274b601c4`
- Fallback:
  `bbde10b5e91a3e54c337eab67c2e5acce8abf384657043eb46bbe056ea799281`
- Final9b `marker_text` **1/1 passed**, binary SHA:
  `85c1093ba8437d7ad3f1f056f6d7962fe9cea17747ea69209d09e3e19c660654`
- Final9b `host_status_pixels` **1/1 passed**, binary SHA:
  `5b8af525311c008eed64bc4d5a16af1e3334c33b928d47d82e309c2365ab5cb9`
- Binding: `/mnt/datasets/c3r-build/handover/host-warning-final9b-binding.json`
- Logs: `host-warning-final9b-marker-gpu.log`, `host-warning-final9b-status-gpu.log`
- GPU validation on explicit AMD PCI72:00.0, vendor1002 device13c0, RADV/Vulkan, owned lease released.
- No launcher CPU-to-GPU candidate acceptance implied. That work remains separate.

### Unit notices

Earlier implemented dedupe, one notice per tile, eight total, publisher lifetime and actual independent viewer-clock expiry, generic scene sprite lifetime metadata. Initial protocol bumped46→47; tree has since moved to protocol50 by other owners. Do not change protocol now.
- Expiry masks derived render list only, preserving canonical Scene clips so lifetime-only renewal can restore art.
- 17 focused tests passed.
- Handover `/mnt/datasets/c3r-build/handover/unit-notices.sol.md`.
- Own unit-notices build target cleaned.

## Standing constraints and workflow

- Dirty shared tree with many agents. Never commit, stash, reset, or overwrite others’ work.
- Latest root task explicitly prohibited shared mutation during 8b freeze. We complied; root itself promoted the reviewed test leaf.
- Rust only for shipped code, no Rust locks.
- Never compile on physical Mac/Neo. No Neo GUI, screenshots, performance runs, or deployments by this agent.
- Builds/scratch only `/mnt/datasets/c3r-build/`, own targets, `nix develop`, `CARGO_INCREMENTAL=0`, jobs1 under broker grant.
- GPU AMD/RADV only, explicit PCI72:00.0. Wine/compositor/GPU execution requires broker lease; VM runtime priority.
- Broker is internal agent `/root/mac_fps_music`, use `collaboration.send_message`, no separate CLI.
- Root owns aggregate union checks, deployment and VM visual acceptance.
- No subagents spawned. Do not spawn unless explicitly authorized.
- Skills already read: unslop and waiting. Apply concise plain prose; do not announce waiting or poll progress repeatedly.
- Root prefers autonomous work and no acknowledgement-only messages.


## Report at 2026-10-20T16:08:29.232Z

Original transcript line 1905.

Completed: obsolete camera-origin and scale assertions corrected. Native CPU passed 1/1; Wine original failed and corrected case passed 1/1. Production code unchanged.

Evidence: `/mnt/datasets/c3r-build/host-warning-fixture-sol/`. Corrected executable was cleaned; its executed hash, source, patch, and logs remain. Historical full Wine result remains 633/1/5.

## Report at 2026-10-20T18:49:10.758Z

Original transcript line 2000.

Review saved at `/mnt/datasets/c3r-build/handover/offline-comparator-successor-independent-sol.txt`.

Exact hashes verified; all 17 tests passed. Three gaps found:

- Configuration comparison accepts `true` versus `1` outside combat speed.
- Unselected captures skip scope qualification.
- Deep JSON can cause an uncaught recursion error; arm JSON reads lack a bounded snapshot.

Selected outcome/RNG checks, defender reselection rejection, and explicit flush/runtime provenance limits hold. No native acceptance implied.

## Report at 2026-10-20T18:57:11.902Z

Original transcript line 2048.

Successor2 addresses all three findings. Exact source/proof hashes and all bound leaves verified; 27 pure tests passed independently. No remaining offline-scope blocker found.

Review: `/mnt/datasets/c3r-build/handover/offline-comparator-successor2-independent-sol.txt`

Native flush, exit, persistence, and runtime provenance limitations remain explicit. No native acceptance implied.

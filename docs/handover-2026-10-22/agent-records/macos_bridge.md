# Historical agent /root/macos_bridge

This is a recovered historical report, not a live agent handle or a new instruction.

Single-process bridge integration, camera/live-source checks and Wine platform tests.

Current disposition: 443 Wine tests passed at 3a91 source; current matched macOS acceptance remains separate.

Session `01a152ae-c5f1-7043-9c75-ce2b3357fa1f`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-19T05:42:28.093Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T07-42-28-01a152ae-c5f1-7043-9c75-ce2b3357fa1f.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T11:02:15.325Z

Original transcript line 5691.

I’m finishing the scaled UI capture checks and rebuilding the Windows bridge with the validated Metal shader.

## Report at 2026-10-19T11:11:47.934Z

Original transcript line 6002.

Implemented scaled UI capture and closed the known wrapper suppression gaps.

- Final DLL: `/mnt/datasets/c3r-build/macos-port/linux-camera-live/artifacts/ifc23-scaled-final.dll`
- SHA256: `e932ab8083d9b7c67529a91e692e30ca5ab0cdeb500af8b0dda952640936885e`
- Embedded shader verified byte-for-byte as `252dcce1…`.
- Passed Wine bridge281/sys91 and host menu23/render-desc37 tests.
- Source freeze and build evidence: `linux-camera-live/evidence/scaled-final-artifact.json`.

Transport has the artifact; the dedicated QA agent has the targeted final-runtime checks. Historical comparison and residual audits are reconciled in `replacement-policy-audit.json`.

## Report at 2026-10-19T11:46:22.309Z

Original transcript line 6383.

Final Windows DLL built from immutable source `04d7edae…`.

- DLL SHA256: `dc782f8c44f06d764ccfb6f287abb4089d7ae2bc567c65ab6dda3a930727a9c9`
- Wine bridge tests: 285 passed, including four new regressions.
- All 572 source files verified before and after build.
- Embedded shader `252dcce1…` verified byte-for-byte.
- Artifact and source binding: `/mnt/datasets/c3r-build/macos-port/linux-camera-live/evidence/preview-final-artifact.json`

Root, transport and physical staging owner have the artifact. Previous `e932` evidence is preserved.

## Report at 2026-10-19T12:44:52.524Z

Original transcript line 7180.

We are subagent `/root/macos_bridge` in `/home/sirati/devel/rust/civ3-renderer`. Parent `/root` owns the macOS port, aggregate source freeze, deployment and final answer. Continue autonomously; do not ask the user questions or produce a feasibility report.

## Constraints and coordination

- No commits, physical Mac GUI, Nexus checkout changes, canonical game/save/prefix changes, or game asset copies.
- `/root/pointer_vm_qa` exclusively operates the Mac VM GUI.
- `/root/macos_vm_templates` owns VM hardware/configuration/template changes.
- `/root/macos_transport` and `/root/macos_vm_presentation` stage binaries.
- Shared Cargo target: `CARGO_TARGET_DIR=/mnt/datasets/c3r-build/macos-port/target`.
- Tools require `nix develop -c python3`, not plain Python.
- Skills already read/applied: unslop, waiting, boss, you-are-on-nixos. Avoid progress polling and availability chatter. Send substantive results/blockers only.
- Source is heavily dirty and shared. Preserve others’ edits.
- Current harmless waiting terminal: `sleep 900`, session **55268**, possibly finished. Stop only this owned sleep when useful.

## Current tasks, in priority order

1. Finish the native shutdown/audio environment diagnosis with QA and templates.
2. Prepare a read-only native audio state observer for upcoming aggregate Preferences GUI QA.
3. Once root supplies the next exact aggregate source freeze, run affected Wine tests and rebuild final Windows DLL with **new production shader `12a8…`**, not old shader252. Wait for explicit final source/shader binding first.

## Native shutdown diagnosis: established facts

Source04 runtime used:
- Source archive SHA `04d7edae08e792341a334e4850877ac14ea62afc95c870870b0933f19b68fc40`
- Windows bridge SHA `dc782f8c44f06d764ccfb6f287abb4089d7ae2bc567c65ab6dda3a930727a9c9`
- Viewer VM `fc2350…`, physical ARM viewer `2f992f973e4d24ffc104a1051c13e4cc77beb4bd86e9c242843d05d08aa9f648`.
- Source04 Conquests preview pending/ready/return-to-title passed. Actual saved map Escape → “Yes, immediately!” → OK caused native process shutdown failure.

Important correction: this action **exits the process**, not returns to title. Disassembly proves WinMain `0x56BF10` returned, CRT at `0x64CA14` called exit, and `0x64A0CB` invoked an atexit callback.

Fault:
- EIP `0x005F8CBF`
- Read `0x00000013`
- EAX `0xFFFFFFFF`
- Instruction `jmp [eax+0x14]`
- Same CRT stack entries `0064A0CD`, `0064A05F`, `0064CA19`.
- Native base destructor `0x5F8CB0` reads backend object at `this+0x14`, then tail-calls its virtual destructor.
- Native sound adapter globals `0xCAAB00`, `0xCAAAD8`, `0xCAAD18` have atexit thunks leading here.
- `0x5F9C60` loads literal **`.\sound.dll`**, not jgl.dll.
- Native sound cleanup `0x5F9BB0`, module unload `0x5F9D00`.
- sound.dll absent from crash module table.
- Do not bypass CRT, ExitProcess, skip destructors, mask exit status, or blame renderer capture.

### Exact uninstrumented control reproduced the fault

Prepared private VM control:
`/Users/c3r/civ3r-runtime/control-exit-24b`

Launch script:
`/Users/c3r/civ3r-runtime/control-exit-24b/launch-control.sh`

- Same canonical C3X executable, SHA:
  `66d0ae91de6f4a2376e03aa74eb29a8256f02108ca3a4c024a4857337bc45ee6`
- Vendor original IFC23 copied as control IFC23.dll, SHA:
  `032c24f528881e25e24d0acf4571fd92e7976302e50474950c505a08ee490e4b`
- sound.dll SHA:
  `32da5ea027041a5ceda170f40be38ef63ec3815efdb3846f62d6e130d3ec668c`
- All game assets and protected saves linked, not copied.
- Same private Wine prefix:
  `/Users/c3r/civ3r-qa/wine-runtime/prefix-wine11`
- No Rust bridge, viewer or civ3r threads.
- Runtime vendor IFC23 module `10000000-1003B000`, 45 modules.
- Same protected `000-c3r-roundtrip` QA Rome map/worker and same Escape/Yes/OK action reproduced **the exact EIP/read13/EAXFFFFFFFF/CRT stack**, Wine exit5.

There was an earlier control startup FONT ERROR because `LSANS.fot` was a dangling link to the native wrapper’s removed generated resource. QA removed **only that owned control link**, letting the control regenerate its own FOT. `LSANS.TTF` remains linked to readonly assets. The font-failure teardown also faulted at `005F8CBD`, but it is preserved separately and is not the comparable workload.

Evidence base:
`/mnt/datasets/c3r-build/macos-port/macos-game-live/`
- `crash-24b-backtrace.txt` (instrumented)
- `wine-24b-game-exit-failed.log`
- `bridge-24b-game-exit-failed.log`
- `control-exit-24b-backtrace.txt` (**independently read/verified**)
- `control-exit-24b-font-error.log`
- `control-exit-24b-map.png`
- `control-exit-24b-details.png`
- `game-24b-exit-dialog-settled.png` explicitly says “Do you really want to quit?”

Our diagnosis:
`/mnt/datasets/c3r-build/macos-port/linux-camera-live/evidence/shutdown-24b/diagnosis.json`
Includes disassembly files, hashes and limits.
Preparation manifest:
`.../evidence/shutdown-24b-control-preparation.json`

### VM audio hardware correction is installed, control replay pending

Before:
`system_profiler SPAudioDataType -json` showed empty `_items`.
Root independently checked owned work QEMU had no audio device.

Templates owner performed coordinated clean stop/configure/start at **3072MiB**, adding persistent per-instance:
```
-audiodev none,id=macvm-audio
-device usb-audio,id=macvm-usb-audio,bus=xhci.0,audiodev=macvm-audio
```
No host audio access, other VM or source changes.

After:
- Guest sees QEMU default/system USB audio output.
- 2 channels, 48000Hz.
- Device name “Audio Output - Disabled”, null backend expected.

Evidence:
- `/mnt/datasets/macvm/evidence/audio-hardware/configuration.json`
- `/mnt/datasets/macvm/evidence/audio-hardware/guest-after.json`

We independently read these and linked them into diagnosis.json.

QA has been explicitly reactivated using `followup_task` to rerun the **same vendor control** after this hardware change, preserving all earlier failures. QA has the sole GUI lease. Await its result, then:
- If clean exit, notify root/templates. Templates must make proven audio configuration inherited by final templates/clones; current qemu_extra is only per-instance.
- Then QA must test aggregate runtime quit.
- If still fails, continue scoped Wine/audio environment investigation without production teardown bypass.

Root said source freeze can proceed independently if no production fix is needed.

## New read-only native audio observer task

Latest root instruction:
> For aggregate runtime QA after native Preferences hooks settle, prepare a bounded read-only audio-state observation alongside QA: prove draft changes leave live channels/volumes untouched, Cancel unchanged, and Accept commits once. VM audio is null-backed, so silence alone is not validation. Use checked native state/callback trace evidence within existing diagnostic policy, without production instrumentation edits unless essential. Coordinate sole GUI owner for input.

I sent root this plan, but **have not implemented the helper yet**:
- Separate i686 Windows console diagnostic, only `PROCESS_VM_READ|PROCESS_QUERY_INFORMATION` and `ReadProcessMemory`.
- No injection, writes, native calls or input.
- Short bounded sample series per QA phase plus existing production debug log.
- Use existing `probe-forms.c` as pattern:
  `/mnt/datasets/c3r-build/macos-port/linux-camera-live/probe-forms.c`
- Build with `nix develop -c i686-w64-mingw32-gcc … -Os -static -s`.
- Stage the small diagnostic in VM private QA directory, not canonical game.
- Can run console helper over SSH while QA owns GUI.
- Prefer explicit PID, or reject multiple `Civ3Conquests.exe` matches.
- Capture baseline, changed draft, after Cancel, second draft, Accept transition, after Accept.
- Natural sound nodes can change with playback. Channel enabled/paused/volume and persisted globals are the stable assertions; include node pointers/counts as supporting data.
- Combine distinct state transition count with existing debug log callback plan to prove one Accept application. No production instrumentation edits needed.

### Exact native fields recovered

`crates/civ3-sys/src/preferences.rs` now contains checked transactional hooks, owned by `/root/settings_transaction`. Do not edit it without coordinating.

Preferences:
- Form `0xC72888`
- Vtable `0x66E628`
- Visibility flag `form+0xA0 & 1`
- Staged effects enabled byte `+0xEC5D`
- Staged music enabled byte `+0xEC5E`
- Original effects byte `+0xEC5F`
- Original music byte `+0xEC60`
- Accepted byte `+0xEC61`
- Staged master f64 `+0x15C98`
- Staged effects f64 `+0x15CA0`
- Staged music f64 `+0x15CA8`
- Master ratio f64 `+0x15C90`
- Master maximum u32 `+0x14E40`
- Original master u32 `+0x15C84`
- Original effects u32 `+0x15C7C`
- Original music u32 `+0x15C80`

Persisted globals:
- master u32 `0xA52BF8`
- effects u32 `0xA52BFC`
- music u32 `0xA52C00`
- preference flags `0xA52638`, effects bit4 and music bit8.

Sound manager:
- Global `0xCAAB00`, normal derived vtable `0x670A00`
- Backend pointer manager+`0x14`
- Sound initialized u32 `0xCAAD40`
- sound.dll module handle `0xCAAD3C`
- 16 channel records at manager+`0x24 + channel*0x1C`
- channel0 music, channel1 effects
- Record fields:
  - enabled u8 `+0`
  - paused u8 `+1`
  - volume u32 `+4`
  - list vtable `+8`
  - head u32 `+0xC`
  - tail u32 `+0x10`
  - iteration/current u32 `+0x14`
  - node count u32 `+0x18`
- Confirmed native volume setter `0x5F9230` writes manager+channel*28+`0x28`, traverses nodes.
- `0x5F92B0` enables channel, `0x5F93B0` stops nodes and disables.

Existing production log in preferences.rs:
```
Preferences native Accept: audio callbacks [Effects(...), Music(...), Volumes(...)] before original closure
Preferences native Cancel: audio callbacks [] before original closure
```
Close hook calls each planned changed callback once, then original closure. Only checked Preferences methods/call sites are patched, no global sound methods.

Settings owner added Cancel gates at:
- volume `0x58820A`, `0x588246`
- effects enable/disable `0x588270`, `0x588296`
Independent pointer review caught the latter two; they are now present.

## Pending aggregate rebuild

Root’s latest instruction:
> Aggregate source review/freezing is next, pending producer coverage and control outcome. Need final DLL built only against exact aggregate immutable source and shader12a8.

Do **not** build while agents edit. Wait for root’s explicit final archive/hash and full shader binding.
- Old production shader252 is now historical:
  `/mnt/datasets/c3r-build/macos-port/tsneo/candidates/05-wide-unrolled/shader.spv`
  SHA `252dcce1c11c3a5179fcdec99bdfda25a8102736e2bc4dfbf4336671bbbc2805`
- New shader starts `12a8…`; full hash/path not yet provided to us.
- Correct override is `TILE_RENDER_PRECOMPILED_SPV`.

Likely tests after freeze:
- Wine bridge + sys libraries, using isolated Linux prefix:
  `CIV3R_WINEPREFIX=/mnt/datasets/c3r-build/macos-port/linux-camera-live/prefix`
- Affected tile-render camera unit tests, e.g. `cargo test -p tile-render --lib camera:: --target i686-pc-windows-gnu`.
- No unnecessary full-game replay loops.
- Parent/Metal owner owns native shader/reference testing.

New aggregate changes by other owners:
- Camera bounds optimization intersects only nonwrapped grid axes, preserving overhang/KEEP_ANCHOR/wrapped copies. Preview previously submitted135,135 cells/layer for70×142 grid, ~13.6× waste. `/root/ui_feature_audit` owns implementation.
- Settings draft/Accept/Cancel + dirty confirmation, native audio deferred.
- Menu checkbox/25–100% sizing, forced title/Preferences fit, protocol **38**.
- Background-only dithering and metadata. `/root/ui_background_dither` owns shared/render-desc and narrow producer regression in ui_screen.rs.
  - I explicitly released ui.rs/ui_screen.rs to it; no current edits here.
  - It reported27 native PopUpAndMenuBackground crops tagged, including repaint row fragments; excludes mixed/control art and broad dimensions.
- No producer shutdown source changes from us.

### Immutable build verification method used previously

Read source tar with Python `tarfile`, exclude metadata member `SOURCE-MANIFEST.json`, hash every regular member and compare current workspace **before and after build**. Source04 had572 source files +1 manifest. Next count may change, do not hardcode572.
Then:
```
env CARGO_TARGET_DIR=/mnt/datasets/c3r-build/macos-port/target \
    TILE_RENDER_PRECOMPILED_SPV=<exact final shader> \
    nix develop -c cargo build --locked --release \
    -p civ3-bridge --lib --target i686-pc-windows-gnu
```
Copy release DLL to immutable directory named full source hash, chmod444.
Verify full SPIR-V bytes occur exactly once in DLL using Python `.find()`/`.count()`.
Record DLL SHA/size, full source archive SHA, before/after source verification, shader SHA/size/offset/count, test logs/hash.
Send exact artifact/manifest to root, transport, presentation.

## Historical completed implementation and artifacts

Known scaled-wrapper closure completed and reviewed:
- Hooks five known wrappers8060/83E0/8560/86A0/8870.
- Static audit70 direct executable callers, not universal unknown indirect/mod coverage.
- Scaled file UI icons preserve native cropped geometry, f32/x87 extents, exact16.16 sampling, palette/mask/key/opacity, copies/clipping/sample phase.
- Lazy8060 file-load path ignores crop/scale, retains source load; nested hooked PCX write remains suppressed.
- NO_MOUSE and rectangle-only keyed forms no longer retain native masks.
- Generated-surface resizing unsupported; no audited required caller needs it.
- Strict default suppresses original pixels, no silent fallback; comparison/compat explicit.
- Comparison original camera uses full native viewport centered on renderer A.

Audit paths:
`/mnt/datasets/c3r-build/macos-port/scaled-wrapper-audit/report.json`
`.../linux-camera-live/evidence/replacement-policy-audit.json`
`.../evidence/run23-bridge-review.json`

e932 historical DLL:
`.../linux-camera-live/artifacts/ifc23-scaled-final.dll`
SHA `e932ab8083d9b7c67529a91e692e30ca5ab0cdeb500af8b0dda952640936885e`
- Wine bridge281/sys91, host menu23/render-desc37.
- Run23 actual title, map Z, Pedia/HUD/Alt inspected.
- title/UI/present originals0, unknown0.
- scaled fog378 skipped/0 original.
- Remaining map originals only Main_GUI hitmask5, navigator sources2.
- `scaled_a4/colored` not naturally reached with parked5×5 camera and protected worker elsewhere.
- Stock PediaIcons explicit32/128 art bypasses nonidentity fallback, resized_capture.file0 expected.
- Valid title screenshot `title-23-unlocked.png`; earlier all-black lock-screen captures excluded.

Source04 final DLL, now historical:
`/mnt/datasets/c3r-build/macos-port/linux-camera-live/artifacts/04d7edae08e792341a334e4850877ac14ea62afc95c870870b0933f19b68fc40/ifc23.dll`
SHA `dc782f8c44f06d764ccfb6f287abb4089d7ae2bc567c65ab6dda3a930727a9c9`
30,682,305 bytes.
Shader252 embedded exactly once at offset17,277,458.
Wine bridge285 passed, including4 new preview regressions:
- `art_schedule::tests::a_replacement_whole_atlas_keeps_missing_screen_art_pending`
- `preview::tests::a_pending_or_failed_selection_cannot_keep_an_older_preview_or_its_art`
- `preview::tests::closing_the_scenario_menu_cancels_work_and_rejects_late_stages`
- `preview::tests::optional_preview_art_can_remain_unavailable_while_the_menu_is_complete`

Provenance:
`.../evidence/preview-final-artifact.json` (immutable444)
`.../evidence/preview-final-source-freeze.json`
`.../evidence/preview-final-wine-tests.log`
`.../evidence/preview-final-release-build.log`

Root source04 archive:
`/mnt/datasets/c3r-build/macos-port/staging/source-history/04d7edae08e792341a334e4850877ac14ea62afc95c870870b0933f19b68fc40/source.tar.gz`

Physical source04 installation proof:
`/mnt/datasets/c3r-build/macos-port/tsneo/evidence/release-04d7edae08e79234/owner-report.json`
Both viewer+DLL installed by owner after no-process check; no physical GUI actions by us.

## Immediate next actions

1. Implement bounded external audio read observer now, without production edits; stage for aggregate QA and provide phase recipe to root/QA.
2. Collect QA vendor-control result after USB audio change. Read/verify evidence, update shutdown diagnosis, notify root/templates. Coordinate inheritance only if proven.
3. Await explicit root aggregate archive + shader12a8 full binding, then build/test/freeze final DLL and send provenance.
4. Parent owns aggregate GUI acceptance/deployment/final answer. Do not claim historical tests validate newly changed artifacts.

## Report at 2026-10-19T12:44:58.772Z

Original transcript line 7195.

I’m continuing the macOS implementation and validation from the recovered state, including the native audio checks and final Windows bridge build.

## Report at 2026-10-19T12:59:11.153Z

Original transcript line 7494.

The VM shutdown crash is fixed. The missing audio device caused the original game to fail during exit; with the corrected VM hardware, the same game exits cleanly. The combined renderer build still needs its final runtime checks.

## Report at 2026-10-19T13:17:34.000Z

Original transcript line 7692.

Native shutdown is fixed by the VM’s missing USB audio device. The identical vendor control now exits 0; evidence is in `linux-camera-live/evidence/shutdown-24b/diagnosis.json`.

The read-only audio observer is built, staged and validated. Aggregate Preferences checks await the final runtime.

No production source edits made. Final DLL tests/build require root’s exact source archive and shader binding.

## Report at 2026-10-19T13:36:56.089Z

Original transcript line 7947.

Final DLL: [ifc23.dll](/mnt/datasets/c3r-build/macos-port/linux-camera-live/artifacts/3a91e2e9dcc38bb3584ec24c6b16eae4f0bf74fcb9a56a943c7f89fed47b8c0f/ifc23.dll)

SHA256 `d485254cba4e5fd3b3b5b5cad7c44cf9cdcba35153d45032e9a1d74507e2db54`.

All 443 Wine tests passed, including both GPU regressions without skips. All 588 source files match source3a91. Shader12a8 occurs exactly once.

[Provenance and test evidence](/mnt/datasets/c3r-build/macos-port/linux-camera-live/evidence/aggregate-final-artifact.json). Paired macOS runtime QA remains pending.

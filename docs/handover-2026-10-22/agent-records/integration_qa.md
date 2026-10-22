# Historical agent /root/integration_qa

This is a recovered historical report, not a live agent handle or a new instruction.

Combined control/camera/notice/boundary plans, artifact verifier and signed-artifact launch gate.

Current disposition: Preparation passed; final recorded report did not execute the combined runtime matrix.

Session `01a14bc1-65a9-72b3-b946-0d4f229548a7`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-17T21:25:28.107Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-25-28-01a14bc1-65a9-72b3-b946-0d4f229548a7.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T01:39:19.754Z

Original transcript line 2583.

## Role and task

You are `/root/integration_qa`, a Sol UI-testing subagent reporting to `/root`. Continue autonomously, no questions. Root wants final QA on exact signed source:

- SHA: `22cd2e4ece069f569ecc485ccbc91c04ffd824c5`
- Tree: `330bfe675e7d9ec56f04f8f36f6a0b5d5531e696`
- Checkout: `/tmp/c3r-zoom`
- Build: `/mnt/datasets/c3r-build/zoom`
- Immutable runtime: `/mnt/datasets/c3r-artifacts/continuation-20261018/runtime-22cd2e4ece06`

Root explicitly released artifacts and functional GPU. Do not build or edit product source. No benchmark timings.

Root/boss requests no acknowledgements or routine progress messages. Report concrete blockers immediately and provide the final evidence handoff.

## Constraints

- AMD only, never NVIDIA. Laptops offline, never connect.
- Private sway/Wine prefixes and ephemeral VM.
- All QA work under `/mnt/datasets`.
- Do not affect other agents’ sessions.
- Preserve exact artifact hashes and new/changed saves, including Windows autosaves/config/high scores/logs, before disposal.
- Stop every owned Wine, sway, input, helper and VM process before root’s benchmark GO.
- Preserve explicit disposable resource inventory at handoff.
- No huge or uncompressed video.
- Skills already read: unslop, waiting, you-are-on-nixos, boss.
- Do not edit a helper while an active script is reading it. One earlier QA-only transient parse error resulted from this; corrected and rerun.

## Current QA root and helpers

`/mnt/datasets/c3r-build/continuation/qa-22cd2e4`

Important scripts:

- `go`: exports GPU release and exact22 SHA/target, invokes `run-22`.
- `qa.sh`: mode defaults native; use `QA_MODE=ip` for IP.
- `phase-22 player|scenario`: requires `QA_GPU_RELEASED=1`, optionally `QA_MODE=ip`; records absolute Unix and monotonic action timings.
- `contact.py OUTPUT INPUT...`: creates labelled image contact sheet.
- `audit-owned.py evidence/relative-output.json`: process/socket/runtime/VM inventory.
- `vm-setup.ps1`, `vm-start.ps1`, `vm-show.ps1`
- `vm-paired-player.ps1`
- `vm-preserve.ps1`

Cached Python with PIL:

`/nix/store/5ci4vg386iwvswz8fn8fxc102jzka7qf-python3-3.14.7-env/bin/python3`

Nix dev shell provides rg/compilers if necessary.

Generic commands:

```bash
QA=/mnt/datasets/c3r-build/continuation/qa-22cd2e4/go
"$QA" shot TAG
"$QA" input click X Y
"$QA" input key KEY
"$QA" collect TAG
"$QA" down TAG

"$QA" vm-shot TAG
"$QA" vm click X Y
"$QA" vm move X Y
"$QA" vm key KEY
"$QA" vm type TEXT
"$QA" vm ps <<'PS'
# PowerShell
PS
"$QA" vm-show game|viewer
"$QA" vm-collect
"$QA" vm pull GUEST_PATH HOST_PATH
"$QA" vm-down
```

VM controller is `/home/sirati/winvm/bin/winvm`; VM name **`c3r-22-qa`**. Guest QA directory **`C:\c3r-22-qa`**.

## Hashes, installed and verified

- Native SO: `5edf337b0bfab8860b9d95efd528d7302fa86a574fa12329ebddf013286640d1`
- Bridge DLL: `3971b99d146ab216d1e74b500b01fe1c40f0b2b194e9d4ca532e8f08b6000910`
- Raw native PE: `4bd44902371c885c2ff6f60ac8f03526854435ebc1c24ec19a6e2a2d6d714adc`
- Marked installed native PE: `85318c94608b506256e87f3fedaa8f978e0a3ef0ca420e54381eb5c49214f726`

Evidence:

- `evidence/build-manifest.txt`
- `evidence/installed-sha256.txt`
- VM `setup.log`, `start.log`, `baseline.log`; guest installed bridge hash matches.

## Completed exact22 live QA

All native/IP Wine/sway sessions are currently **stopped**. Only the Windows VM remains active.

### Native1080 and IP1080

Both passed actual inspected:

- Passive title image/text.
- World Setup → Player Setup: first sample retains complete World, subsequent full Player Setup; no partial black scene or diagnostic.
- Dawn → Rome founding → city screen → F1.
- Science prompt and turn4000BC →3950BC.
- Settled Preferences CivIII, Resolution, Layout tabs.
- Retirement through Summary Replay/HoF and clean returned title.
- Replay lower raw FogOfWar artifact absent.
- Native genuine Cursor.flc ring remains, visible clipped along bottom of HoF.

Evidence under `evidence/native` and `evidence/ip`:

- `setup-sheet.png`
- `gameplay-sheet.png`
- `preferences-sheet.png`
- `*-retire-replay.png`
- `native/retire-sheet.png`
- `native/native-hof-attempt.png` is actual HoF.
- `native/native-hof-trace.txt`: `ui_flc 466 1061 ... frame9 ... Cursor.flc`
- `ip/ip-hof-attempt.png` is actual HoF.
- `*-title-after-retire.png`

Native final log: `native/native1080-final-bridge.log`, no warnings/errors.
IP gameplay log: `ip/ip-final-bridge.log`; only expected failed Dx12 backend probes before RADV Vulkan selection.

### Scenario feedback, both modes

Both have verified actual WWII Pacific preview and alternating orange OK/Cancel hover with visible cursor during preparation:

- `scenario-hover-controls.png`
- `scenario-preparation-{before,0..7}.png`
- `scenario-preparation-events.json`
- `scenario-timing-alignment.json`
- `*-preview-pacific.png`

Native:
- Whole CPU build977ms, GPU preparation821ms/44frames.
- Samples at bridge247.778,248.168,248.579,248.969s overlap pending replacement; GPU completed249.179s.

IP:
- Separate clean startup used for final scenario test.
- `ip/ip-scenario-final-bridge.log` is this run.
- Actual initial selection was Rise of Rome; changed to Pacific.
- Final screenshots/contact controls inspected successfully.

An invalid first IP scenario attempt is retained in:
`evidence/ip/invalid-scenario-attempt`.
It was still on title and then a QA helper write caused a transient shell parse error. No product defect. Reran correctly in a fresh IP session. Do not use invalid attempt as a pass.

### Native4K, clean independent lifecycle passed

- Physical3840×2160, compositor/game/session scale2.
- Default original drawing OFF.
- **Passive title complete before any input, focus helper or repaint.** This fixes source68’s title omission.
- Auto1920×1080, x13840×2160, x31280×720, Auto restored.
- Actual settled Layout, World Setup, Player Setup, Dawn, world, Rome city and F1 inspected.
- No warnings/errors in final bridge log.
- Launch helper exit0, deliberate shutdown helper exit0, no unexplained143.
- Game process PID1823228 was alive at end of actions; owned process inventory recorded before shutdown.
- All owned sessions/socket/runtime then absent.

Evidence:

- `native/native4k-passive-title.png`
- `native/native4k-auto.png`
- `native/native4k-x1.png`
- `native/native4k-x3.png`
- `native/native4k-layout.png`
- `native/native4k-scales-sheet.png`
- `native/native4k-gameplay-sheet.png`
- `native/native4k-final-bridge.log`
- `native/native4k-lifecycle.txt`
- `native/native4k-launch.txt`
- `native/native4k-stop.txt`
- `native/native4k-before-shutdown.json`
- `native/native4k-stopped-state.json`, clean at03:30:28.

Private wlh now records detached launcher PID in `run.pid`; collect preserves run/sway/wayvnc/input PID files.

## ACTIVE work: Windows VM DEFAULT preload and paired Player Setup

VM `c3r-22-qa`,4GiB,1280×800 is running. Microsoft Basic Render Driver/Dx12 software rendering. Host display AMD only.

Guest task names:

- `c3r22Qa`
- `c3r22QaShowgame`
- `c3r22QaShowviewer`
- `c3r22QaPairPlayer`

Launch command explicitly clears both preload overrides:

```
set "CIV3R_PRELOAD="
set "CIV3R_PRELOAD_MIB="
```

Preserved in `evidence/vm/setup.log` and guest `launch-command.txt`.
Original game drawing stays enabled and window available for paired reference.
**Never substitute preload-off as final proof.**

### VM startup and paired capture status

Startup title and actual World Setup passed. Installed bridge hash matches3971…0910.

First paired attempt:
- Original Player Setup was complete.
- Immediate foreground check for viewer transiently returned blank.
- Correctly aborted before claiming viewer captures.
- Preserved host:
  - `pair-player-first-attempt.zip`
  - `paired-first-attempt/`
  - `pair1-bridge.log`
- This was a QA foreground timing issue.
- Fixed only QA script `Show-Target` by retrying foreground checks every25ms up to20 attempts.
- Restarted guest game with same exact bridge and default preload, without reboot.
- First restarted “World Setup” screenshot was still title generation progress. Did **not** use it as World proof.
- Later actual World Setup was inspected at `vm-world-ready.png`.
- Standalone `vm-show` logs can be immediately blank due async activation. Paired task now waits and validates actual titles.

Second paired attempt **completed successfully** with actual foreground logs for every capture:

Host files:

- `evidence/vm/pair-player.zip`
- extracted `evidence/vm/paired/`
- `evidence/vm/player-pair-sheet.png`
- current `evidence/vm/bridge.log`, collected immediately after task.

Absolute timing in `paired/pair-player-events.json` is UTF-16 JSON.

Important events:

- World OK released at task1120.819ms, epoch1792287415674.836ms.
- Original fast Player capture at1229.011ms, actual foreground original.
- Viewer foreground verified at1389.459ms.
- Viewer captures at1485.842,1739.853,1977.191,2229.838,2482.407,2719.493,2956.803,3194.058ms.
- All viewer captures have actual foreground `"Civ III (reimplemented renderer)"`.
- Original later capture3434.856ms, foreground verified.
- Ends with viewer foreground verified3589.887ms.

**Last action before compaction:** inspected `player-pair-sheet.png`.

It shows:

- Original fast Player Setup **complete**.
- Viewer samples0,2,7 all retain **complete World Setup**, without partial Player Setup.
- Viewer sample7 was approximately2.073s after World OK.
- This is valid pending-art completeness proof, but **eventual Player adoption has not yet been checked**.
- Do not classify it as a stall yet. Software GPU growth in first attempt took roughly885ms +408ms plus scheduling gaps for split mandatory growth; current frames may simply still have been preparing.
- Current game state is Player Setup in original. Current foreground viewer.
- Next check must capture current viewer and inspect latest logs to confirm it now reaches full Player Setup, then proceed beyond Dawn and through current gameplay.

The most recent paired images were viewed; no partial black Player Setup occurred.

## Immediate next steps

1. Capture current VM viewer, inspect actual screen. Confirm settled complete Player Setup. Collect/log timings around required growth and adoption, align to paired epochs.
2. If still World, inspect current bridge and actual original/viewer state before classifying default preload regression. Report established material failure to root immediately if confirmed. No source edits.
3. Continue default-preload VM:
   - Player OK around1096,755.
   - Wait for world generation, inspect actual Dawn position.
   - Close Dawn using actual screenshot.
   - Found Rome with B, inspect naming popup, Enter.
   - Verify actual settled city, F1, end turn and3950BC after Science prompt.
   - Verify viewer follows current state, not just original.
   - Preferences/FPS smoke if practical, scenario optional per final plan but root explicitly requires city/turn/F1.
   - Preserve actual foreground logs, installed hash and default-preload backlog/ack logs.
4. Before VM destruction, **preserve all changed/new guest saves/autosaves/config/high scores/logs with hashes**.
5. Stop VM and verify all owned processes/runtime/socket gone.
6. Preserve native/IP changed saves/metadata and final logs, verify hashes.
7. Write complete exact22 screenshot-informed report and disposable resource inventory, send final handoff to root. No benchmark GO from us.

## VM coordinates and input lessons

At1280×800:

- New Game390,358.
- World/Player OK1096,755; Cancel1126,755.
- City X1101,54.
- F1 X1118,758.
- Game Menu36,26.
- Main menu dialog Preferences520,251; Retire525,277; OK771,363.
- Preferences centered128,16:
  - Resolution350,72.
  - Layout460,72.
  - FPS226,512.
- Dawn close varies; inspect actual screenshot.
- Science prompt normally shifted from native by(-320,-140), so native950,299 becomes630,159; inspect.
- Hover before clicking. New title menu sometimes needs a second click.
- `winvm key` uses QEMU names (`ret`, `spc`, `f1`, chord `ctrl-s` etc.), **not wlh’s `enter`/`space` names**. Consult `/home/sirati/winvm/bin/winvm` if needed.
- Always inspect actual current image before using a geometry-sensitive script.
- Do not put `view_image` and dependent UI mutations into the same `functions.exec` call. You cannot assess the image until that call returns.

## Preservation tooling already prepared

`vm-preserve.ps1` was pushed to guest before initial launch.

Initial `-Baseline` ran before game and hashed1080 files. Guest:

- `C:\c3r-22-qa\baseline-files.json`

Final invocation, **after stopping guest Civ3 so files/logs are stable**:

```powershell
& C:\c3r-22-qa\vm-preserve.ps1
```

It:

- Hashes all `.sav` under Conquests\Saves recursively.
- Compares against baseline by relative path and SHA256.
- Copies new/changed saves and always config/high scores/log into:
  `C:\c3r-22-qa\preserved`
- Copies settings.toml.
- Verifies copied hashes.
- Writes:
  - `final-files.json`
  - `preserved-files.json`
  - `preserved-files.zip`
- Prints zip hash.

Pull zip and manifests **before vm-down**. JSON written by Windows PowerShell is UTF-16.

Also archive guest QA evidence before destruction:

- all paired PNGs/events/results,
- show.log,
- launch-command.txt,
- settings,
- task scripts,
- baseline/final/preserved JSONs,
- guest bridge logs.

Can make separate `qa-evidence.zip` with top-level `.png/.json/.log/.txt/.ps1`, excluding DLL/zip/self recursion. Pair files already on host but retain final copies.

Root specifically requires changed Windows saves including autosaves preserved. Do not destroy VM first.

For native/IP, compare prefix Conquests\Saves to baseline same-relative file. Efficiently skip identical size+mtime, then hash candidates and copy changed bytes into evidence/preserved-native or preserved-ip. Always preserve `conquests.ini`, `HighScores.cv3`. Prior68 run preserved seven files this way. No22 host saves have been copied yet.

## Disposable resources to inventory at final

Owned active-root resources only:

- `/mnt/datasets/c3r-build/continuation/qa-22cd2e4/prefix-native`
- `/mnt/datasets/c3r-build/continuation/qa-22cd2e4/prefix-ip`
- `/mnt/datasets/c3r-build/continuation/qa-22cd2e4/native`
- `runtime`, `tmp`, any owned Wine server directories inside QA tmp
- Private socket `wayland-wlh-22-qa`, VNC5948
- VM instance `/mnt/datasets/winvm/instances/c3r-22-qa`, destroyed at finish

Do not delete evidence, scripts, source checkout or shared build/runtime archives. Root assigned retirement agent CPU-only audit of OLD qa/qa-revised/qa-68 prefixes; they do not touch active22.

## Historical context, not final proof

-347 had partial Player Setup, default-preload VM stuck Dawn, raw replay FogOfWar.
-584 introduced false missing-art diagnostics and missing menu text.
-68 fixed normal text/readiness but native4K passive title background disappeared.
-22 fixes early UI capture before first jgl factory, retaining capture model and latching failed normal capture. Root628workspace +2 explicit GPU +i686/releases passed.
- All older evidence preserved separately; do not present as final22 proof.


## Report at 2026-10-18T01:39:22.691Z

Original transcript line 2595.

I’ll resume the remaining Windows checks on the recovered build, then preserve the saves and logs before shutting down the test sessions.

## Report at 2026-10-18T01:40:00.797Z

Original transcript line 2652.

The Windows viewer has reached the full Player Setup and Dawn screens with default preloading enabled. I’ll now check city founding, F1, and turn advancement to confirm it continues following the game.

## Report at 2026-10-18T01:41:52.421Z

Original transcript line 2764.

The Windows viewer shows Rome’s city screen, the Domestic Advisor, and the new turn at 3950 BC. The required gameplay checks passed. I’ll finish with a Preferences check, then archive the changed saves and shut down the VM.

## Report at 2026-10-18T01:44:07.247Z

Original transcript line 2882.

Preferences, Resolution, Layout, and the FPS display control have passed visual checks. I’m preserving the Windows autosaves and test evidence now. I’ll verify the VM and all owned display sessions are stopped before handing control back for benchmarks.

## Report at 2026-10-18T01:48:36.008Z

Original transcript line 3007.

The save and evidence archives have passed hash verification. The VM is destroyed, and the final audit shows no owned Wine, display, input, or VM processes and no listeners on the test ports. The build hashes still match.

## Report at 2026-10-18T01:49:07.504Z

Original transcript line 3023.

Final `22cd2e4` functional QA passed.

- Native/IP gameplay, preparation, Preferences and retirement passed.
- Independent native 4K lifecycle passed with exit 0.
- Windows default preload reached complete Player Setup, Rome, F1 and 3950 BC.
- Saves, metadata and logs preserved; 1,392 evidence files hashed.
- All owned sessions stopped; VM destroyed. GPU released for root’s benchmarks.

[Full report](/mnt/datasets/c3r-build/continuation/qa-22cd2e4/evidence/FINAL-QA-REPORT.txt) · [Shutdown proof](/mnt/datasets/c3r-build/continuation/qa-22cd2e4/evidence/final-stopped-state.json) · [Disposable inventory](/mnt/datasets/c3r-build/continuation/qa-22cd2e4/evidence/DISPOSABLE-RESOURCES.json)

## Report at 2026-10-18T14:41:27.799Z

Original transcript line 3257.

Preparation is ready. [PLAN.md](/mnt/datasets/c3r-build/qa-current-fixes/PLAN.md) includes all requested controls, camera, notices and boundary checks.

- Syntax, ShellCheck, artifact-verifier controls and launch gate pass.
- Startup captures will preserve native form trees and mirrored UI traces.
- Install/start now require `QA_ARTIFACT_MANIFEST` matching the signed SHA and all three raw binaries.
- No Wine, GPU, VM or laptop session launched.
- VM ownership coordinated with `vm_qa`.

[Readiness evidence](/mnt/datasets/c3r-build/qa-current-fixes/evidence/readiness/readiness.json). Final launches remain held for the signed source, artifacts and GO.

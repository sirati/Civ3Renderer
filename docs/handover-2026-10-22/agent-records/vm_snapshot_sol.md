# Historical agent /root/vm_snapshot_sol

This is a recovered historical report, not a live agent handle or a new instruction.

macOS snapshot/restore/GUI ownership, no-device causal controls and restoration; later runnable loaded-audio owner driver ce5e104.

Current disposition: Template/restoration closed within scope. Loaded no-device arm incomplete and latest driver has three review blockers.

Session `01a15823-d9f0-7f10-8504-f0c6111c3773`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-20T07:08:26.999Z. Original transcript `/home/sirati/.codex/sessions/2026/10/20/rollout-2026-10-20T09-08-26-01a15823-d9f0-7f10-8504-f0c6111c3773.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-20T18:09:20.121Z

Original transcript line 9986.

Healthy f7b title and Mal3 loaded-game Quit both passed with app exit0. Both traces showed initialized1/count0, so failed-unwind acceptance remains unexercised.

Final f510 launcher cancellation arms both passed normal143, with launcher-only TERM and normal child/lock/session cleanup.

Evidence and verdicts are in `/mnt/datasets/c3r-build/vm-audio-production-sol`. Config is restored, final f510 payload remains, USB-null is unchanged, and runtime lease is released.

No-device plan and corrected preflight await root review. No hardware change occurred.

## Report at 2026-10-20T18:29:18.794Z

Original transcript line 10386.

USB-null restoration passed, including exact fixture hashes, unchanged RAM/cgroup, no owned survivors and fresh afplay0.

The no-device attempt is invalid setup: vendor helper exited1 before launching the game. No production arm ran.

Preserved evidence and the audited correction proposal are under `/mnt/datasets/c3r-build/vm-audio-production-sol/`. Retry awaits root review; runtime lease is released.

## Report at 2026-10-20T18:32:51.931Z

Original transcript line 10455.

Corrected package is ready in `nodevice-retry-final-root-review`.

The watchdog preserves test roles, samples exit state after the grace period, and handles log truncation/replacement. The preflight pins its new hash and requires the old vendor log archived.

All validation passed. No hardware change or retry occurred.

## Report at 2026-10-20T19:16:05.024Z

Original transcript line 11246.

USB-null restoration is complete and verified, with afplay0 and no owned survivors.

Vendor reproduction and causal title acceptance passed. Loaded Mal3 remains incomplete at480seconds.

All evidence is preserved. New driver preparation is stopped under the broker freeze; no further edits or runtime actions occurred.

## Report at 2026-10-20T19:36:34.200Z

Original transcript line 11319.

I’ll prepare the driver locally with a timestamped action journal and visible-state checks. It will retain the 480-second cap and reserve the final 60 seconds for evidence and normal Quit.


## Report at 2026-10-20T19:38:57.525Z

Original transcript line 11327.

## Identity and current task

You are `/root/vm_snapshot_sol`, the sole Intel macOS VM GUI/runtime owner, subagent of `/root`.

**Active task is local source-only preparation of a deterministic loaded no-device QA driver and action journal. No VM operations, runtime hashing, captures, launches, profile changes, or builds are authorized now.** Broker is running shipping AMD timing. Root will review exact driver/harness source and hashes before execution.

Latest root instruction:
> Resume local source-only bounded noaudio driver/journal preparation per accepted plan aedda67a. Preserve480cap/>=60s finalreserve/freshloaded oracle/fullHUD+units/normalQuit. Source/test driver changes need root review. Do not require unit records from parked map trace; actual map/player/GUI identity plus fresh native audio oracle establish loaded context.

Root backup freeze was released. New persistent user policy requires **source commits and off-host backups before cleanup**. Never delete backups. Main repo committed `8106ae5`; private QA committed `6162a7d`, backing ~55k files onto both laptops. Do not edit main repo or perform cleanup here.

Last actions:
- Sent commentary: “I’ll prepare the driver locally with a timestamped action journal and visible-state checks. It will retain the 480-second cap and reserve the final 60 seconds for evidence and normal Quit.”
- Created empty directory:
  **`/mnt/datasets/c3r-build/quit-audio-sol/loaded-owner-driver-sol`**
- **No driver code or recipe has been written yet.** The detailed design below is proposed thinking, not implemented or approved.

## Skills / communication

Already read/applied:
- `/home/sirati/.codex/skills/unslop/SKILL.md`
- `/home/sirati/.codex/skills/waiting/SKILL.md`

Do not spawn agents. Existing agents can be messaged directly.
Use normal readable sentences with spaces; root specifically corrected prior compressed messages.
Avoid repeated progress polling/commentary. Root often says “Do not acknowledge; report blocker or completed evidence.”
Tools are accessed via `functions.exec`, e.g. `tools.exec_command`, `tools.write_stdin`, `tools.view_image`, `tools.clock__curr_time`. Collaboration tools called directly, not inside exec.
Host has no `python3` on PATH. Use:
- general Python: `/nix/store/0r6k8xa2kgqyp3r4v2w7yrb80ma2iawm-python3-3.13.12/bin/python3`
- Pillow Python: `/nix/store/lb41b0anx1f98y9y5s9mdv97gjgsq740-python3-3.14.7/bin/python3`
Avoid shell command success hidden by later successful commands. Check each subprocess exit and actual JSON `passed == true`.

## VM and constraints

VM `work`; root `/Users/c3r/civ3r-qa-b41-sol`.
Manager `/mnt/datasets/macvm/bin/macos-vm`.
SSH port22390, VNC5909. Current broker prohibits **all QMP** for these phases. Never pause/reset/force power-off.
No Neo operations, builds, FPS tests, RAM or unrelated profile changes.
RAM3072MiB,8vCPU. Cgroup parent actual page-rounded limits:
- memory.max `9999998976`
- memory.high `8999997440`
- memory.swap.max `0`

VNC:
`/mnt/datasets/c3r-build/macos-port/vm/helpers/vnc`
supports `-t 10 capture FILE`, `move X Y pause 0.35 mousedown 1 pause 0.2 mouseup 1`, `key esc`.
Immediate click sometimes merely focuses/hovers. Held click with delay is proven.
Do not use `spc` or `super-w`; they previously hung.
Original assets read-only SMB at `/Users/c3r/civ3-assets`. Never copy original game assets.
After each normal reboot remount with `/mnt/datasets/macvm/assets/mount-assets work`, checked exit0 and mount/hash/write-denial proof.
Aqua unlock only after confirming `ioreg ... IOConsoleLocked Yes`; private helper:
`/mnt/datasets/c3r-build/vm-qa3be-sol/unlock-private.py`.
Use credentials only in memory, never print.
Aqua launch context:
`sudo -n launchctl asuser 501 sudo -n -u c3r /bin/sh -c 'nohup ... &'`.
Plain SSH nohup causes false startup failures.

## Current exact runtime and accepted results

Final source:
**`f51034b34b94eeb3bc3a8dcfbd2e4faf8eabc6350721b7f7172da7b406504703`**

Guest final f510 launcher/starter remain staged in idle private fixture:
- launcher `2d161a9c0508c378c0bae586666a64e85bdf2fad5327b66112ddffefbc375c21`
- starter `5a64e8627f187d4a3eaaba77278ebe387b595a992ce7f3d92359f3dcd38e4c5d`
- unchanged production f7b bridge `71d3fa9488b4eeda83e1ddc28de79aac9ed6d618368f0500ef1bf0251e3b5e69`
- viewer9b `a22e864121b123cc13e80b7b28da99048460cba5445a714782aa3f0c39284485`
- native.so9b `5f4d5d3f1deb239db37d82baed963952690845a52a8601d9fc535f837a354e75`
- PE native shim `4151b134ac7c2076abea13644e54519a95c8bb349cafdf3c01c1e85d35ead935`
- cross manifest SHA `1fe3d270701f8d973018c6ac2a32d58d98f613fdeb3599c984823db7470e37a3`
- protocol50
- shader `43961a92338f6b5b1f5ebd12dfdc806057fe43d8bfeb50abdb7872d274b601c4`
- fallback `bbde10b5e91a3e54c337eab67c2e5acce8abf384657043eb46bbe056ea799281`

Canonical artifacts:
`/mnt/datasets/c3r-build/release-1019/artifacts/f510.../`
including `intel-macos/manifest.json`, `cross-manifest.json`.
Root review `/mnt/datasets/c3r-build/handover/sol-root-f510-production-artifact-review.json`.

Game SHA:
`66d0ae91de6f4a2376e03aa74eb29a8256f02108ca3a4c024a4857337bc45ee6`
Original IFC:
`032c24f528881e25e24d0acf4571fd92e7976302e50474950c505a08ee490e4b`
sound.dll:
`32da5ea027041a5ceda170f40be38ef63ec3815efdb3846f62d6e130d3ec668c`
Protected Mal3:
`d6af1b185d858090c143f11fcc50f16a463b80c4d6dff4d288b5e81286e40216`
Private alias `.../Conquests/Saves/qa-mal3.SAV` resolves original read-only Mal3.

Accepted scopes:
1. Healthy USB-null f7b title-D and loaded-E normal GUI Quit, app0, no signals. Both healthy initialized1/count0, so **not causal failed-unwind acceptance**.
2. f510 cancellation suite:
   - retained failure window/AppKit worker-gone then launcher-only atomic TERM -> **normal positive143**
   - actual owned game child + lock/session then launcher-only TERM -> normal143 and normal child/lock/session cleanup.
   - no external child/server signals, KILL or watchdog.
   - Intel binary unsigned as recorded; no visible startup-failure UI acceptance from white desktop/nil-device warnings.
3. Corrected no-device true original-vendor title control:
   - actual original IFC, no Rust game/renderer modules, full title visible.
   - ordinary Exit -> **native005F8CBF read0x12**, natural Wine exit5.
   - watchdog detected fatal, waited12s; role remained vendor, natural exit file true before cleanup, **no signals/cleanup rows**.
4. Final f510 production no-device **title causal acceptance passed and independently accepted by root**:
   - full fresh required oracle, unwind_count1, first close/cleanup Some0, init/module/all owners0, factories true.
   - ordinary title Exit -> actual app0, no fatal or external signals.
   - root review `handover/sol-root-f510-no-device-title-review.json`.
5. No-device loaded Mal3 **INCOMPLETE**, not passed:
   - correct Saved Game summary1050BC Romans Emperor.
   - terrain/city map near deadline, full HUD/units not photographed after adoption.
   - no fresh loaded trace request/oracle, no ordinary Quit.
   - watchdog exact game TERM at480.223s.
   - retained f510 launcher then exact-owned atomic launcher TERM -> normal143.
   - no native game fault claimed; cap cleanup only.

## Restoration is complete

After incomplete loaded arm, restored **USB-null** using normal manager stop/configure/start.
Actual original argv and instance match baseline exactly; RAM/cgroup unchanged.
Original app config restored:
SHA `3da0faff0e3507e1a985e6b845c96eeeef7b1e8d61afe5b1a1cbcb77401388d1`.
Private INI SHA:
`69189422b52f9d76e0faeb9a099b664d06fe81616bfea8407c18abc547be8d70`.
Exact runtime/game/sound/save/effective IFC hashes/resolves match baseline.
Read-only SMB remounted, unlocked Aqua.
Fresh default QEMU USB48kHz stereo inventory and **afplay0 in2.703290678s**.
Exact-marker scan empty, no launch.lock or owned runtime.
Broker lane explicitly released. No VM actions since restoration verification; latest work only local archived-file reads and empty package mkdir.

Restoration evidence under:
**`/mnt/datasets/c3r-build/vm-audio-production-sol/nodevice-retry-sequence/`**
- `restored-host-fixture.json`
- `restoration-guest-result.json`
- `restoration-cgroup.json`
- `restoration-marker-absence.json`
- `restored-boot-readiness.txt`
- `clean-stop-restoration.json`

## Critical evidence and historical failures

Main evidence base:
**`/mnt/datasets/c3r-build/vm-audio-production-sol`**
Durable handover:
`/mnt/datasets/c3r-build/handover/vm-snapshot.sol.md`
Last handover append predates latest corrected retry completion; do not assume it contains latest restoration/incomplete timeline. Root received direct messages.

Corrected retry:
**`.../nodevice-retry-sequence/`**
- vendor-preflight, title-preflight, loaded-preflight exact source/results
- `vendor-evidence.tar` / extracted same stem
- `vendor-verdict.json`
- `title-evidence.tar` / extracted
- `title-verdict.json`
- `loaded-incomplete-evidence.tar` / extracted
- `loaded-verdict.json`
- screenshots `loaded-*.png`, title/vendor frames
- `loaded-screenshot-timeline.json`
- `loaded-additional-timing-evidence.json`

First invalid no-device attempt is immutable:
`.../nodevice-sequence/vendor-setup-failure-evidence.tar`.
It proved no output hardware + natural native afplay1/error-66680, but vendor helper exited1 before game because new private session parent absent. Not game/audio evidence.
Exact helper audit proved historical compat helper expects **existing parent**, creates only parent/wine-compat via create(), not create_dir_all.
Actual Wine dependencies all lengths/hashes validated.
Do not delete or relabel first invalid attempt.

Helper source/provenance:
- `/mnt/datasets/c3r-build/mac-audio-sol/offline-snapshot2/src/main.rs`
- `.../src/runtime/wine_compat.rs`
- `.../offline-payload2/provenance.json`
- exact helper SHA `d60f656b58586f6120952c6d9e7a5878d3fd3eaa7ac941f157450d6aa323ad59`
- guest helper `/Users/c3r/civ3r-inproc-31b5951a-sol/qa-wine-private-probe-v2/compat-launch-probe`

## Root-approved current preflight/watchdog package

Authoritative:
**`/mnt/datasets/c3r-build/vm-audio-production-sol/nodevice-retry-root-approved`**
Read:
- `root-approved-hashes.json`
- root review `/mnt/datasets/c3r-build/handover/sol-root-no-device-retry-review.json`

Authoritative hashes:
- watchdog `f7a21ea51b9662165aeb54ba732d5ad13d1be9d824d70ef2b47c27f386bcce89`
- preflight `c9796a6c71840b02706e3ca627ef16d7aecf71d3f7d27546afb1fbc6c22a80f4`
- vendor bootstrap `0af71824f4b2c1eb73d803d4760d7a9ea775a3a846f51b474783d353c7762dea`

Older `nodevice-retry-final-root-review` and `nodevice-retry-root-review` are historical, not authoritative.

Approved watchdog:
- role-specific natural exit path:
  vendor `evidence/audio-production/vendor-nodevice-exit-code.txt`
  production `evidence/app-exit-code.txt`
- rejects stale exit artifact and stale `evidence/stop-fatal-watchdog`
- distinct `test_role` vs cleanup `process_role`
- detection-time exit state separate from pre-cleanup state, latter sampled after12s fatal grace
-480s cap, exact environment marker + atomic PID-version TERM of game/debugger only, shared server excluded
- binary per-file log reader tracks device/inode/offset/64-byte checkpoint, handles truncation/regrowth/replacement/disappearance and separate tails
- new vendor log read from offset0; **archive vendor log before starting any later production watcher**, otherwise it will reread vendor fault!
- no manual stop flag used for natural completed retry arms.

Preflight checks canonical f510 manifest/PE pins, all6 runtime hashes, actual bundle starter/effective game executable hash+resolve, read-only assets/game/sound/save, privateINI, no output endpoint with valid recursive audio schema, bounded native afplay natural nonzero, USB/QEMU absence,3072MiB/8CPU/cgroup, unlocked Aqua, exact-marker absence + conservative process names, byte-identical lean config except exactly one QA marker.
Also vendor fresh owned empty0700 nonsymlink parent + exact helper/Wine dependencies; pins bootstrap/watchdog; role exit absent; stale stop absent; vendor active log absent.

## Why previous loaded arm timed out

Root/mac_audio_sol/preview_smear analyzed archived logs. No late native sample exists; do not claim native stall or permanent missing art.

Accepted analysis:
`/mnt/datasets/c3r-build/quit-audio-sol/loaded-no-device-analysis/`
- **`bounded-rerun-plan.md` SHA prefix aedda67a**, root accepted for source-only driver preparation
- `comparison.json`

Healthy loaded-E:
- world art request303.299s after PE load
- PE art adoption334.276s, art12 done336.377
- healthy023 photo319.717s was pre-adoption/missing HUD
- healthy024 photo340.654s visibly full HUD/minimap/buttons/unit bodies

Incomplete no-device:
- world art request420.050s
- same4367sprites/5613KiB, same1268KiB world scene
- native compile5575ms
- PE ArtJob35957ms
- PE adoption456.034s, all visible456.097s
- last missing-unit message455.999s, unit animation shown456.110s
- GPU upload425.866s is not PE adoption
- publisher still active through476.641s
- partial map photo452.877s was **before adoption**, no later pre-deadline screenshot.
- audio DLLs unloaded during startup, never reloaded.
- operator/model roundtrips consumed significant ready-to-action time. No exact historical action journal.

Precise host file timestamps:
- preflight start18:49:16.862613Z
- watchdog approxT0 18:49:46Z, deadline18:57:46Z
- loaded-title.png18:50:21.639984Z
- title-ready18:51:10.647579Z
- file-dialog-check18:51:49.544045Z
- ready file-dialog18:52:41.116668Z
- process snapshot immediately after save Accept18:53:26Z
- summary18:56:07.346194Z
- initial map18:56:44.421654Z
- partial map18:57:22.345125Z
- named quit-dialog18:58:16.227797Z is actually retained failure window after cap
- actual launcher cleanup app143 file18:59:59Z.
No separate exact VNC click journal exists. Use bounds/log timestamps, never invent exact historic click times.
No fresh loaded trace request was made.

## Current proposed driver design, not implemented

Need now implement local package in empty `loaded-owner-driver-sol`, tests and recipe; root review before any execution.

Requirements from accepted bounded plan:
- same480s total from actual existing watchdog start; no reset after title/load.
- reserve final **>=60s** for fresh loaded oracle/full HUD evidence/ordinary Quit.
- by420s prerequisites must be ready; if not, classify incomplete permanently and attempt only known state-checked normal Quit if possible.
- targets title75s, selected file120s, summary330s, map/art390s. These are targets, not assumptions/promises.
- one owner driver loop, no per-click model roundtrips.
- every action **and capture** journal actual begin/end UTC and monotonic, run_id, phase, arguments/result, elapsed from watchdogT0 and prerequisite evidence ID.
- visible-state barriers; no blind clicks through loading/unresponsive screens.
- no QMP, unvalidated Ctrl-A filename replacement, space token, or super-w.
- root visual review of stored post-adoption full image remains required; do not auto-accept weak computer-vision evidence.

Potential architecture considered:
1. `owner_driver.py`: stdlib state machine + injected backend for pure offline tests; real backend uses bounded SSH and existing VNC helper only after `--execute` plus root approval/lease record.
2. Local image/state gates from archived screenshots, using Pillow:
   - title pattern/logo + menu, prioritize summary/file/quit before underlying title.
   - file gate must visibly select **qa-mal3.SAV**; row varies. No unvalidated editing.
   - summary must show1050BC/Romans/Emperor.
   - full map gate requires current-run **PE-side adoption/all-visible** log ordering, full HUD and representative unit image gates. Root manually inspects stored full frame after run.
   - busy/stuck overlay blocks additional clicks; avoid focus retry through unresponsive screens.
   - proposed focus click on known native window titlebar before Load could avoid first-click focus consumption, but **not yet validated or approved**. Safer source review should explicitly flag any new focus path.
3. Fresh trace helper touches request **once**, waits<=5s, requires fresh output after touch and current run; strict required audio fields plus actual known map/player fields. No unit-record requirement.
4. Backend start should run checked fresh preflight and archive its source/result, start independent watchdog, then Aqua App. Watchdog must outlive driver failure.
5. Actual watchdog T0 currently exists only as guest `start=time.monotonic()`. Proposed minimal additive **watchdog anchor metadata** change would record actual guest T0/UTC/monotonic/runid/PID atomically before main loop. This would require separate root review and a preflight hash-pin update. Do not pretend it already exists.
   - Host driver can conservatively use command-begin monotonic as upper-bound deadline, never later than actual watchdog deadline.
   - Record host launch/readiness interval and guest anchor clock domain; do not directly subtract guest monotonic from host monotonic.
6. Normal App wrapper must record actual shell wait status with `if wait ... else status=$?`, never fabricate exit0. Existing guest `/evidence/run-app.sh` does this with `set -u`, writes app-exit-code. Could pin/reference or provide exact reviewed wrapper rather than trust unknown source.
7. Driver never signals game/server itself. Approved standalone watcher owns cap cleanup. Driver candidate acceptance requires actual app0, no fatal/TERM, no owned descendants/lock. Mandatory USB restoration remains separate reviewed owner sequence.
8. Tests should be pure fake backend/state clock + archived positive/negative screenshot gates, not actual VNC/SSH. Include no clicks on wrong save, no progression through busy state,420 reserve failure, no trace reuse/touch renewal, no false map from pre-adoption frames, normal Quit order, deadline unchanged.

Image gate thoughts, not commitments:
- archived full HUD image has Quit dialog at top; bottom HUD and unit patches can calibrate independent map-full detector, while classifier prioritizes Quit modal.
- Partial vs full healthy frames allow mask tests for unit-body presence; require negative partial screenshots to fail. Do not let background dominate similarity.
- Captures can use an ephemeral rolling path plus retained barrier images to avoid hundreds MB; journal must explicitly state which captures are retained vs ephemeral and preserve exact capture timestamps, not relabel later copies as new captures.
- Root source backup policy means no cleanup/deletion of source/backups; use local scratch carefully, no large archives during disk pressure.

## Owner-confirmed controls,1280x800

From current final f510/unchanged9b native bitmap viewport. Revalidate against shipping actual-App layout before timed run; never blindly reuse on changed layout.
- title Load Game `(434,419)`
- file dialog selected qa-mal3.SAV -> Accept `(750,554)`
- Saved Game1050BC Romans -> Accept `(854,312)`
- map Escape key
- normal Quit dialog Yes immediately `(493,214)` then Accept `(771,243)`
- title Exit `(403,640)`

Held VNC sequence:
`.../helpers/vnc -t 10 move X Y pause 0.35 mousedown 1 pause 0.2 mouseup 1`

Archived screenshots for local template/test use:
- current f510 title:
  `/mnt/datasets/c3r-build/vm-audio-production-sol/nodevice-retry-sequence/title-complete-check.png`
- selected file:
  `.../loaded-file-dialog.png`
- summary:
  `.../loaded-summary-check.png`
- current partial map:
  `.../loaded-map.png`
- healthy partial:
  `/mnt/datasets/c3r-build/vm-audio-production-sol/023-loaded-E-map.png`
- healthy full HUD + normal Quit:
  `/mnt/datasets/c3r-build/vm-audio-production-sol/024-loaded-E-quit-dialog.png`
- current busy:
  `.../nodevice-retry-sequence/loaded-file-dialog-check.png`
- current stuck:
  `.../nodevice-retry-sequence/loaded-progress-1.png`
- current named loaded-quit-dialog is post-deadline failure, **negative only**.

## Actual trace fields, not invented

Read archived healthy full loaded trace:
`/mnt/datasets/c3r-build/vm-audio-production-sol/loaded-E-evidence/evidence/audio-production/loaded-E-complete-map.trace.txt`

Actual first lines:
```
# civ3-bridge trace: map_renderer 0x0
# native_audio Diagnostics { ... }
view camera 0 0 visible_x -1 5 visible_y -1 5 zoomed_out 0
map 80 80 wrap 1 0 seed 964580286 flags 0x5
player 1
...
civ_adjective 1 Roman
```
Unit header exists but **no unit records required** with parked map renderer.
Fresh map/player/GUI identity can use expected80×80, wrap1/0, seed964580286, player1, adjectiveRoman, plus protected save SHA and native summary/frame.

Required causal audio line:
```
installation: Installed
unwind_count >0
first_midi_close_result: Some(0)
first_cleanup_result: Some(0)
initialized/module/midi_backend/master_backend/wave_in_backend: Some(0)
factories_cleared: Some(true)
```

Current-run PE-side log gating example from incomplete loaded bridge:
```
420.061 DEBUG ifc23::live: scene: shown while visible art loads
423.429 INFO ifc23::live: memory: ... scene cells 1268 KiB ...
425.654 INFO civ3r_native: ... art11 ...5575ms ...   # GPU upload, insufficient
455.999 DEBUG ifc23::units: ... has no art ...yet
456.034 INFO ifc23::live: art: grown by4367sprites ...35957ms for city labels, control states, interface, unit pictures, units, visible art not resident...
456.097 INFO ifc23::live: scene: all visible art arrived36035ms...
456.110 INFO ifc23::units: unit126 shows animation...
```
Use PE `ifc23::live` adoption and subsequent all-visible, not Unix GPU upload. Require fresh bridge file/epoch after runT0, no old logs/trace can satisfy.

## Secondary queued source-only F1/scenario preparation

Root earlier requested source-only exact f510 fixtures/actions from:
`/mnt/datasets/c3r-build/combat-recipe-army/next-vm-acceptance-plan.md`
No runtime granted. **Driver preparation is current priority.** No F1/scenario plan file written yet.

Read-only findings:
- F1 candidate previously native-confirmed30BC Romans:
  original `/mnt/datasets/winvm/wine-civ3-wow64/drive_c/Games/Civilization III Complete/Conquests/Saves/20226 Rom fl war.SAV`
 62567bytes, SHA `b054d5a294f8ba3c737e92f4358ee3324396b9893ac5b946e9a43ad136785cd4`.
- historical F1 screenshots:
  `.../vm-qab41-sol/032-f1-open.png`,033 popup,034 one wheel.
  Shows9 visible rows Antium,Cumae,Ellipi,Eridu,Hispalis,Lugdunum,Lutetia,Neapolis,Pisae; need fresh native city-count/endpoint proof. Historical smooth off and popup did not establish row/city identity or full endpoint acceptance.
  Production middle cell approx `(1045,512)` at this viewport; validate visible row, do not blindly assume identity.
- settings key `input.smooth_menu_scrolling`, defaultfalse.
  Source `/.../src-f510.../crates/viewer-settings/src/controls.rs`.
  Launcher default effective settings `<CIV3R_HOME>/settings.toml`, unless CIV3R_SETTINGS override.
  Toggle via normal native Preferences + Accept, not file writes.
- stock scenario candidates exact host originals:
  `Conquests/Conquests/1 Mesopotamia.biq`
   27873bytes SHA `fb65288ebb3a8927f86105e4b28c1aa97618239a932d1995a8de2535f56826a8`
  `Conquests/Conquests/3 Fall of Rome.biq`
   59518bytes SHA `bc823f1cb7001ee786012af87d56f80b9096b5e451bfbf2fec708ca0e96a01af`
  Guest corresponding under `/Users/c3r/civ3-assets/Conquests/Conquests/`.
  **Short load times and exact native nation lists not yet qualified.** Do not invent them or claim scenario-ready pass.
- Source plan requires F1<=6min, scenarios<=8min including startup/loading/Quit; need readiness/budget gates, no silent cap redefinition.
- No offscreen combat triggers may be manufactured; combat diagnostic/comparator reviews still separate.

## Next steps

1. Implement **local-only** driver/journal + recipe and pure tests in empty package directory.
2. Decide conservative visible-state gating and actual watchdog clock anchoring. Any watchdog/preflight change must be separate exact delta/hash for root review.
3. Use archived trace fields and frames, no VM hash/capture or activity during current AMD timing.
4. Produce source hashes, delta(s), test results, and explicit unresolved calibration/visual gates. Send concrete package to `/root` and coordinate with `/root/mac_audio_sol`.
5. Do not execute driver, profile transition, or game until fresh root review and broker grant.
6. Preserve all accepted title/vendor and incomplete loaded evidence and restored fixture. No cleanup/deletion.

## Report at 2026-10-20T19:41:12.602Z

Original transcript line 11379.

Prepared the local controller draft and review notes in [loaded-owner-driver-sol](/mnt/datasets/c3r-build/quit-audio-sol/loaded-owner-driver-sol/REVIEW.md).

Six offline tests pass. The draft preserves the 480-second cap, final 60-second reserve, fresh loaded oracle, and normal Quit sequence.

No VM activity or hashes were performed. A reviewed VM adapter and screenshot calibration remain required before execution.

## Report at 2026-10-20T19:50:56.801Z

Original transcript line 11641.

Completed the runnable source package with 12 passing offline tests and a separate 30-second radio calibration arm.

Committed as `ce5e104270258e207e16c3a8cf7a0ff5336c41d5` and replicated to both laptops. Both remote heads match; both integrity checks pass.

No VM operations occurred. Root review and calibration approval remain required before qualification execution.

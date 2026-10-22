# Historical agent /root/pointer_vm_qa

This is a recovered historical report, not a live agent handle or a new instruction.

VM pointer/UI tests and original vendor-only audio control after VM audio correction.

Current disposition: Vendor control exited 0; later retry failure is historical, not a current live handle.

Session `01a152bd-be55-7072-a5b9-07973d0e679a`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-19T05:58:49.175Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T07-58-49-01a152bd-be55-7072-a5b9-07973d0e679a.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T10:54:35.293Z

Original transcript line 6784.

Completed the current VM QA pass and released the GUI/share.

- Default map, F1, settings, capture, and owned-game exit passed.
- Remaining sampled native calls were verified as required readbacks.
- Explicit comparison matched both cameras at `(2752, 1440)`.
- Evidence is in [integration-status.json](/mnt/datasets/c3r-build/macos-port/macos-game-live/integration-status.json).

Pending: targeted icons/fog/Z/capture smoke on the forthcoming scaled-writer fixes.

## Report at 2026-10-19T11:12:52.761Z

Original transcript line 6805.

I’m resuming the final VM smoke test for the changed rendering paths: zoom, fog and unit overlays, scaled Civilopedia icons, Alt targeting, and the HUD click mask.

## Report at 2026-10-19T11:21:30.839Z

Original transcript line 6965.

The new fog wrapper is exercised in the VM: 330 calls were skipped and none reached the original renderer. The HUD click-mask readback remains active, while the `NO_MOUSE` overlay is absent from the early readback audit.

## Report at 2026-10-19T11:32:13.900Z

Original transcript line 7140.

## Identity and scope

You are `/root/pointer_vm_qa`, a subagent of `/root`. You own macOS **work VM GUI QA**, currently holding the GUI/share lease for final run **23**.

Root owns VM lifecycle, final source freezes, snapshot/clone, commits, and user coordination. **Do not boot/stop VMs or restart SMB. Do not touch the physical Mac session.**

User wants autonomous work, no questions, no game asset copies, and no Nexus/NixOS checkout changes. Shared repo has extensive changes; do not reset or commit.

## Latest parent instructions

Finish run23’s narrow changed-path smoke:

- Strict default title → saved map.
- Z normal/out, fog, unit/team-disc rendering.
- Civilopedia resource/unit icons.
- A nonidentity scaled UI/list icon and Alt anchor at its visible corner.
- Actual HUD click through the retained hit mask.
- Bounded audit showing scaled wrappers skipped and `NO_MOUSE` overlay no longer retained.
- Preserve actual Main_GUI and navigator readbacks.
- Clean owned processes/helpers, release GUI/share, update `integration-status.json` and transport prerequisites.
- Do not repeat F1/preferences/fallback suites already proven.

**New user requirement received during run23:** configured views apply only during actual gameplay. Title/start/new-game setup must be full-window. `/root/ui_feature_audit` is implementing this shared viewer fix. Root says finish/clean run23 first. After the new candidate, one focused **configured title/setup → game → title transition** check will be needed. Snapshot remains held.

## Access and paths

Repo:
`/home/sirati/devel/rust/civ3-renderer`

Host root:
`/mnt/datasets/c3r-build/macos-port`

Evidence:
`/mnt/datasets/c3r-build/macos-port/macos-game-live`

SSH:
`.../vm/helpers/ssh`

VNC:
`.../vm/helpers/vnc`

QMP:
`.../vm/helpers/qmp.py`, socket resolves to:
`/mnt/datasets/macvm/instances/work/qmp.sock`

Always assert QMP VM name `c3r-macos-work`.

Guest:
- Source `/Users/c3r/civ3-renderer`
- Target `/Users/c3r/civ3r-build`
- Runtime `/Users/c3r/civ3r-runtime`
- Readonly assets `/Users/c3r/civ3-assets`
- Private game cwd:
  `/Users/c3r/civ3r-runtime/game/Civilization III Complete/Conquests`

VM is Sonoma x86_64, **3 GB**, with a hard shared VM cgroup limit of 10,000,000,000 bytes. No concurrent Cargo work during live QA.

Skills already read: waiting, unslop, you-are-on-nixos. Avoid busy polling.

## Current exact runtime and source

Run23 is active:

- Launcher PID **4242**
- Viewer PID **4289**
- Game PID **4292**
- Attached SSH tool session **67030**
- Viewer log:
  `/Users/c3r/civ3r-runtime/logs/session-1792408424322935000-4242.viewer.log`
- Host launcher log:
  `launch-23-scaled-final.log`

Exact installed hashes:

- Viewer:
  `1e90ca6ab1f7f83be01028e39308a62b8d0125e9a45e7e8753e20d5e8ec6d9c8`
- Windows DLL:
  `e932ab8083d9b7c67529a91e692e30ca5ab0cdeb500af8b0dda952640936885e`
- Launcher:
  `a3018e73696f45713342f2621265c65d441731891cf8bfc35a3b3a0e5b93fe75`
- Rendering binaries embed shader **252dcce1**.
- Native/reference checks **114/114 passed** before staging.
- Guest source is exact final Windows repack **2d084a0e**, **572 files**. Transport verified 431 native closure hashes unchanged from prior 3411.
- Transport explicitly confirmed no build/staging active and gave GO.

Manifest:
`runtime-23-manifest.txt`

## Launch and audit helpers

Host and guest launch helper:
- Host `macos-game-live/launch-game-qa.sh`
- Guest `/Users/c3r/civ3r-qa/game-integration/launch-game-qa.sh`

Use Aqua session:
```bash
qa_uid=$(/usr/bin/id -u)
sudo -n /bin/launchctl asuser "$qa_uid" \
  /usr/bin/sudo -n -u c3r \
  /usr/bin/env CIV3R_PIXEL_AUDIT=on \
  /bin/bash /Users/c3r/civ3r-qa/game-integration/launch-game-qa.sh
```

Helper sets native Rust launcher/viewer, Wine11 private prefix, SwiftShader Vulkan, bitmap presentation, preload off for this 3 GB QA VM. No production-default change.

Wine prefix:
`/Users/c3r/civ3r-qa/wine-runtime/prefix-wine11`

Wine runtime:
`/Users/c3r/civ3r-qa/wine-runtime/runtime/Wine Stable.app/Contents/Resources/wine/bin/wine`

Host bounded audit helper:
`macos-game-live/audit.sh LABEL`

It touches `civ3-pixel-audit.request` in private game cwd, waits at most 30 sec for worker to remove it, then saves:
`pixel-audit-LABEL.json`

Full JSON remains in artifact. Console summary excludes lengthy `readback_images`.

Audit fields:

- `policy`
- `title`, `ui`, `map`, `present`, `readback`, `empty`, `replay`, `unknown`
- `scene_events`, `pixel_reads`, `text_runs`, `inputs`
- `readback_images`, capped at 64 distinct rows
- `readback_reasons`, uncapped totals
- `map_original_samples`, capped at 32 distinct rows
- `scaled_wrappers.scaled_a4/scaled_fog/scaled_colored`, skipped/original counts
- `resized_capture.file/unsupported`

Expected legitimate readbacks:

- Main_GUI portrait/click mask **0x9fdd84**
- Navigator images **0x9b3f50**, 300×145 and **0xa0e248**, 230×108
- `map_sprite` 8060 lazy loading is now classified `file_image_source`; nested PCX copy is separately suppressed.
- Main map overlay **0x9fd80c** has `NO_MOUSE` and must no longer count as `native_hit_mask`.

## GUI helpers and pitfalls

Host Python:
`/nix/store/b5bpi6zfajzzrwwpgba2q6li3nnya4bs-python3-3.14.7/bin/python3`

Helpers in evidence directory:

- `ui.sh shot NAME`
- `game-input.py key KEYS`
- `game-input.py tap X Y`
- `game-input.py point X Y`
- `game-input.py drag BUTTON X Y TX TY`
- `qinput.py click 1|3`
- `qinput.py motion DX DY COUNT`

QMP key for Space is **`spc`**, Command is **`meta_l`**.
Alt persistent snapping; Ctrl leaves it.

**New VNC connections reset pointer to 0,0.** Use QMP for input. Absolute jumps sometimes hit/drag the split divider; shortcut capture has been reliable. While Alt is active absolute pointer movement is ignored; use relative `qinput.py` clicks/motion or Ctrl first.

`qinput.py` adds an owned relative USB mouse if needed and restores the original tablet afterward. It was re-added during run23’s Alt click. Marker:
`macos-game-live/qa-mouse-added`
Device ID:
`c3r-game-qa-mouse`

Remove only this owned device during final cleanup, restore absolute tablet, and record QMP state. Existing old Wine device processes 1407/1415 belong to earlier baseline work; leave them alone.

UI/art can take **10–30 sec**, map initial art ~2 minutes. Do not assume a screenshot at 3 sec reflects the latest underlying page.

## Lock-screen recovery in run23

Guest auto-locked while idle during native builds. Initial run23 screenshots were entirely black; this was **not a renderer defect**. Command-Tab revealed lock screen.

Unlocked with:
```bash
.../vm/helpers/vnc --delay 80 \
[Credential-related line redacted from this historical copy.]
```
[Credential-related line redacted from this historical copy.]

Started bounded guest:
`caffeinate -u -d -t 1200`

Its PID is stored on host:
`macos-game-live/caffeinate-23.pid`

Guest log:
`/Users/c3r/civ3r-qa/game-integration/caffeinate-final.log`

It began around guest 13:15, current screenshots around 13:27–13:29, so it may expire around 13:35. Renew bounded wake prevention if needed. Final cleanup must kill only the stored owned caffeinate PID if still active.

After unlock, clicked title bar `(640,60)` to focus native viewer. It renders correctly.

## Current GUI state

**Civilopedia → Aluminum detail page**, native window active, **Alt mode is OFF** after Ctrl.

Latest screenshot:
`pedia-23-aluminum.png`

Native outer roughly `(134,46)`, content origin `(134,74)`, client 1012×632.

Civilopedia board spans x302..975; top header y75..124, left content x328..800. Right navigation:

- Up arrow `(886,362)`
- Back arrow `(864,382)`
- Forward `(912,382)`

Current Aluminum page:

- Large aluminum image top right, x822..951, y74..200.
- Rocketry requirement icon around `(560,510)`.
- Unit links at bottom around y636..651:
  `AEGIS Cruiser, Cruise Missile, F-15, ICBM, Jet Fighter, Modern Armor, Radar Artillery, Stealth Bomber, Stealth Fighter, Tactical Nuke`.
- “Modern” near x704..750,y636 and wrapped “Armor” x354,y650 may open Modern Armor unit page.
- Right Up probably returns Resources, then another Up to Main. On Main, Units row text near `(420,389)`.

All current Civilopedia resource icons shown so far are natural identity-size images, so new resize capture counters remain zero.

## Run23 checks completed

1. Strict default title renders, then protected city save loaded to actual map.
2. Normal map renders city **QA Rome**, worker, terrain, fog, team/selection disc, HUD.
   Screenshot `map-23-normal.png`.
3. Pressed Z once:
   - Native world zoomed out correctly; unit/selection disc/fog retained.
   - UI stayed normal size.
   Screenshot `map-23-z-out.png`.
4. Pressed Z again, normal world rendering restored, visible behind Civilopedia.
5. Clicked actual **HUD Civilopedia button** `(208,100)`.
   - It belongs to Main_GUI’s known buttons (`controls.rs` g+0x98d8).
   - Civilopedia opened, proving actual HUD button input through retained mask.
   Screenshot `pedia-23-from-hud.png`.
6. Opened Resources `(440,503)` → Resource Index `(450,235)`.
   - Resource list and large resource image render.
   Screenshots:
   `pedia-23-resources.png`,
   `pedia-23-resource-index-settled.png`.
7. Positioned near Aluminum icon `(372,160)`, pressed Alt.
   - Snapped pointer hotspot landed at icon’s **lower-right corner**, about `(389,176)`, within actual icon rect x354..390,y142..178.
   Screenshot `pedia-23-resource-alt-anchor.png`.
   - Relative `qinput.py click 1` activated Aluminum.
   - Aluminum detail and linked text/images rendered.
   Screenshot `pedia-23-aluminum.png`.
   - Ctrl left Alt mode.
8. `NO_MOUSE`:
   - Earliest title audit has **550 readbacks**, vs prior 551, and UI skipped731 vs prior730.
   - **0x9fd80c absent** from early 64-row readback detail, where it was previously an early row.
   - Actual Main_GUI **0x9fdd84** remains classified `native_hit_mask` in map original samples.
   - Preserve bounded-detail limitation; source contract excludes `NO_MOUSE` independently.

### Current audit evidence

`pixel-audit-23-title.json`:
- title skipped35/original0
- ui skipped731/original0
- present skipped4/original0
- unknown original0
- readbacks550: palette3, hitmask547
- no scaled/resize calls yet.

`pixel-audit-23-map-startup.json`:
- scaled_fog **330 skipped / 0 original**
- UI/title/present originals0, unknown0
- original map3 all legitimate Main_GUI mask at0x9fdd84
- two expected navigator readbacks.

`pixel-audit-23-z-out.json`:
- scaled_fog354/0
- unit/team disc/fog rendered visibly
- scaled_a4 and scaled_colored **0/0**.

`pixel-audit-23-pedia-open.json`, `23-resources-list`, `23-resource-index`, `23-resource-icons`, latest `23-aluminum`:
- scaled_fog **378/0**
- title/ui/present/unknown originals **0**
- map original5 all retained Main_GUI mask at0x9fdd84
- `resized_capture.file=0`, unsupported=0
- scaled_a4=0/0, scaled_colored=0/0.

**Do not falsely claim actual exercise of scaled_a4/scaled_colored or nonidentity file captures.**

Private save has only a worker/city, no combat/effect animation. Colored/effect wrapper may remain unexercised because original camera is parked at0,0 and real unit is42,44.

## Immediate outstanding hint request

Last action was a **followup_task** to `/root/macos_bridge`, waking it for a natural recipe hint, without requesting code changes:

> Run23 resource main/list/Aluminum detail render, but resized_capture.file and scaled_a4 remain0; scaled_colored0 with lone worker outside parked camera. Need a natural UI recipe to invoke the nonidentity file path, rather than claiming coverage from identity icons. No source changes requested.

A response may arrive next. Earlier `send_message` also reported this. Bridge may have been idle, hence the followup.

## What remains to finish run23

1. Get bridge owner’s natural recipe for actual nonidentity UI/file capture. Try a unit Civilopedia page and scaled list icon.
2. Verify an actual nonidentity icon and Alt corner anchoring if reachable. Existing resource identity-icon Alt proof is valid but does not exercise new `resize_step`.
3. Capture audit:
   - Need `resized_capture.file>0` and scaled wrapper counts where possible.
   - Original display callback counts must stay suppressed.
   - Classify any unexercised scaled-colored/effect paths honestly.
4. Root mentioned capture smoke too. Prior default capture run20 passed; latest requested narrow paths include capture. If needed use documented Ctrl+Shift+Space after configuring UnitMouse split. Avoid another full settings matrix.
5. Preserve full viewer/bridge logs, then extract snap logs from full viewer log. Existing `alt-23-resource-anchor.log` used tail85 too late and missed snap lines, so use full log.
6. Update `integration-status.json` phase23 with exact hashes, results, limitations, audit snapshots.
7. Close native red button `(147,60)`. Default launcher should terminate owned game and return0, as run20/21/22 already proved.
8. Clean owned USB mouse and caffeinate. Preserve saves. Release GUI/share to root, transport, templates. No VM/service actions.
9. New full-window-title/setup fix will require a **separate focused transition test** after root stages its new viewer candidate. Do not call snapshot ready yet.

## Earlier accepted work retained

- Native common-pointer probe:
  - `crates/pointer-control/examples/pointer_qa.rs`
  - `.../README.md`
  - only common API + portable winit; no UI unless `--run`.
  - 22 geometry/capture stages,1160 injections, zero outside-region samples.
  - Bounds, one-pixel axes, divider opening/faces, strip, box faces, focus/menu/release.
  - Latest outside-lock-entry probe SHA:
    `149b9d608399e2ff40ec6b21df06fc285e23e3a4d6419aec1c521913a573c01e`
  - Evidence `pointer-qa-evidence/summary.json`.
- Run20 default ae37/a301/2c14:
  map/F1/preferences/layout/capture via shortcut/Ctrl release/owned-game close exit0.
- Run21 diagnostic db071:
  classified all5 map originals as required HUD mask,2 navigator originals as required sources; no suppression fix needed.
- Run22 db071 explicit `--compare`:
  - matched A center `(2752,1440)`
  - original viewport `[0,0,1280,800]`
  - actual native camera `(2112,1040)`
  - tile x32..54,y31..58, normal zoom
  - screenshots `map-22-native-matched.png`, `map-22-original-matched.png`
  - `compare-22-camera-proof.log`
  - original pixels explicitly enabled; native close exit0.
- Compatibility fallback run18 full modal repaint passed on earlier5ea1. **Do not repeat fallback loops.** Strict default now suppresses legacy drawing and closes owned game on viewer exit.

## Saves and assets

Readonly original game share only; no game asset copies.

Protected private saves outside Auto:

- `Saves/c3r-roundtrip-city.SAV`,21,964 bytes
  SHA:
  `1fa8304d469b397df3c91ba36e51bd03b621c145aa0a9636fefe2738a837b502`
- `Saves/c3r-original-4000BC.SAV`,644,122 bytes
  SHA:
  `8d9c6822d5c384950d948b26efbe388b901f9514c9a1115b8a5f29409ba6e980`

GUI alias:
`Saves/Auto/000-c3r-roundtrip.SAV -> ../c3r-roundtrip-city.SAV`

Load workflow:
Title Load `(390,425)`, remembered alias selected, OK `(750,564)`.
Wait~75sec for Saved Game summary, OK `(854,222)`.
Map mandatory art can take~2min.

Launcher isolates all `.tmp` outputs, private regular scratch/histories. Prior save-load failure was a readonly scratch symlink, fixed long ago. Prior SMB EACCES was server fd exhaustion; live fd limit now65536. Durable SMB cleanup deployment is still held for root/template coordination.

## Owners

- `/root`: coordination, final source/runtime/snapshot, user updates.
- `/root/macos_transport`: exact staging/native builds/manifests/font cache/SMB.
- `/root/macos_bridge`: scaled wrappers, audit, WinUI/capture semantics.
- `/root/macos_pointer`: platform/common pointer, comparison camera.
- `/root/ui_feature_audit`: new configured-view/title full-window fix.
- `/root/rust_launcher`: a301 strict lifecycle/options.
- `/root/macos_vm_templates`: snapshot/clone prerequisites; waiting final release.


## Report at 2026-10-19T11:34:34.101Z

Original transcript line 7280.

Run23 passed its targeted smoke and cleaned up. GUI/share lease is released.

Evidence and limitations are recorded in `macos-game-live/phase23-changed-path-report.json`. Scaled fog suppression passed; nonidentity Pedia and scaled A4/colored branches were not dynamically reached.

Ready for the patched title/setup and Conquests transition candidate.

## Report at 2026-10-19T12:08:55.601Z

Original transcript line 8007.

Phase24 is complete; GUI/share lease released and owned processes cleaned.

Passed title/setup, Conquests pending/ready/menu/cancel-to-title, and saved gameplay split/zoom restoration.

Preserved two failures: large Rise of Rome preview timeout and native process shutdown crash. The shutdown action did not test an in-process return to title.

Report: `macos-game-live/phase24-frontend-transition-report.json`.

## Report at 2026-10-19T12:39:50.063Z

Original transcript line 8365.

Vendor-only control reproduced the exact shutdown fault at `005F8CBF`, reading `0x13`, with no Rust bridge or viewer. Exit code was 5.

Full evidence: `macos-game-live/control-exit-24b-report.json`.

Owned processes are cleaned. GUI/share lease is with VM templates for the authorized audio correction. Ready for the next control and aggregate QA.

## Report at 2026-10-19T12:52:25.291Z

Original transcript line 8598.

The identical vendor-only control now exits cleanly with code 0 after the VM audio correction.

Evidence: `macos-game-live/audio-vendor-control-report.json`.

Owned processes are cleaned and GUI/share lease released. Ready for the frozen protocol38 aggregate.

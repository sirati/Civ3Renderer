# Historical agent /root/macos_vm_templates

This is a recovered historical report, not a live agent handle or a new instruction.

Prepared reusable macOS snapshots and verified stopped clone instances.

Current disposition: Template work covered with scope limits; liveness must be rechecked.

Session `01a152ce-8612-7aa1-b3cc-036f6a0c8bcf`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-19T06:17:08.885Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T08-17-08-01a152ce-8612-7aa1-b3cc-036f6a0c8bcf.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T06:38:41.058Z

Original transcript line 705.

Reusable snapshot is ready at `/mnt/datasets/macvm/templates/sonoma-login`.

[Credential-related line redacted from this historical copy.]

Private login details and connection commands: `/mnt/datasets/macvm/LOGIN.md` (0600).

Management tools:
```sh
/mnt/datasets/macvm/bin/macos-vm clone my-test --template sonoma-login
/mnt/datasets/macvm/bin/macos-vm start my-test
/mnt/datasets/macvm/bin/macos-vm ssh my-test
```

Source files: `tools/macos-vm`, `tools/macos-vm.py`, `tools/test-macos-vm`, `docs/macos-vm.md`. Reset archives previous state and recovers interrupted disk/firmware transitions.

For the later compiler-prepared handoff:
```sh
/mnt/datasets/macvm/bin/macos-vm stop work
/mnt/datasets/macvm/bin/macos-vm snapshot work sonoma-prepared \
  --boot-image OpenCore-apfs-v1.qcow2
```

`sonoma-prepared` remains pending. The automatic boot profile must be explicit for this first work descendant because work still references the earlier boot profile.

## Report at 2026-10-19T06:39:52.214Z

Original transcript line 744.

Both verification clones, `snapshot-check` and `ready-check`, are cleanly stopped. Their disks, history and evidence are preserved. `work` remains running for renderer QA.

## Report at 2026-10-19T08:31:48.582Z

Original transcript line 1835.

## Identity and active task

You are subagent `/root/macos_vm_templates`, working for parent `/root` in `/home/sirati/devel/rust/civ3-renderer`.

Own the reusable macOS VM templates, management tools, documentation, and final prepared snapshot validation. **Final snapshot is still HELD. Do not stop `work`, take `sonoma-prepared`, or start a verification clone until parent explicitly releases it.**

Parent’s latest status:
- Actual Mac Alt title activation now passes on new viewer prefix `2c14e4f4`.
- New source archive SHA256 `54aead1ac7e8e879951910cf2248bdb651102f1bb9ed60358cb1179b4db30970`, 570 files, released to transport for staging.
- QA still replaying save/map/F1/settings/capture/shutdown/fallback.
- `docs/macos-vm.md` is frozen. No source/docs edits unless parent requests another concrete fix.
- Wait for explicit snapshot release.

## User constraints

- Fully autonomous, no questions.
- No physical Mac operations.
- Do not touch Nexus/NixOS checkouts.
- Do not copy original game assets; use existing read-only share.
- Preserve saves, disks, history, source and evidence.
- All renderer VMs **we operate together** must share a hard 10,000,000,000-byte memory cap, swap disabled.
- Unrelated fleet VMs belong to another task and must remain untouched.
- **Do not stop external/unowned `wayland-host` again.** It is another session’s Mac-store VM and may appear/disappear/restart at 4 GB.

Skills already read: unslop, waiting, you-are-on-nixos. Parent uses boss. Avoid acknowledgment-only parent messages; report completed results or actual blockers.

## Stable VM store

`/mnt/datasets/macvm`

- `templates/sonoma-clean`: first installed OS/SSH baseline, ~31 GB allocated.
- `templates/sonoma-ready`: preserved ancestor after desktop login, but boot selection was not persistent.
- `templates/sonoma-login`: **working OS/login baseline**, deterministic APFS-only boot profile, cheap layer.
- **`sonoma-prepared` does not exist yet.**
- `instances/work`: live development VM. Current configured RAM **3072 MiB**.
- `support`: immutable firmware/OpenCore plus private QA key and credentials.
- `bin/macos-vm`, `bin/macos-vm.py`: standalone durable manager.
- `LOGIN.md`: actual username/password/exact commands, **0600**, store **0700**. Never print its secret contents.
- `README.md`: durable copy of public docs.

Existing helper paths under `/mnt/datasets/c3r-build/macos-port/vm/helpers` remain compatible with `work`.
Legacy disk, UEFI vars, QMP and PID paths are symlinks into stable `work`. Boot helper delegates to durable manager.

Access:
- Work SSH `127.0.0.1:22390`, VNC `127.0.0.1:5909`.
- User `c3r`, password in private credentials/LOGIN only.
[Credential-related line redacted from this historical copy.]
- SSH key `/mnt/datasets/macvm/support/id_ed25519`.
- Manager `ssh INSTANCE ...` selects key and per-instance known hosts.
[Credential-related line redacted from this historical copy.]

## Source files owned and frozen

- `tools/macos-vm`
- `tools/macos-vm.py`
- `tools/test-macos-vm`
- `docs/macos-vm.md`

Current hashes:
- `tools/macos-vm.py`: `44be30d2062daeb14d01881ab822160e0499ecf311ed488bb3376ebda9fa9660`
- `tools/test-macos-vm`: `494399c0da35bf124ae511bf14856f24d41709581c2bc3c6141aedd84c90a9f4`
- `docs/macos-vm.md`: `380f9d730da7a58845941bb80937e0077089007230fddfd02de86844f9a7a8b5`

Durable manager matches source and was updated atomically.

### Manager features

Commands: `import`, `clone`, `start`, `run`, `stop`, `ssh`, `status`, `verify`, `snapshot`, `reset`, `recover`, `configure`.

Examples:
```sh
/mnt/datasets/macvm/bin/macos-vm clone test --template sonoma-login
/mnt/datasets/macvm/bin/macos-vm configure test --ram-mib 3072
/mnt/datasets/macvm/bin/macos-vm start test
/mnt/datasets/macvm/bin/macos-vm stop test
/mnt/datasets/macvm/bin/macos-vm snapshot test new-state \
  --boot-image OpenCore-apfs-v1.qcow2
/mnt/datasets/macvm/bin/macos-vm reset test --to sonoma-login
```

- Disk snapshots move stopped qcow2 overlay into immutable template, copy small matching UEFI vars, create fresh overlay. No full disk copies.
- Reset archives original disk/vars/manifest under `instances/NAME/history/TIMESTAMP`.
- No repeated confirmation flag.
- Snapshot and reset are journaled and recover forward after interruption. Includes real process-death test and errors after manifest commits.
- Partial new snapshot overlays preserved under `snapshot-recovery`.
- Running/open disks rejected, snapshot needs clean marker from manager `stop`.
- `status` reports pending transactions even when source disk is temporarily moved.
- Distinct UUID/MAC/local ports and vars per clone.
- OpenCore is self-contained flattened qcow2; per-template profile and SHA inherited by descendants.
- Boot profile `/mnt/datasets/macvm/support/OpenCore-apfs-v1.qcow2`: ScanPolicy `0x10103`, ShowPicker false. Excludes own EFI FAT boot entry; automatically boots APFS OS.
- Existing optional external extensions were discovered in stable manager and **preserved/merged into source**:
  - `configure --ram-mib`
  - `configure --shared-ram on|off`
  - `configure --qemu-extra JSON`
  - memfd shared guest RAM and extra device args.
- **No new `--mouse` option.** Brief undeployed prototype was removed when QA corrected a VNC driver bug. Default absolute tablet retained.

### Memory policy and systemd launch

Persistent user slice:
`/home/sirati/.config/systemd/user/c3r-vms.slice`

- MemoryMax `10000000000`
- MemoryHigh `9000000000`
- MemorySwapMax `0`
- cgroup under `/user.slice/user-1000.slice/user@1000.service/c3r.slice/c3r-vms.slice`

New guest default `6144 MiB`, final verification clone must be explicitly **3072 MiB**.

Admission:
- Counts configured RAM + **1 GiB overhead per VM**.
- Uses larger of reservation and actual MemoryCurrent.
- Scope includes every QEMU inside slice, renderer names `c3r-*`, Mac-store paths, and registered external font QEMU.
- Ignores unrelated `fleet-*`.
- Relevant QEMU outside slice fails closed; parent coordinates its owner.
- Registered in store JSON `scoped_qemu_names`: `winvm-c3r-fonts-current`.
- Font Windows disk `/mnt/datasets/winvm/instances/c3r-fonts-current/disk.qcow2`. Font task used 2 GB serialized, then shut down.
- Empty slice can retain clean file cache. Subtracts `file - shmem - file_dirty - file_writeback` only with no active VMs.
- With active VMs and configured reservations fitting, can make **one bounded <=4 GiB** clean file reclaim request using `memory.reclaim "<bytes> swappiness=0"`, remeasure and recheck. Never reclaims to hide configured overcommit or allows swap.
- Attempts recorded in `/mnt/datasets/macvm/last-admission-reclaim.json`.
- `start` launches foreground QEMU in `c3r-macos-NAME.service` via `systemd-run --user --slice=c3r-vms.slice --service-type=exec`, no independent daemonization.
- If already inside its exact managed unit, avoids recursive wrapping; direct unit must be Type=exec/simple.
- Preserves unit names compatible with parent’s prior temporary forking service.
- No live VM was stopped for source changes.

Latest lifecycle fix:
`slice_info` checks LoadState loaded and exact hard limits **before** activation, starts a configured inactive static slice after fresh user-manager boot, then rereads and rechecks active state, cgroup and limits. Missing/wrong config never gets started or relaxed.

## Verification already completed

OS/login baseline:
- Fresh `sonoma-login` clone automatically booted with no picker input.
[Credential-related line redacted from this historical copy.]
- Actual VNC desktop login, Finder and Dock passed.
- Real rollback wrote disk and NVRAM markers, cleanly stopped, reset, firmware matched byte-for-byte, rebooted automatically, both markers absent, SSH/sudo passed.
- Evidence under `instances/ready-check/evidence`.
- `snapshot-check`, `ready-check`, `bootstrap-fix` are cleanly stopped, preserved.
- `asset-check` was transport-owned and cleanly stopped.
- External `wayland-host` was stopped once under an earlier parent instruction, then ownership clarified; **never operate it again**.

**Full final synthetic manager suite passed after latest slice activation fix:**
- **15/15**, 18.174 sec.
- Log `/mnt/datasets/macvm/evidence/prepared/manager-tests-final.log`
- Log SHA `42fe1a6c2298e540343ec1013674989b75b77d3c6994e9b4e1703f4eae529b3b`
- Parent accepted, **do not repeat**.
- Tests include real qcow2 clone/reset isolation, interruption/process death, six snapshot commit boundaries, partial overlay preservation, budget/systemd launch, inactive-to-active slice and missing/wrong-config denial.
- Mocked start test prints “Started managed”; it does **not** start a VM.

## Durable assets, fonts, power

Host commands:
```sh
/mnt/datasets/macvm/assets/mount-assets INSTANCE
/mnt/datasets/macvm/assets/generated-fonts/provision-fonts INSTANCE
```

Assets:
- Guest mount `/Users/c3r/civ3-assets`.
- Game source `/Users/c3r/civ3-assets/Conquests`.
- Original host game remains read-only; no game copies.
- Persistent service `c3r-macos-assets-share.service`, Nix runtime GC roots registered.
- Kernel and SMB read-only, authenticated reads/hashes and denied writes tested.
- `mount-assets` probes 15 sec and remounts stale mounts.
- Session idle timeout **86400 sec**. Authenticated read after **310 sec idle** passed beyond old 300 sec bug.
- Source `sonoma-login` predates Remote Login full disk access; work has GUI permission enabled, prepared snapshot will inherit it. No SIP/TCC DB hacks.

Fonts complete:
- Guest `/Users/c3r/civ3r-runtime/fonts`
- **624 generated files, 59,191,251 bytes**
- 208 styles ×224 legacy slots; Windows punctuation mappings validated.
- Native loader **11,232 punctuation bitmap checks** passed.
- Archive `/mnt/datasets/macvm/assets/generated-fonts/generated-font-cache.tar.gz`
- Archive SHA `66eec9ddaf8fe342c4c64fa58aa43c1fa6bca12c3f772352550db7d7b236754a`
- Manifest SHA `be3f4a800637db2ef45906c7d83ad3915b079d1a2e925169f3f98cd1c9cbd28a`
- Guest metadata `font-cache-{manifest,provenance,guest-validation,runtime-validation}.json`.
- Native launcher defaults `CIV3R_FONT_CACHE=Z:\Users\c3r\civ3r-runtime\fonts`.
- Old incomplete cache/archive retained as legacy evidence by transport; guest duplicate removed after success.
- Font helper validates existing files without overwriting, restores absent cache atomically.

Power configured **guest only**, before snapshot:
```sh
sudo -n /usr/bin/pmset -a sleep 0 displaysleep 0 disksleep 0 \
  standby 0 autopoweroff 0 powernap 0 hibernatemode 0
```
System/display/disk sleep already0; additionally disabled hibernation and Power Nap. No host/physical Mac setting changes.
Evidence:
- Guest `/Users/c3r/civ3r-runtime/evidence/guest-power-{before,settings}.txt`
- Host `/mnt/datasets/macvm/evidence/prepared/power-and-share-configuration.json`
- Bounded QA caffeinate may still run; clean guest shutdown removes it.

## Runtime and actual launch context

Guest paths:
- Native Rust launcher `/Users/c3r/civ3r-runtime/bin/civ3-play-macos`
- Viewer `/Users/c3r/civ3r-runtime/bin/civ3r-viewer`
- DLL `/Users/c3r/civ3r-runtime/bin/IFC23.dll`
- Wine `/Users/c3r/civ3r-qa/wine-runtime/runtime/Wine Stable.app/Contents/Resources/wine/bin/wine`
- Prefix `/Users/c3r/civ3r-qa/wine-runtime/prefix-wine11`
- Vulkan runtime `/Users/c3r/civ3r-port/software-vulkan/runtime`
- Source `/Users/c3r/civ3-renderer`
- Rust `~/.cargo/bin`, CLT clang16/SDK installed.

Verified QA launch helper:
`/Users/c3r/civ3r-qa/game-integration/launch-game-qa.sh`

It exports:
```text
CIV3R_HOME=/Users/c3r/civ3r-runtime
CIV3R_VIEWER_BIN=/Users/c3r/civ3r-runtime/bin/civ3r-viewer
CIV3R_WINE=<Wine path above>
WINEPREFIX=<prefix above>
DYLD_LIBRARY_PATH=<Vulkan runtime above>
VK_DRIVER_FILES=<runtime>/vk_swiftshader_icd.json
VK_ICD_FILENAMES=same
WGPU_BACKEND=vulkan
TILE_RENDER_ADAPTER=SwiftShader
TILE_VIEWER_SOFTWARE_PRESENT=1
```
Latest adds RUST_LOG debug defaults. Ends:
```sh
exec /Users/c3r/civ3r-runtime/bin/civ3-play-macos \
  --game /Users/c3r/civ3-assets/Conquests --window "$@"
```

**Aqua session essential**. Plain SSH Wine launch fails graphics security gate despite AppKit working. Login to native desktop first, then guest Terminal helper or exact tested SSH:
```sh
/mnt/datasets/macvm/bin/macos-vm ssh INSTANCE \
 'vm_uid=$(id -u); sudo -n /bin/launchctl asuser "$vm_uid" /usr/bin/sudo -n -u c3r /bin/bash /Users/c3r/civ3r-qa/game-integration/launch-game-qa.sh'
```

QA ownership `/root/pointer_vm_qa`. Coordinate final release through parent, not direct stop while QA owns GUI.

VNC automation pitfall:
- Fresh vncdotool client begins x=y=0.
- Never standalone `vncdo click 1` after separate move invocation.
- Use move+click same session, or QMP button-only.
- Relative pointer QA used USB `usb-mouse` on `xhci.0`, selected HMP mouse_set; all22 native stages passed.
- Default absolute tablet best for ordinary desktop, retained.
[Credential-related line redacted from this historical copy.]
[Credential-related line redacted from this historical copy.]

## Manifests / source checkpoints

Candidate prereq manifest:
`/mnt/datasets/macvm/evidence/prepared/prerequisites.json`,0600.
Captured all basic native health probes successfully, no missing files, but **marked candidate before source freeze** and has older hashes. Refresh after explicit release; do not mistake it for final.
Probes captured:
sw_vers/uname, clang/SDK, rustc/cargo, wine --version, native launcher/viewer --help, power, runtime/dylib/font metadata hashes.

Authoritative parent manifest:
`/mnt/datasets/c3r-build/macos-port/staging/final-runtime-manifest.json`
also staged inside guest runtime. Transport refreshes QA section after final tests.
Its earlier SHA `db759a...` is now obsolete/checkpoint; new viewer/source will change it.

Known shape:
```text
runtime_directory
binaries: name -> {bytes,sha256,guest_path,host_path}
font_cache:
 directory, file_count,total_bytes
 metadata: filename -> {path,bytes,sha256}
 generated_files: [{path:relative-font-name,bytes,sha256}, ...624]
 host_archive
qa_graphics: filename -> {path,bytes,sha256}
source_directory
embedded_source_manifest: {path,bytes,sha256}
production_shader: {path,bytes,sha256}
source_archive: {path,bytes,sha256,guest_path}
source_manifest: {path,bytes,sha256,file_count,git_head}
actual_game_qa, source_staging_verification, native_validation, dll_staging, evidence, scope
```

Host `source-manifest.json` keys:
`git_head`, `files` **dictionary keyed relative path**, `archive` dict.
Last inspected files sample `.gitignore`, `Cargo.lock`, `Cargo.toml`; inspect an entry to see bytes/hash shape before implementing fresh clone exact verification.

Old checkpoint:
- viewer `97ef09d07463695c8fc3c1251538d3c8532709d646adf34ad8435bebc8619881`
- DLL `9e70f26a0556626be0d059f233d49a33e2acf25f98d572f1f1b9ccc31878f432`
- launcher `4bb98ed2b1287af0f7d56fe1982513459e3191b272fa8fba880d9bb894ed028f`
- shader `effdc0cd07fa3d2a89a2692362f9019b2789cf1d8af93c225b38f61674a38a2c`,231644 bytes
- old source archive74a8... **superseded by54aead...**
Latest viewer prefix **2c14e4f4**, full hash from authoritative refreshed manifest.
Do not freeze old hashes.

## Final remaining sequence AFTER explicit parent release

1. Obtain QA’s clean handoff/final manifest/source freeze hash.
[Credential-related line redacted from this historical copy.]
3. Clean stop **work only** through manager.
4. Snapshot:
   ```sh
   /mnt/datasets/macvm/bin/macos-vm snapshot work sonoma-prepared \
     --boot-image OpenCore-apfs-v1.qcow2
   ```
5. **Leave work stopped.** Create disposable `prepared-check`, configure **3072 MiB**, start only this clone. Never stop external wayland-host if it returns.
[Credential-related line redacted from this historical copy.]
7. Mount existing assets using durable host helper; validate denied writes and hashes. Validate full624 fonts through provision helper.
8. Verify compiler/Wine/native binaries/Vulkan libs/source/shader and final manifest hashes on clone. It should inherit all runtime dependencies and Remote Login TCC setting.
9. Launch real game through tested Aqua helper; prove usable native rendering. Parent may provide private saved map/smoke checklist. Do not assume --map semantics.
10. Clean own game by UI or SIGTERM **verified native launcher PID only**, avoid broad Wine kills.
11. Cleanly stop verification clone, preserve disk/history/evidence, then restart `work`.
12. Update private LOGIN status from pending to completed and final host evidence. Public docs frozen already final form; avoid source changes after archive.
13. Send parent concrete snapshot name, successful clone checks, memory status, evidence paths, and actual limitations (x86 VM software Vulkan, no Metal/ARM/physical Mac validation).
14. Parent owns final signed commit/source integration.

Potential metadata verification issue:
- `manager verify` asserts every ancestor not open. External VM may share immutable ancestors; **do not stop it** just to verify.
- Can safely check new frozen top layer and hash/NVRAM/profile plus read-only chain info, without touching active unrelated ancestor disks.

## Tools/runtime convenience

Host has no default `python3`; use:
`/nix/store/b5bpi6zfajzzrwwpgba2q6li3nnya4bs-python3-3.14.7/bin/python3`
or durable manager Bash wrapper falls back to Nix.
QEMU tools in `/run/current-system/sw/bin`.

VNC CLI used:
`/nix/store/cr2zaans66nm1dihaldq2j18chgqqxxs-python3.14-vncdotool-1.2.0/bin/vncdo --force-caps -s 127.0.0.1::PORT ...`

Use `functions.exec` with `tools.exec_command`; collaboration tools called directly. Long `collaboration.wait_agent(timeout_ms=600000)` has been used to avoid busy polling and wakes on parent/agent messages.

Parent wants completed reports, not repeated plans. **Current correct action is continue preparing non-mutating validation details or wait for explicit release, not snapshot/start now.**

## Report at 2026-10-19T08:31:52.361Z

Original transcript line 1847.

I’m checking the VM handoff and snapshot tools. Once the final tests release the VM, I’ll preserve it as a reusable template and verify that a fresh instance boots correctly.

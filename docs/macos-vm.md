# macOS renderer test VMs on Papa

The runtime template is `sonoma-prepared-desktop` in `/mnt/datasets/macvm`. Use `status` to check that a template exists before cloning it. `sonoma-login` is the earlier OS-only baseline; it has verified desktop and SSH login but lacks the finished renderer runtime. Earlier `sonoma-clean` and `sonoma-ready` remain backing ancestors.

The desktop template adds automatic login to the private QA account and an accurate runtime manifest at `/Users/c3r/civ3r-runtime/baseline-runtime-manifest.json`. It contains the accepted Intel x86_64 protocol 38 aggregate from source `3a91e2e9`, not the newer Apple ARM64 releases. The original `sonoma-prepared` remains immutable and boots to the login screen.

The store uses read-only qcow2 templates and small writable instance overlays. Each instance has separate UEFI variables, UUID, MAC address and localhost ports. Cloning does not copy the operating system or game installation.

Actual account credentials and exact connection instructions are in `/mnt/datasets/macvm/LOGIN.md`, mode 0600. This public guide contains no passwords.

## Start a runtime clone

Run on Papa. The standalone manager works without this checkout; the equivalent source entry point is `tools/macos-vm`.

```sh
/mnt/datasets/macvm/bin/macos-vm status
/mnt/datasets/macvm/bin/macos-vm clone rendering-test --template sonoma-prepared-desktop
/mnt/datasets/macvm/bin/macos-vm configure rendering-test --ram-mib 3072
/mnt/datasets/macvm/bin/macos-vm start rendering-test
/mnt/datasets/macvm/bin/macos-vm ssh rendering-test 'sw_vers; uname -m; sudo -n true'
```

The clone uses 3 GiB for the shared testing budget. New instances otherwise default to 6144 MiB. Stop an owned development VM before starting its verification clone. Leave instances owned by other sessions alone. `status` reports running instances and assigned SSH/VNC ports.

Connect a VNC client on Papa to the clone's `127.0.0.1:PORT`. The desktop template logs into the private QA account automatically. Credentials for manual login remain in the private guide. VNC listens only on localhost and has no separate VNC password. SSH uses the private QA key with separate known-host files for each instance. The development instance `work` uses SSH port 22390 and VNC port 5909.

## Mount the existing game and validate fonts

After each guest boot or reset, run on Papa:

```sh
/mnt/datasets/macvm/assets/mount-assets rendering-test
/mnt/datasets/macvm/assets/generated-fonts/provision-fonts rendering-test
```

The mount helper starts the persistent `c3r-macos-assets-share.service` user service, mounts the existing installation at `/Users/c3r/civ3-assets`, checks Readme/font hashes, confirms writes fail and checks the read-only mount flag. The launcher uses `/Users/c3r/civ3-assets/Conquests`. Active network mounts do not survive a disk snapshot, so remount after booting. No game assets are copied.

The prepared runtime includes macOS Remote Login permission for remote users to access network volumes. A clone of the older `sonoma-login` needs Settings > General > Sharing > Remote Login > Allow full disk access for remote users enabled once before mounting over SSH. Share credentials stay in mode 0600 files under `/mnt/datasets/macvm/assets` and pass through SSH input.

The font helper validates the cache already included in the prepared runtime. If the directory is absent, it extracts the verified archive atomically. It never overwrites an existing cache. The cache contains 624 generated files, 59,191,251 bytes, covering 208 styles and all 224 legacy glyph slots with Windows punctuation mappings. The native loader passed 11,232 punctuation bitmap checks.

The guest cache is `/Users/c3r/civ3r-runtime/fonts`. The launcher sets `CIV3R_FONT_CACHE` to its Wine path, `Z:\Users\c3r\civ3r-runtime\fonts`. The durable archive is `/mnt/datasets/macvm/assets/generated-fonts/generated-font-cache.tar.gz`, SHA256 `66eec9ddaf8fe342c4c64fa58aa43c1fa6bca12c3f772352550db7d7b236754a`. `tools/gen-civ3-fonts` generates new sheets when font inputs change. Cache manifests include `font-cache-manifest.json`, `font-cache-provenance.json`, `font-cache-guest-validation.json` and `font-cache-runtime-validation.json`.

## Launch the renderer inside the Windows game's process

Current macOS launchers default to in-process rendering. Wine runs the i686 game and builtin Windows shim in its x86_64 host process, loads the native x86_64 unixlib, and attaches the viewer to Wine's AppKit application on the main thread. The `viewer-gpu` thread owns graphics work. No separate viewer process or viewer socket is needed. Intel execution does not use FEX.

The prepared snapshots preserve the earlier protocol 38 runtime, whose helper at `/Users/c3r/civ3r-qa/game-integration/launch-game-qa.sh` uses a separate viewer. For a current in-process test, first deploy a complete, matched runtime cross-compiled on Papa. The example below uses `/Users/c3r/civ3r-runtime-current` as that installation root; point `qa_runtime` at the actual deployed directory. It does not upgrade the prepared snapshot automatically.

| Component | Path below the current runtime root |
| --- | --- |
| Native Rust launcher | `bin/civ3-play-macos` |
| Native viewer, used by `--separate-viewer` | `bin/civ3r-viewer` |
| i686 Windows bridge | `bin/IFC23.dll` |
| i686 builtin Wine shim | `native/i386-windows/civ3r_native.dll` |
| x86_64 native renderer unixlib | `native/x86_64-unix/civ3r_native.so` |

Deploy all five components from one frozen source revision and scene protocol. The bridge, viewer and native unixlib must embed the same canonical shader. The Windows shim must retain its Wine builtin marker. `CIV3R_NATIVE_DIR` selects the directory containing the two architecture subdirectories. The native unixlib has a Mach-O dylib payload despite its `.so` filename.

Verify that the desktop is logged into the QA account before launching:

```sh
/mnt/datasets/macvm/bin/macos-vm ssh rendering-test \
  'stat -f %Su /dev/console; pgrep -x Finder; pgrep -x Dock'
```

The console owner should be `c3r`. Run the launcher in that Aqua session explicitly. Set the environment after `launchctl asuser` and `sudo` so the dynamic-loader settings reach the Wine-hosted unixlib:

```sh
/mnt/datasets/macvm/bin/macos-vm ssh rendering-test '
  qa_runtime=/Users/c3r/civ3r-runtime-current
  vm_uid=$(id -u)
  sudo -n /bin/launchctl asuser "$vm_uid" /usr/bin/sudo -n -u c3r /usr/bin/env \
    CIV3R_HOME="$qa_runtime" \
    CIV3R_VIEWER_BIN="$qa_runtime/bin/civ3r-viewer" \
    CIV3R_BRIDGE="$qa_runtime/bin/IFC23.dll" \
    CIV3R_NATIVE_DIR="$qa_runtime/native" \
    CIV3R_WINE="/Users/c3r/civ3r-qa/wine-runtime/runtime/Wine Stable.app/Contents/Resources/wine/bin/wine" \
    WINEPREFIX=/Users/c3r/civ3r-qa/wine-runtime/prefix-wine11 \
    CIV3R_FONT_CACHE="Z:\\Users\\c3r\\civ3r-runtime\\fonts" \
    DYLD_LIBRARY_PATH=/Users/c3r/civ3r-port/software-vulkan/runtime \
    VK_DRIVER_FILES=/Users/c3r/civ3r-port/software-vulkan/runtime/vk_swiftshader_icd.json \
    VK_ICD_FILENAMES=/Users/c3r/civ3r-port/software-vulkan/runtime/vk_swiftshader_icd.json \
    WGPU_BACKEND=vulkan TILE_RENDER_ADAPTER=SwiftShader \
    TILE_VIEWER_SOFTWARE_PRESENT=1 CIV3R_PRELOAD=off \
    CIV3R_STARTUP_WINDOW=0 TILE_VIEWER_PROFILE=1 TILE_RENDER_GPU_TIMING=1 \
    CIV3R_PIXEL_AUDIT=on RUST_LOG=tile_viewer=info,civ3r_native=info,tile_render=info \
    "$qa_runtime/bin/civ3-play-macos" \
      --game /Users/c3r/civ3-assets/Conquests --window --in-process
'
```

This diagnostic command disables the startup status window and enables textual frame and pixel-policy diagnostics. It still runs the real game and renderer. The environment and launcher portion can also run directly in a logged-in guest Terminal. The launcher creates a private writable game wrapper under `CIV3R_HOME`, links the existing read-only assets, and uses the existing private Wine prefix. Preserve prefix registry files before testing a different installation path.

Plain SSH can fail Wine display-driver initialization even when a native AppKit window works. Use SSH for deploying Papa-cross-built artifacts, health probes and asset mounting; use the Aqua session for runtime launches. Do not compile on the guest.

SwiftShader supplies CPU Vulkan execution and the viewer presents the resulting bitmap through AppKit. The VM's VMware display has no usable Metal device. This path exercises production shaders and process/thread integration; its frame rate does not establish physical Mac GPU performance. Physical Apple ARM64/Metal testing uses the separate [TS Neo run guide](macos-tsneo.md).

Normal launches suppress the original map, UI and window pixel output. Semantic readbacks remain for hit testing, transparent controls and simulation. `--compare` enables the original renderer alongside the native output. `--compatibility-fallback` permits original-renderer recovery after viewer loss. `--show-game-window` changes window visibility without enabling its pixel output; `--map` enables original map pixels only. `--separate-viewer` selects the separate native viewer process for comparison. An explicit CLI mode overrides `CIV3R_GAME_RENDER`; its default is `off`.

For automated cleanup, send SIGTERM only to the verified native launcher's PID and wait for its owned game to exit. Check its owned Wine debugger descendants as well as the launcher, game and viewer. Earlier launcher cleanup left three owned `winedbg` processes; verify their command lines and working directories before terminating them. Do not kill unrelated Wine or renderer processes. A failure is reported through the launcher status and the logs below `CIV3R_HOME/logs`, plus `civ3-bridge.log` in the private Conquests directory.

The protocol 48 source `31b5951a` established same-process game/shim/unixlib mappings, main-thread AppKit attachment, a GPU worker and shader-frame logs. Its 75 Intel macOS CPU tests and two help probes passed. Those observations do not establish interactive runtime acceptance. Broader GUI inspection found a game Program Error, and all three earlier startup logs contained an unhandled stack overflow. The original shutdown timing also omitted owned Wine debugger descendants. Preserve the historical evidence under `/mnt/datasets/c3r-build/vm-inproc31b5951a-sol/`, but do not treat its earlier pass summary as a successful game session.

Current acceptance requires an actual map load and UI/input interactions, sustained progress without game faults, and cleanup of every verified owned descendant. Fault diagnosis and a matched original-vendor control are in progress under `/mnt/datasets/c3r-build/mac-audio-sol/` and `/mnt/datasets/c3r-build/vm-qa31b-sol/`. The vendor-only title control also stack overflowed about 50 seconds after launch with the same Wine, prefix and USB audio, without our bridge or native modules. Its short backtrace crosses `mmdevapi`, `dsound`, `kernel32` and `ntdll`. The renderer faults also occur at local stack access just after Wine Unixcall returns; the common cause and fix are not yet proven. The launcher has no save-file or game-argument forwarding option.

A guarded, process-local Wine SS-return diagnostic subsequently kept the original-vendor control healthy for more than 246 seconds, then allowed an actual protected save load, city and advisor screens, and normal exit with status 0. A fresh unmodified vendor repeat faulted again before map input. These observations support a Wine return-state compatibility issue, but they are diagnostic evidence rather than acceptance of a shipping fix. Both 240-second DirectSound-only controls passed with 1 MiB and 2 MiB PE stacks. Preserve the comparison, exact memory guards and backtraces under `/mnt/datasets/c3r-build/vm-qa31b-sol/`; do not apply the diagnostic memory patch as part of routine launch instructions.

The prepared snapshot's prerequisite manifest and clone validation remain under `/mnt/datasets/macvm/evidence/prepared/verification-20261020`. Its `baseline-runtime-manifest.json` records the historical Intel protocol 38 runtime; it is distinct from each current runtime's deployment manifest. Font provisioning refreshes its guest validation record, so that record is mutable output. Basic health probes include the native executables' `--help` and `wine --version`; successful probes alone do not establish runtime acceptance.

The original vendor game previously crashed during shutdown when the VM had no audio device. With the standard USB audio device connected to the silent QEMU backend, the same vendor control exited successfully.

## Save or restore a state

Stop the guest normally before taking a new template:

```sh
/mnt/datasets/macvm/bin/macos-vm stop work
/mnt/datasets/macvm/bin/macos-vm snapshot work before-renderer-upgrade \
  --boot-image OpenCore-apfs-v1.qcow2
/mnt/datasets/macvm/bin/macos-vm start work
```

The prepared template includes the validated `usb-null` audio hardware profile. macOS receives a standard USB audio output connected to a silent QEMU backend, without accessing host audio. The original game needs an initialized audio device for clean shutdown. Snapshot, clone and reset preserve this typed audio profile. Instance identity, ports, optional host mappings and arbitrary extra QEMU arguments are not inherited. To select this hardware explicitly on a stopped instance, use `configure INSTANCE --audio usb-null`; `--audio none` removes it.

The snapshot moves the stopped overlay into a new read-only template and creates a fresh writable overlay for the instance. It copies only the small matching UEFI variable file. Existing template names cannot be overwritten. The APFS boot profile automatically selects the installed operating system; new clones need no manual picker selection.

```sh
/mnt/datasets/macvm/bin/macos-vm stop rendering-test
/mnt/datasets/macvm/bin/macos-vm reset rendering-test --to sonoma-prepared-desktop
/mnt/datasets/macvm/bin/macos-vm start rendering-test
```

Reset restores disk and firmware together. It archives the previous pair and manifest under `instances/rendering-test/history/TIMESTAMP`, preserving work. The instance keeps its UUID, MAC, ports and configured RAM.

Snapshot and reset refuse running or otherwise open disks. Snapshot also requires the clean shutdown record written by `stop`. Both operations write recovery journals before moving original files. After an interruption, the next instance command finishes the saved transaction before booting; `macos-vm recover INSTANCE` also does so explicitly. `status` reports pending transactions. After an unclean host or QEMU exit, boot and use `stop` before taking another snapshot.

## Shared memory budget and unattended testing

Manager launches run foreground QEMU in `c3r-vms.slice`. The combined renderer VM hard limit is 10,000,000,000 bytes, with high threshold 9,000,000,000 and swap disabled. The default guest RAM is 6144 MiB; the documented shared testing configuration uses 3072 MiB. Admission counts configured RAM plus a 1 GiB overhead reserve per VM, and checks observed slice usage. A running renderer-owned QEMU outside the slice blocks another renderer launch. Unrelated fleet VMs belong to other host work and are not taken over.

Clean file cache is discounted when no VM is active. If configured reservations fit but clean cache blocks headroom with another active VM, the manager makes one bounded reclaim request of at most 4 GiB with `swappiness=0`, then measures and checks again. It never reclaims to hide configured overcommit or enables swap. It records attempts in `last-admission-reclaim.json`.

Use one test instance at a time for snapshot verification: stop `work`, save the template, leave `work` stopped, start and verify the clone, stop the clone, then restart `work`. A font-generation Windows VM must also fit the same budget or run separately. Do not stop another session's VM to make room.

```sh
systemctl --user show c3r-vms.slice -p MemoryCurrent -p MemoryMax -p MemorySwapMax
```

The persistent slice definition is `/home/sirati/.config/systemd/user/c3r-vms.slice`. Existing optional shared-memory and extra-device settings remain supported; process-ownership overrides and independent daemonization are blocked.

The private guest disables automatic system/display/disk sleep, standby and Power Nap. Verification still reports `hibernatemode 3`; do not assume that setting changed:

```sh
sudo -n /usr/bin/pmset -a sleep 0 displaysleep 0 disksleep 0 \
  standby 0 autopoweroff 0 powernap 0 hibernatemode 0
/usr/bin/pmset -g custom
```

For an exclusive host measurement window, finish the owned game and use a normal guest shutdown or verify that the guest is idle. A live QMP pause followed by resume caused an NMIPI spinlock-timeout kernel panic in `work` on 2026-10-20. The guest rebooted and lost the paused game session. Evidence is `/mnt/datasets/c3r-build/vm-qa31b-sol/resume-kernel.panic`; the VM disk and memory limits were unchanged.

Explicit guest shutdown remains available. No Papa or physical Mac power settings change. The SMB service's idle-session timeout is 24 hours, recorded as `session_idle_timeout_seconds=86400` in `/mnt/datasets/macvm/assets/server-state.json`. An authenticated read passed after 310 seconds idle, beyond the previous 300-second timeout. The mount helper probes for 15 seconds and remounts stale connections.

## Verify or archive the store

```sh
/mnt/datasets/macvm/bin/macos-vm verify
/mnt/datasets/macvm/bin/macos-vm verify rendering-test
```

Stop instances before verifying the backing chains they use. Verification checks qcow2 metadata and parent paths, firmware and OpenCore hashes, and template UEFI hashes. Keep the entire store together when archiving it: overlays use absolute backing paths and descendants depend on their ancestors. No automatic pruning deletes templates or history. The external read-only game installation remains separate.

Boot support, private keys, credentials, durable management tools and service helpers live inside the store. Build output stays on `/mnt/datasets`. Earlier helpers under `/mnt/datasets/c3r-build/macos-port/vm/helpers` remain compatible with `work`; its disk, firmware, QMP and PID paths point into the stable store.

# Civ III renderer handover

The installed Neo release is **Civilization III**, protocol 50, Git commit `41edad8`, source snapshot `b9abc707…`. Its files, signature and registration were verified, and the native renderer window now opens with the renderer inside the game's Wine process (dual stack, no separate viewer): in 3 of 3 launches of the installed app the viewer attached in 8–9 s, the Wine game process owned the on-screen window, and the app stopped cleanly. Installation evidence is in [deployment.json](deployment.json).

The earlier f510 install never showed its window because startup deadlocked inside Wine on Neo. Commit `fee4096` removes the trigger. The loaded map/HUD, input and the in-game Quit on Neo are still unverified, because the agent has no screen-recording or input permission there. See [the scoped verification](verification.md).

Sound and the 240 FPS frame target are low priority P3. Six-bit/seven-bit pixel attribution is P4. They do not block deployment. Keep completed and accepted agents out of this handover.

## What is closed

| Task | Status |
| --- | --- |
| Source recovery and integration | All of this task has been done. |
| Existing source-backup checkpoint | All of this task has been done. |
| Dedicated GPU-worker implementation | All of this task has been done. |
| UI command-transaction implementation | All of this task has been done. |
| Existing frame-counter accounting implementation | All of this task has been done. |
| Narrow Microsoft Basic Render Driver compatibility fix | All of this task has been done. |
| Vendor and title-screen audio causal controls | All of this task has been done. |
| Launcher cancellation regression checks | All of this task has been done. |
| Prepared macOS desktop template | All of this task has been done. |
| Collection and review of the f510 moving 4K baseline | All of this task has been done. |
| 21 October viewer verification | All of this task has been done. |
| Latest coherent release installed on Neo | All of this task has been done. |
| Intel macOS VM native-window check | All of this task has been done. |
| Neo startup deadlock (no native window) | Fixed in `fee4096`, installed with `32cb02c`; window verified 3 of 3 launches. |

## Unfinished work

| Priority | Task | Next action |
| --- | --- | --- |
| P0 | Neo visual acceptance | The window opens. Someone at Neo, or the user after granting screen recording and accessibility, still has to confirm the title UI, a loaded map/HUD, input and the in-game Quit. |
| P1 | Combat camera arrival and speed | Finish the diagnostic build and matched 1x/4x battles with RNG and stable-outcome comparison. |
| P3 | Sound | Fix the loaded-save audio driver and finish its qualified loaded/normal-Quit test and music soak. |
| P3 | CPU busy attribution | Add actual CPU busy timing while preserving the existing counter design. |
| P3 | Native 4K at 240 FPS | Continue controlled thin-view attribution and qualified optimization from the recorded 43–44 FPS baseline. |
| P4 | Six-bit/seven-bit pixel attribution | Continue the strict rounding investigation and private wider-color qualification. |

No agents are active. When later resuming unfinished work, use [the continuation briefs](subagents.md) and respect its priority. Do not automatically start the backlog. Completed implementation and accepted reviews do not need new agents just because a historical checklist contains broader test suggestions.

The [acceptance checklist](acceptance.md) is a reference to the older feature ledger, not a list of active subagents. [Deferred work](deferred-work.md) retains sound, numerical and performance details without making them deployment gates. [handover.html](handover.html) is the offline reading copy.

## Installed release and source

| Item | Current identity |
| --- | --- |
| Neo app | `/Users/sirati/Applications/Civilization III.app`, bundle ID `org.sirati.civ3r`. Native window verified; visual map/HUD/Quit check pending. |
| Installed at | 22 October 2026, 16:44:49 UTC. Rollback backup `runtime/releases/app-backup-20261022T164448336184Z`. |
| Installed source | Git commit `41edad8ce53211ad08f3069b15bc59df14d4b4be`, snapshot `b9abc70760ae5aaa4c110aec5e79ad91ac12628ce6087a3918f71619d2f1c873`, protocol 50. |
| Implementation baseline | Git commit `8106ae5a4debbe247cb5e64523d4972474f4fe39` (f510 snapshot, 673 entries), plus code commits `fee4096` (Neo startup deadlock) and `32cb02c` (Wayland redraw freeze). Other handover commits change documentation only. |
| Primary shader | `43961a92338f6b5b1f5ebd12dfdc806057fe43d8bfeb50abdb7872d274b601c4`. |
| Fallback shader | `bbde10b5e91a3e54c337eab67c2e5acce8abf384657043eb46bbe056ea799281`. |
| Private source | Development snapshot `6162a7debb8adcc78eac147611eeb1539c7e78c8`; loaded-audio QA checkpoint `ce5e104270258e207e16c3a8cf7a0ff5336c41d5`. Restore from the separate development backup. |
| Origin | `/home/sirati/Projects/civ3-renderer-old` is missing. Use the verified off-host Git backup fallback. |

The five runtime files are `civ3-play-macos`, `civ3r-viewer`, `IFC23.dll`, `aarch64-unix/civ3r_native.so` and `i386-windows/civ3r_native.dll`. Keep them paired with the app starter and bundle artifacts. All five installed hashes, the starter and the app signature were independently checked after installation. Launch checks are recorded separately; no gameplay acceptance is inferred.

The [restore guide](backup-and-restore.md) covers main/private/FEX backups and worktrees. [state.json](state.json) gives machine-readable identities and priorities. [Verification](verification.md) records existing tests and their scope.

## Constraints to carry forward

- Ship Rust, isolate unsafe interop and add no production Rust locks. Use the common pointer API, channels/actors and `civ3r-sync`.
- Keep game world/UI/text drawing on the GPU; CPU state/layout and an independent test oracle remain appropriate.
- Preserve the user's counter design and three small control buffers. Do not triple-buffer the world or atlas.
- GPU work uses the AMD RADV iGPU, historically PCI `0000:72:00.0`, vendor `1002`, device `13c0`. Do not select NVIDIA for diagnostics.
- Cross-build on Papa, never on physical Neo. Use `nix develop`, `CARGO_INCREMENTAL=0` and owned targets under `/mnt/datasets/c3r-build`.
- Reuse existing/read-only game assets. Do not copy large asset trees or touch Nexus.
- Renderer VMs share `c3r-vms.slice` with a combined 10 GB cap. The macOS `work` VM uses SSH port 22390. Never operate `wayland-host`.
- Reserve one owner for VM GUI/input and one for AMD measurements. Do not reuse historical leases. Sol GUI/screenshot work is authorized.
- The user authorized the latest Neo deployment. Old audio, wider-color and FPS deployment holds are superseded. Do not auto-launch the game during source backup or documentation work.
- Neo is reachable again. Launch tests there must run inside the logged-in GUI session (`open`, or a Terminal `.command`); runs started over SSH cannot reach WindowServer and do not represent the user's launch. Do not change its privacy permissions without the user.
- Commit work and use a working origin, or replicate the Git tree to Papa's laptop and Neo before source cleanup.

## Next session

1. Check the current Git tree, installed release and process/VM ownership. Keep the installed source and artifacts coherent.
2. Continue only work that is still unfinished, using its priority and fresh-agent brief. Do not recreate completed/accepted agents or start P3/P4 diagnostics as installation prerequisites.
3. Preserve commits and off-host backups. Review concrete user feedback separately from the historical acceptance ledger.

Useful guides are [macOS Neo](../macos-tsneo.md), [macOS VM](../macos-vm.md), [concurrency](../concurrency.md), [settings](../settings-transactions.md), [independent camera](../independent-camera.md) and [modal input](../modal-input.md). Credentials remain in local environment-owned records and are excluded here.

# Native failed-audio cleanup, Sol handover

The native macOS Wine Quit crash was independently reproduced with the original vendor IFC23 and no Rust renderer. Startup initializes MIDI successfully, Wave initialization returns3, and the original failed-Wave path unloads sound.dll while retaining the MIDI owner. CRT destruction later calls that stale object's invalid vtable. The early LLDB attempt perturbed WoW execution and is excluded from accepted lifetime evidence.

A separate, debugger-free, observation-only Rust IFC23 shim proved the full lifecycle in one run. MIDI returned0, Wave returned3, the exact relocated sound.dll MIDI vtable/shutdown/delete targets were valid before original unload, and the same MIDI owner was unreadable after unload. The strict read-only verifier accepted this evidence.

- Native game SHA256: `66d0ae91de6f4a2376e03aa74eb29a8256f02108ca3a4c024a4857337bc45ee6`.
- Vendor control: `/mnt/datasets/c3r-build/vm-qab41-sol/audio-vendor-small-evidence`.
- Debugger-free observation: `/mnt/datasets/c3r-build/vm-qab41-sol/audio-observe.json`.
- Accepted lifetime report: `/mnt/datasets/c3r-build/quit-audio-sol/observer-build1/runtime-lifetime-proof.json`.
- Observation DLL SHA256: `a850200e3c695e83ec72d5f9edd38cf57c9660cfe385d13b479482d990424d48`. It is diagnostic only and must not ship.

The final causal candidate is `/mnt/datasets/c3r-build/quit-audio-sol/audio-fix-proposal3/audio-init.patch`. It adds `civ3-sys/audio_init.rs`, its module export, synchronous bridge early installation/deferred status log, and one on-demand trace comment. The only native patch is failed-Wave callsite5F9B7D. It calls original native MIDI close5F9770 while the module is live, then original unload5F9D00, forwarding its return unchanged. The original caller still preserves Wave's failure in nonvolatileESI. Native close performs original shutdown/delete/ownership clearing. Ordinary valid-audio startup and shutdown retain their original callsites.

Installation checks supported image/build and complete caller/close/delete/unload bodies. It requires clean zero init/module/all three backend owners/all eleven factory slots, declining late active/stale/unreadable audio state. It adds no destructor guard, forced success, music disable, pointer-zero shortcut, new waiting lock, new unsafe, per-frame work or periodic audio reads.

A bounded first-completion record is immutable after publication. Later unwinds only increase the total count. An existing `civ3-trace.request` in Conquests cwd produces `civ3-trace.txt` and removes the request; title/menu fallback works without a map. Its compatible comment can prove actual corrected-branch execution:

`# native_audio Diagnostics { installation: Installed, unwind_count: 1, first_midi_close_result: Some(0), first_cleanup_result: Some(0), initialized: Some(0), module: Some(0), midi_backend: Some(0), master_backend: Some(0), wave_in_backend: Some(0), factories_cleared: Some(true) }`

None means unavailable/unreadable. A count may exceed1. Healthy audio can show count0 with live module/owners; a Quit pass alone does not prove the corrected failure branch executed.

Root reviewed final source3 and independent startup_progress_sol review found no blocking source issue. Twelve CPU tests passed, including lifetime/order, original stale regression, no-MIDI no-op, original result forwarding, image/late-state/byte/ABI gates, completed-only publication, unreadable diagnostic state and first completed record surviving nested/later completions. Offline locked jobs1 Windows i686 checks passed for both civ3-sys and civ3-bridge. No GPU or runtime test is claimed by these compiler gates.

- Final validation: `/mnt/datasets/c3r-build/quit-audio-sol/audio-fix-build3/validation.json`.
- Final immutable source: `/mnt/datasets/c3r-build/quit-audio-sol/audio-fix-build3/source`.
- Exact four leaves: `/mnt/datasets/c3r-build/quit-audio-sol/audio-fix-proposal3/source-identities.json`.
- Module SHA256: `6785d365402588b4704799fafaf3047103f0bcb6dda559ed8b7eece1942bd92e`.
- Independent review: `/mnt/datasets/c3r-build/handover/audio-fix3-independent-review.startup.sol.json`.
- Before/after source manifests prove the compiled snapshot was unchanged.

Earlier source1 passed8 CPU tests but its bridge check hit an omitted small repository asset descriptor fixture. That incomplete run is preserved separately in audio-fix-build1, with packaging-failure-note.json; the fresh final source3 includes those descriptors and passes both checks.

No shared source was edited by this audio task. Root owns promotion of the exact four reviewed leaves and freezing/building the production successor. VM owner alone runs actual production-PE acceptance under broker runtime grant: prove Installed/count>=1/close0/unload0/cleared native owners, then ordinary title Quit0 and loaded-game Quit0, without debugger or observer DLL. Healthy-audio startup/shutdown remains an additional runtime check. Physical Neo remains unavailable. Compiler lease was explicitly released after final passing gates.

## Later exact-production startup arm

Root promoted/froze/built the reviewed successor and VM owner attempted exact production title acceptance. This arm stalled before Wave initialization returned, so the corrected unwind had not run. Its on-demand trace showed Installed/count0, initialized0, live module/master/MIDI and populated factories. All321 sampled game-thread stacks waited inside native CoreAudio AudioOutputUnitStart -> device-start/HAL start-state wait. The separate CoreAudio IO client thread waited in mach_msg. Guest memory evidence was68% free with48.25MiB swap, not a demonstrated RAM-pressure stall. The eight-minute arm was preserved as incomplete; strict owned cleanup produced app137 and is not ordinary Quit acceptance.

Read-only setup evidence confirms the existing supported USB model and AppleUSBAudio driver are present and matched. No missing-driver or newly disabled-device conclusion is justified by the endpoint's historical Audio Output - Disabled name. Existing Wine11 source calls AudioOutputUnitStart synchronously during stream creation and documents that it can take a while. This stall precedes the new compatibility failure branch and does not invalidate the earlier debugger-free valid-before/stale-after proof.

Evidence is `/mnt/datasets/c3r-build/vm-audio-production-sol/title-A-startup-evidence.tar`, plus `audio-setup` under that directory. Detailed read-only diagnosis and proposed service/native-control/reboot sequence is `/mnt/datasets/c3r-build/quit-audio-sol/hal-stall-setup-proposal.md`. Root subsequently authorized VM owner alone to restart the existing guest launchd coreaudiod service and run bounded stock afplay, with one normal clean guest reboot if native audio remains stalled. At this handover update, those results are still pending. No VM setup, RAM, device, service, runtime or source mutation was made by this diagnosis agent.

## Healthy recovery and proposed no-device qualification

The guest's normal clean reboot restored stock native afplay to exit0 in2.678seconds, and a repeated native control also passed. The kickstart attempt had actually been denied Operation not permitted, so it never restarted coreaudiod; preserve that correction. Current production title-D reports Installed/init1/count0 with live module/MIDI/master, a healthy-audio control. It does not execute or qualify the failed-Wave compatibility branch. Healthy title/loaded controls remain VM-owner work.

Root supports a separately labeled supported audio_profile none regression after healthy loaded acceptance, with a fresh matched original-vendor no-device title control before production no-device title+loaded arms, then restoration to usb-null and native afplay0. The plan is `/mnt/datasets/c3r-build/quit-audio-sol/no-device-qualification-plan.md`. No variant operation was performed by this planning agent, and the VM owner is preparing concrete preflight for root review.

Historical `/mnt/datasets/c3r-build/macos-port/macos-game-live/audio-vendor-control-report.json` records the same original66d game/sound/IFC crashing at005F8CBF/exit5 without guest audio, then normal exit0 after adding USB-null. Static sound.dll MIDI-init thunk1000126C points to10009E90, which skips optional midiOutOpen when its own flags&2 is clear. The accepted observation used MIDIflags0/result0 and Waveflags2/error3. This supports a real no-output-endpoint condition as a plausible causal regression, not an assumed outcome for the current production binary.

The complete fresh production oracle remains mandatory for each accepted no-device arm: Installed/count>0/first close0/first unload0/cleared init,module,MIDI,master,wave-in,factories and actual normal Quit0, with no observer/debugger/return forcing. No-device native playback is expected to fail naturally rather than satisfy afplay0; all nonaudio preflight gates, including read-only SMB after reboot, remain mandatory. Restore the original USB profile, argv, RAM/cgroup/settings/artifacts and positive native playback afterward. A count0, unexpected endpoint, native hang or missing field stays inconclusive and must not be relabeled a causal pass.

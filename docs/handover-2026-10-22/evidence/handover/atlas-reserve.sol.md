# Atlas reservation storm and live Neo audio evidence

The urgent successor change is in `crates/tile-viewer/src/atlas_reservation.rs`, `lib.rs` ArtInbox, and `render_worker.rs` receive_art plus CPU tests. The small production state tests passed 4/4; targeted full-viewer CPU tests are compiling in a broker-approved jobs1 lane. No Neo configuration, GUI, stop or relaunch operation was performed by this agent. The FPS broker exclusively owns the user-authorized emergency capture/stop.

## Proven current failure

The actual Neo session log repeatedly prepares identical room for 1 colour and 147/148 index pages, many times per second, taking 21–116 ms. One optional growth took 8,892 ms across 933 frames. Current sprite metadata is about 139k sprites. Source evidence explains why unchanged capacity still costs work: the reserve transfer clones all sprite/palette vectors on begin and again on commit, swaps atlas/bind-group state, submits an empty completion fence and rebuilds its bind group.

The producer replay is `State::receive_art`: every immutable ArtSnapshot repeats its reserve, which was sent into Assets' mailbox unconditionally. The consumer `ArtInbox::reserve_more` retained maxima only until `take_reserve` cleared the pending request. A repeated snapshot therefore became a new identical transfer every frame.

The fix keeps monotonic requested room for each whole atlas epoch after consumption. A taken request remains owned by PendingArt through in-flight work, invalidation/retry or terminal failure. New epochs, including replacement with a reused numeric generation, reset the request record. New GPU actors start with a fresh record for recovery. The worker also admits snapshot room directly into its local inbox, avoiding duplicate internal mailbox messages entirely. Higher requests preserve both capacity axes; identical/lower requests do nothing. GPU memory limits, accounting, transfer budgets, fences and cancellation are unchanged.

Focused tests cover repeated packets, in-flight retry/failure, replacement, stale packets, recovery, axis growth and integer bounds. Existing pure transfer-room budget tests will also be run. No GPU test is included in this CPU lane.

## Evidence locations

- Current live capture: `/mnt/datasets/c3r-build/perf240-sol/evidence/neo-live-3be/`
- Immutable local source: `/mnt/datasets/c3r-build/atlas-reserve-sol/source1`
- Initial source identities: `/mnt/datasets/c3r-build/atlas-reserve-sol/source-identities1.json`
- CPU log: `/mnt/datasets/c3r-build/atlas-reserve-sol/cpu-tests1.log`

The current one-second/10-ms native sample shows the dsound mixer waiting in 77/78 samples and CoreAudio IO waiting in all 78. The CoreAudio timer thread mostly sleeps and still signals events. It does not show an x87 decoder-hot thread or audio waiting behind a renderer/publisher lock. The main UI preparation dominates the sampled main thread; text_perf_sol owns that independent fix. The reserve storm is concrete allocation/copy churn, but this capture does not prove it caused the audible music loop.

A two-name process environment query found no FEX_X87REDUCEDPRECISION or HADRON_HARDWARE_TSO markers. Wine rewrites its process environment, so absence does not establish disabled settings. No broad environment dump was retained. No disable-music workaround or speculative audio hook has been added.

## Assigned follow-on preload work

The user wants idle capacity to prepare scenario/UI assets before first opening a screen, within existing reservations/caps and without publisher/UI waits. Diagnosis so far:

- Existing planning reads every unit/era animation header before returning any interface work.
- Every animation of visible/present units is ahead of the interface tier.
- Budget0/off still starts the expensive planner.
- The interface manifest was captured from one old Greece game, so scenario-dependent screen/Pedia assets can remain cold.
- Optional screen insertion currently omits the indexed-art set even though the planner counts it.

No preload feature edit has been made while the emergency takes priority. Preserve army_units' optimization that skips upload-status queries when planning is absent and optional preload is finished/off. Use the existing ArtFolders generation token for cancellation. Coordinate with preview_smear before touching preview/title generations, and with scenario_crash for source-region bounds. Never load the whole installation, copy game assets, bypass memory caps or add CPU rendering.

## Completed CPU validation

The corrected immutable source2 finished with exit 0 and all 18 selected CPU tests passed. This includes 9 ArtInbox queue tests, 4 monotonic reservation-state tests, the repeated-snapshot, inflight/retry/failure/recovery and independent-replacement worker tests, and both existing peak/cross-atlas memory budget tests. The script selected no GPU tests. Source2 incorporates only the already-existing shared `crate::ModalStack` scene qualification over source1. Each immutable source has its own target.

- Final CPU script: `/mnt/datasets/c3r-build/atlas-reserve-sol/run-cpu2.sh`
- Final CPU log: `/mnt/datasets/c3r-build/atlas-reserve-sol/cpu-tests2.log`
- Final source identities: `/mnt/datasets/c3r-build/atlas-reserve-sol/source-identities2.json`

The broker's CPU lane was released after completion. Performance claims still require the successor paired runtime and the hardware-specific FPS run; these CPU tests establish deduplication, epoch behavior and unchanged budget logic only.

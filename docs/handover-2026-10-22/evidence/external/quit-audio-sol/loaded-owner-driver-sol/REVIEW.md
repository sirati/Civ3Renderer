The package now includes an executable owner adapter. No module contacts the VM on import. No VM command, runtime hash, capture, launch, profile change, compiler, or GPU work was performed during preparation.

Twelve offline tests pass. They exercise the controller, positive archived screenshots, five busy/partial/failure negative screenshots, stale-trace rejection, single request persistence, fresh bridge adoption ordering, the final reserve, normal Quit order, and radio selection rejection. Source is parsed without producing bytecode. The tests do not establish runtime acceptance.

The qualification entrypoint requires a root review JSON with matching run_id and mode, a broker lease JSON with matching run_id, owner vm_snapshot_sol and future expires_utc, a fresh evidence directory, and the separately calibrated Yes-selected screenshot. These are authorization records, not success booleans. Runtime observations derive from captures, current-run bridge deltas, trace file metadata, actual exit status, process environments, and the independent watcher's result. No manual success switches exist.

Use this Python with Pillow, without a build:

```
export PYTHONPATH=/nix/store/k5hjvcdk83wpvpg3zzi8i449k0wzgywm-python3.13-pillow-12.1.0/lib/python3.13/site-packages
PY=/nix/store/0r6k8xa2kgqyp3r4v2w7yrb80ma2iawm-python3-3.13.12/bin/python3
"$PY" -B -m unittest -v
```

Before qualification, the sole owner must stage the already reviewed no-device fixture/config marker and restore the reviewed environment. This package deliberately has no audio profile transition. It pins the original root-approved watchdog f7a21ea5 and preflight c9796a6c without changing either source. The existing preflight checks guest runtime pins, effective save alias, lean config and hardware/process absence. The new runner records the actual shell wait status. All prior trace/request/exit/watchdog evidence is renamed into a fresh immutable per-run archive only after a conservative quiet-process scan. No backup or source is deleted.

The host monotonic T0 is sampled immediately before independent guest watchdog launch. This is a conservative lower bound on actual guest T0. The start receipt retains host begin/receipt times and guest PID. It never subtracts monotonic times from separate machines. Both the controller and every subprocess use the unchanged T0+480 deadline. At T0+420, missing loaded evidence permanently classifies the arm incomplete. Only visible, known normal Exit/Quit states may then advance. The independently launched approved watchdog survives driver failure and owns cap cleanup. The adapter never signals processes.

The adapter captures at intervals of at least two seconds, retains all frames and classification scores, and journals actual command begin/end UTC and monotonic times. A capture's command record supplies its actual timestamp. Every click uses the validated move/pause/held-button helper sequence. Each next click requires a fresh recognized state. Unknown save rows, busy screens, shifted viewports and ambiguous images fail closed. Filename editing and focus workarounds are absent.

The fixed 1280x800 classifier compares independent cropped regions with a maximum mean absolute RGB error of five per patch. File recognition includes the selected red qa-mal3.SAV row and filename field. Summary recognition includes Emperor/Romans/1050 BC. Map recognition requires independent HUD/minimap/control and three unit-body patches. It additionally requires current-run PE ifc23::live adoption and subsequent all-visible messages, not GPU-upload messages. The fresh trace must appear after one exclusive request, disappear from the request path, remain stable while read, and arrive within five host seconds. Native diagnostics and known 80x80/seed/player/Roman identity are required. Unit trace records are not required. Root must inspect the stored post-adoption full image before accepting any candidate pass.

Minimal separate calibration arm, requiring its own root review and broker grant:

1. Begin from an already loaded healthy Mal3 map with full HUD in the unchanged viewport, and open ordinary Quit through the normal Escape action. Qualification has not begun.
2. Run the calibration subcommand below. It first captures and recognizes the ordinary Quit modal, holds the Yes-immediately click, then captures the resulting radio state. It requires the cyan center at Yes and its absence at No before pressing the normal Accept button. It never launches or loads, requests a trace, changes a profile, signals a process, or claims qualification.
3. Root visually reviews quit-before.png and quit-yes-selected.png, including viewport and radio selection. The final app status remains the existing owner wrapper's actual status, to inspect separately. The candidate template may then be referenced by a separately reviewed qualification arm. A missing or rejected selected template prevents qualification startup.

```
"$PY" -B qualify.py calibrate-selected-quit \
  --run-id sol-production-f510-nodevice-CALIBRATION_TOKEN \
  --evidence /ABSOLUTE/FRESH_CALIBRATION_DIR \
  --root-review /ABSOLUTE/ROOT_CALIBRATION_REVIEW.json \
  --lease /ABSOLUTE/BROKER_CALIBRATION_LEASE.json
```

After calibration review and the source commit/replication checkpoint, the reviewed qualification command is:

```
"$PY" -B qualify.py qualify \
  --run-id sol-production-f510-nodevice-QUALIFICATION_TOKEN \
  --evidence /ABSOLUTE/FRESH_QUALIFICATION_DIR \
  --root-review /ABSOLUTE/ROOT_QUALIFICATION_REVIEW.json \
  --lease /ABSOLUTE/BROKER_QUALIFICATION_LEASE.json \
  --selected-template /ABSOLUTE/REVIEWED_CALIBRATION_DIR/quit-yes-selected.png
```

Tokens must contain only letters, digits and hyphens. No command in this document is runtime authorization. The selected-radio calibration remains the only necessary live image prerequisite. Archived gates can reject a future valid but different animation/layout; that yields incomplete evidence rather than weakening the criterion. A successful controller result remains a candidate with qualification_accepted false until root's full-image review. USB restoration remains the separately reviewed owner procedure after any outcome.

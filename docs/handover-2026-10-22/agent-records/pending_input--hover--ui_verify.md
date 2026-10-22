# Historical agent /root/pending_input/hover/ui_verify

This is a recovered historical report, not a live agent handle or a new instruction.

Compared context rows, hover/press states and Preferences tabs against game/mirror images.

Current disposition: Context rows exact; small menu tooltip/reference differences retained; not isolated performance.

Session `01a14bc3-6991-7f33-beea-671566f2dfdb`, parent session `01a14bbf-6b14-7930-92b6-18f3ed16a93f`, depth 3. Started 2026-10-17T21:27:40.180Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-27-40-01a14bc3-6991-7f33-beea-671566f2dfdb.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T21:28:21.563Z

Original transcript line 44.

Existing captures confirm context rows use flat cream normal fills, flat orange hover fills, and text. No sprite or gradient appears within either enabled row.

The viewer's own variant showed no feedback on either row. `1g.log` records exactly zero changed pixels against normal. The Terrain Info mirrored reference did turn orange. The Civilopedia mirror stayed normal while the game's own capture turned orange, so its reported zero difference was a stale-reference match.

These old captures establish the previous failure, but cannot establish whether the recovered build still fails. Fresh checks must distinguish missing state from stale captures.

Prepared, without running UI/GPU work:

- `tools/recover-ident.py`, syntax checked, requires `HOVER_ROOT_SLOT_APPROVED=1` and `recover-*` names. Saves full paired captures alongside crops.
- `recover-ui-plan.txt`, covering context diagnosis, native/in-process button smoke, and geometry checks after HiDPI changes.

Old scripts assume two 1920×1080 outputs at scale 1. Coordinates must be checked against fresh screenshots before reuse. No sessions started or altered.

## Report at 2026-10-17T21:34:52.422Z

Original transcript line 65.

Compatibility checked read-only. Native startup preserves explicit `CIV3R_GAME_RENDER=on`, keeps the game window shown, and accepts the requested diagnostic variables through `XENV`. The hold-file path matches `recover-ident.py`.

Updated `/mnt/datasets/winvm/civ3r-runs/hover/recover-ui-plan.txt` with the exact environment, quoting requirements, context-log checks, and log preservation before startup truncates `run.log`.

No sessions started or UI/GPU commands executed.

## Report at 2026-10-17T21:38:12.186Z

Original transcript line 202.

Private native and in-process sessions stopped. `mygames` reports no remaining processes in either hover prefix.

Current-build findings:

| Check | Own changed pixels | Own vs mirror | Own vs game window |
|---|---:|---:|---:|
| Terrain Info, y535 | 0 | 3605 | 3241/4184 |
| Civilopedia, y595 | 3742 | 0 | 0/4374 |
| Civilopedia, y601 | 0 | 0, stale mirror | 2468/3235 |

Civilopedia at y595 visually matches the game's orange hover fill and text exactly. At y601, the game's own row hovers while our detected rectangle excludes the pointer. Terrain Info remains cream despite hovering inside its detected rectangle.

Preserved full captures/crops under `shots/recover-ctx-*` and bridge/viewer logs under `logs/recover-context-*-final.log`. No latency measurements or subsequent smoke runs performed before rebuild.

## Report at 2026-10-17T21:49:09.453Z

Original transcript line 467.

Fixed build passes the context checks. Both private sessions are stopped; neither hover prefix has remaining game processes.

Context rows now match pixel for pixel against the mirrored game and its own window:

| Native context check | Own changed pixels | Differences vs either reference |
|---|---:|---:|
| Terrain Info, y535 | 3834 | 0 |
| Terrain Info bottom, y541 | 3834 | 0 |
| Civilopedia, y601 | 3742 | 0 |
| Civilopedia bottom, y609 | 3742 | 0 |

Orange fills and text match visually, including the previously missed bottom pixels.

Native and in-process smoke results:

- Unit-order hover and held press match both references exactly. Own changes were 649 and 669 pixels.
- HUD Menu held press matches the mirror exactly. Native also matches the game window exactly. In-process has four differing pixels against the game window.
- HUD Menu hover differs from the mirror only along the tooltip strip at y42–43, 74 pixels. In-process additionally has six differing pixels against the game window within y10–41.
- Popup OK, Load Game and Retire hover match both references exactly in both modes. Own changes were 309, 993 and 873 pixels. No destructive rows were pressed.
- Native Menu and unit controls returned to their exact normal pixels before the subsequent test.

Own Preferences tabs are accessible in both modes. Resolution and Layout turn orange on hover, blue when selected, and display their corresponding content. Returning to CivIII restores the game preferences. Tab hover changed 2906 and 2977 pixels; selecting changed another 3360 pixels per tab. All five captured tab states match between native and in-process across the entire Preferences panel.

Shared-machine hoverwalk observations, not isolated performance measurements:

| Mode/state | Own median | Driver median |
|---|---:|---:|
| Native hover | 85.1 ms, n15 | 178.1 ms, n11 |
| Native normal | 90.8 ms, n11 | 177.0 ms, n11 |
| In-process hover | 34.9 ms, n18 | 270.6 ms, n14 |
| In-process normal | 34.7 ms, n14 | 287.6 ms, n14 |

Artifacts remain under `/mnt/datasets/winvm/civ3r-runs/hover/`, with `recover-fixed-*` screenshots, identity output, latency output, and saved bridge/viewer logs. No repository edits or builds performed.

# Historical agent /root/performance/live_tester

This is a recovered historical report, not a live agent handle or a new instruction.

Measured recovered source panning/advisor/drag workloads.

Current disposition: Some rates exceed 240; p99 gaps and advisor rates leave steady240 unproven.

Session `01a14bbf-4bae-7ea1-b579-3174291ee9e6`, parent session `01a14bbe-20e1-73a0-9309-79642739e53b`, depth 2. Started 2026-10-17T21:23:10.385Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-23-10-01a14bbf-4bae-7ea1-b579-3174291ee9e6.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T00:11:50.786Z

Original transcript line 926.

Ready for the complete native→IP proof on `68b75062e5c6e363b27d9587302d001d6f7d7042`. The functional runner accepts the supplied source manifest and pauses for fresh-title visual review before input.

Prior `374dd45` evidence is preserved. No launch until artifact handoff and functional GO.

## Report at 2026-10-18T00:18:32.270Z

Original transcript line 1023.

Native and in-process both passed on `68b75062e5c6` at 1920×1080 on the AMD iGPU:

- Fresh title labels and copyright apostrophe appeared before game input.
- Real World Setup and Player Setup rendered complete text and controls.
- No generated missing-art diagnostic occurred.
- No throughput instrumentation ran.

All owned sessions/processes are stopped; port 5987 and runtime directories are clear. Source remains clean and unchanged.

[Combined visual review and shutdown evidence](/mnt/datasets/c3r-build/perf-scratch/functional-68b75062e5c6-live-review.json)

## Report at 2026-10-18T00:43:06.679Z

Original transcript line 1080.

## Role and task

You are child agent `/root/performance/live_tester`, reporting to `/root/performance`. Own scratch live-performance/functional-QA scripts and screenshot verification. Do not edit repository source or build artifacts. Parent prefers no routine acknowledgments/progress, only results or blockers.

Original overall user request was to recover a Claude transcript and continue its work, including subagents. Parent handles that broader task.

## Current authorization

**No GPU, Wine, VM, live session, or benchmark launch is currently authorized.**

Candidate `68b75062e5c6e363b27d9587302d001d6f7d7042` was invalidated after root found native 4K with normal game drawing OFF loses the baked title background/logo. Drawing ON made it complete. Root's startup child is fixing early generic UI/capture.

Root conditionally reserved our functional window after a new signed CPU/i686-checked candidate, but **new source/artifacts and explicit functional GO have not arrived**.

Current task is preparation only, completed but not yet reported in final:
- Extend functional proof to native 1080, IP 1080, native 4K.
- Native 4K must use actual 3840×2160 output, compositor scale 2, `CIV3R_SCALE=2`, Auto pixel scale 2, internal 1920×1080.
- Scale physical pointer inputs correctly.
- All runs use normal game drawing OFF, no hover/repaint workaround.
- Fresh title evidence must include full baked background/logo and glyphs before input.
- Preserve all 68 evidence and benchmark scripts/pins.

## Files owned and current changes

Scratch root `/mnt/datasets/c3r-build/perf-scratch`.

### `functional-menu-qa.sh`

Independent guarded functional launcher. Requires:
- `QA_FUNCTIONAL_GO=1`
- `QA_COMMIT=<exact new 40-char SHA>`
- `QA_DIST=<root-associated distribution with source-manifest.json>`
- `LD_LIBRARY_PATH` from `nix develop`
- source checkout `/mnt/datasets/c3r-wt/perf` exactly matching SHA and clean
- runtime manifest hashes/tree association

Now supports arguments:
- `native` or `native 1920x1080`
- `ip` or `ip 1920x1080`
- `native 3840x2160`

IP4K deliberately unsupported and not required.

New 4K behavior:
- output `3840x2160`, compositor `scale 2`
- `CIV3R_SCALE=2`, `CIV3_SESSION_SCALE=2`
- settings `pixel_scale=0` Auto, expected effective 2, internal 1920×1080
- separate private prefix `/mnt/datasets/winvm/wine-civ3-perf-4k`
- prefix created by reflink from native perf prefix **only during authorized run**
- 4K prefix does not currently exist

Other private prefixes:
- native `/mnt/datasets/winvm/wine-civ3-perf`
- IP `/mnt/datasets/winvm/wine-civ3-perf-ip`

Resources:
- port 5987
- unique `wayland-c3r-functional-$$` socket
- private wayvnc control socket, wayvnc `-r`, grim `-c`
- locks `final-gpu-window.lock` and `live-resume.lock`
- cleanup kills only private Wine/session resources

Always drawing OFF:
`CIV3R_GAME_MAP=off CIV3R_GAME_RENDER=off CIV3R_GAME_WINDOW=hide`.
FPS false, all throughput/profile/frame-time/GPU timing variables unset.
`CIV3R_TEXT_TRACE=1` retained.
Default preload restored by unsetting `CIV3R_PRELOAD` and `CIV3R_PRELOAD_MIB`.

Synchronously archives/removes old bridge log before startup and records `launch-provenance.json`, preventing passive snapshots from reading stale logs.

Writes `functional-config.json` recording output, scales, expected canvas, private prefix.

Blacklists superseded source SHAs 347,584,374,68 for this next follow-up.

### `functional-menu-driver.py`

Passively captures at 2,4,8,10 seconds before any game input or repaint request.
Preserves screenshots, complete logs and extracted text traces.

Checks:
- current bridge-log Unix time matches new launch provenance
- title labels New Game/Load Game/Preferences/Exit exist in mirror
- raster or valid whole-run outline fallback is allowed
- TextOutA hook logged before first game font and first positive mirrored text
- game drawing OFF confirmed in log
- actual canvas/window/pixel-scale log matches selected case
- for4K must explicitly log Auto ×2
- physical PNG dimensions match output
- compositor current mode and scale match request

**Pauses for agent visual review before any game input.**
Prints:
`TITLE_REVIEW_REQUIRED /path/to/run`
and waits for `/path/to/run/title-visual-release` JSON requiring all true:
```json
{
  "baked_title_background_complete": true,
  "logo_visible": true,
  "labels_visible": true,
  "copyright_apostrophe_correct": true
}
```
Agent must inspect full fresh screenshot and copyright crop. If background/logo missing, reject, do not repair with hover/repaint, preserve and stop.

Pointer conversion now uses:
`int((logical_coordinate + 0.5) * physical_input_scale)`.
Verified examples:
-1080 move708,498 →708,498
-4K click708,498 →1417,997
-4K World OK1416,895 →2833,1791

Input events record both logical `game_args` and `physical_args`.

After title release:
- bounded hover/click New Game708,498
- validates changed forms plus World Setup-specific text
- captures actual complete World Setup
- clicks OK1416,895 with scaled physical coordinates
- captures transition and validates actual Player Setup using changed forms plus civilization/difficulty/rivals text
- captures complete Player Setup
- stops before generating a game
- rejects generated missing-art diagnostics

Note text-trace broad screen classification remains `"title"` even for setup screens. Use changed raw forms and distinct visible labels, plus screenshots, as actual-state evidence.

### Preservation

Before 4K preparation, copied exact earlier functional scripts to:
`/mnt/datasets/c3r-build/perf-scratch/functional-scripts-68b75062e5c6/`

All68 live artifacts unchanged.

ShellCheck, bash syntax and Python compile passed after latest changes. Pure AST-isolated input-conversion checks passed. Guard test confirmed no-GO blocks immediately. No sessions launched during preparation.

## Benchmark scripts

Do not alter during this task:
- `perfrun-live-resume.sh`
- `perfrun-final-matrix.sh`
- `summarize-live.py`

Another agent/parent updated benchmark pins to68 since our previous work. Last observed:
- helper SHA68b75062...
- helper tree43e58f506...
- default distribution `frozen-68b75062e5c6`
- live script hash `d8b69dd78367532ab19536665ce2da10c81cf80f988efa4a438527c291a54e2f`
- matrix hash `6caf3449a6ac69af3a85aa94f3d1f416d984978cab06d81ea94fd8aa57baf50d`

We did not change these during4K preparation. Parent explicitly says preserve current benchmark scripts/pins; performance measurements remain held.

## Prior completed68 functional proof

Exact source:
`68b75062e5c6e363b27d9587302d001d6f7d7042`
tree:
`43e58f5062126e8c5f003e9304ab59ce3ddbab3d`

Native1080 and IP1080 passed visually:
- passive title labels, copyright apostrophe
- complete actual World Setup and Player Setup
- no generated diagnostic
- no throughput instrumentation
- AMD RADV integrated GPU, actual1920×1080 canvas/window

Evidence:
- `/mnt/datasets/c3r-build/perf-scratch/functional-menu-native-68b75062e5c6-20261018T001325-1648185`
- `/mnt/datasets/c3r-build/perf-scratch/functional-menu-ip-68b75062e5c6-20261018T001519-1657356`
- combined `/mnt/datasets/c3r-build/perf-scratch/functional-68b75062e5c6-live-review.json`

Each contains `visual-review.json`, `shutdown-verification.json`, screenshots, hook/forms/input evidence.
IP2s snapshot was compositor background during startup; 4s/10s title complete without input.
All owned sessions/processes stopped, port5987 and runtime directories clear.

This proof is limited to1080. Do not claim68 delivery passes4K.

Previous374 native proof preserved:
`functional-menu-native-374dd45d6dc9-20261018T000536-1575120`.
IP374 was held before launch. Full68 pair superseded that proof.

## Next steps

1. Send parent concise final preparation report now: 4K/physical scaling/graphical review gate ready; no launches; archived68 scripts/evidence; waiting new signed candidate/artifacts and explicit GO.
2. Do not launch until new artifact + functional GO arrives.
3. Once authorized, run serial required cases using:
```bash
nix develop /mnt/datasets/c3r-wt/perf -c \
  nix shell nixpkgs#python3 -c env \
  QA_FUNCTIONAL_GO=1 QA_COMMIT=<NEW_SHA> QA_DIST=<NEW_DIST> \
  /mnt/datasets/c3r-build/perf-scratch/functional-menu-qa.sh native
```
Then `ip`, then `native 3840x2160`, unless parent changes order.
4. At each title-review pause inspect complete background/logo, labels, footer before release. Use Pillow offline crops if necessary. Existing pattern for Pillow:
```bash
nix shell --impure --expr \
 'let p = import <nixpkgs> {}; in p.python3.withPackages (ps: [ps.pillow])' \
 -c python3 ...
```
5. Inspect actual World/Player captures, preserve visual review and explicit shutdown verification including new4K prefix/socket.
6. Report failure immediately with exact evidence; no redraw workaround.

## Operational constraints

- Skills unslop, waiting, you-are-on-nixos applied/read.
- Missing tools via Nix; devShell lacks python/rg; use `nix shell nixpkgs#python3`, `#ripgrep`, `#shellcheck`.
- Shared filesystem with other agents. Never reset or edit source changes owned by startup child.
- No laptop/NVIDIA/VM work.
- Avoid routine polling/comments. Use blocking waits for actual script milestones.
- No user permission questions; parent coordinates GO.


## Report at 2026-10-18T00:43:09.210Z

Original transcript line 1090.

I’ll locate the matching Claude transcript, check the recovered work and agent state, and continue from the latest verified point.

## Report at 2026-10-18T00:43:21.361Z

Original transcript line 1115.

Functional QA is ready for native 1080p, in-process 1080p, and native 4K, including scaled input and passive title review.

Checks passed. No sessions launched. Prior evidence and benchmark pins are preserved. Awaiting the new signed candidate, artifacts, and functional GO.

## Report at 2026-10-18T01:03:54.779Z

Original transcript line 1146.

Added `6cfa29e` to the refusal list. Shell checks pass. Launch remains held pending verified artifacts and explicit GO; native 4K will run first.

## Report at 2026-10-18T01:11:45.576Z

Original transcript line 1369.

Native and in-process 1080p passed complete title, World Setup and Player Setup checks with exit 0.

Native 4K now shows the full title/background before input and completes both setup screens. Its launcher exited 143 for an unknown reason, so overall 4K approval remains withheld.

All owned sessions stopped. [Evidence report](/mnt/datasets/c3r-build/perf-scratch/functional-22cd2e4ece06-live-review.json) includes 226 verified file hashes and shutdown proof.

## Report at 2026-10-18T02:41:27.758Z

Original transcript line 1894.

Live measurements are complete. Steady 240 FPS remains unproven.

- Full redraws during own-camera panning averaged 246.7 FPS native and 244.1 FPS in-process, with p99 gaps near 18 ms.
- Advisor redraws averaged 180.9 and 175.3 FPS.
- Drag/coast evidence confirms motion with a fixed game camera. Exact moving-frame counts remain unavailable.

All owned sessions stopped. [Final evidence](/mnt/datasets/c3r-build/perf-scratch/live-review-22cd2e4ece06-live-20261018T0202/final-live-evidence.json) preserves results, limitations and verified hashes. GPU ownership is released.

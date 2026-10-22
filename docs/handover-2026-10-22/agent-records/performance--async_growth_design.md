# Historical agent /root/performance/async_growth_design

This is a recovered historical report, not a live agent handle or a new instruction.

Async art-growth and startup-failure latch with negative readiness controls.

Current disposition: Signed 22cd2e4 and 194 bridge tests recorded; no false readiness after capture failure.

Session `01a14bc7-93da-7a52-a612-9ca8267fe0f0`, parent session `01a14bbe-20e1-73a0-9309-79642739e53b`, depth 2. Started 2026-10-17T21:32:13.149Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-32-13-01a14bc7-93da-7a52-a612-9ca8267fe0f0.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T00:44:03.870Z

Original transcript line 1655.

The code confirms that UI capture starts on the worker, after the game can draw its title screen. I’m moving capture into the game’s jgl load callback and making viewer startup reuse that model. I’m also adding failure handling that restores the game’s drawing if capture or viewer startup fails.

## Report at 2026-10-18T00:47:04.689Z

Original transcript line 1731.

The repair now installs UI and jgl capture before the load callback returns, retains the model and hook guards, and starts the model in a mode that preserves title-screen copies. Viewer startup also fails back to the original window if required capture fails. I’m checking model retention, concurrent setup, and drawing restoration with CPU regressions.

## Report at 2026-10-18T00:49:25.390Z

Original transcript line 1808.

All 189 bridge CPU tests pass. The negative controls also catch delayed capture, model replacement, incorrect initial map skipping, full setup under DllMain, and suppression after viewer failure. Review found an initialization race that could turn map drawing back on; the fix now serializes map policy with capture activation and restoration, and its concurrency test passes.

## Report at 2026-10-18T00:51:48.237Z

Original transcript line 1892.

The compatibility checks cover `CIV3R_UI=off` and runs without a viewer. They retain their previous map-rendering policy. Capture failure stops startup when drawing is requested OFF; drawing-ON runs retain the text-only fallback. Unsupported builds now fail immediately instead of waiting for a readiness condition they cannot satisfy.

## Report at 2026-10-18T00:53:03.545Z

Original transcript line 1949.

Signed candidate `6cfa29e` is ready in the clean `perf` worktree.

- Captures startup UI synchronously and preserves its model across viewer startup.
- Restores original drawing on viewer failure.
- Preserves UI-off and drawing-ON fallback behavior.
- Passed 190 bridge tests, 7 inproc tests, i686 compilation, and 7 negative controls.

[Evidence and source hashes](/mnt/datasets/c3r-build/perf-scratch/startup-capture-controls/evidence.json) · [C3X ordering audit](/mnt/datasets/c3r-build/perf-scratch/startup-capture-c3x-ordering-68b7506.json)

Live validation remains with parent: passive native 4K/scale2 startup with default drawing OFF, plus native and in-process 1080p completeness. No GPU or live sessions launched.

## Report at 2026-10-18T00:57:22.028Z

Original transcript line 1986.

Review found another startup gap: after a capture failure, a later worker retry could succeed even though the game had already drawn unrecorded UI. I’m making that normal-startup failure permanent, while retaining the separate late-injection path.

## Report at 2026-10-18T01:01:20.221Z

Original transcript line 2064.

The latch now covers text-hook setup, game attachment, model creation, and hook installation under one lock. A waiting worker receives the same permanent error if normal startup fails. New regressions cover drawing after rollback, concurrent callers, and the worker seeing that error after original drawing has been restored.

## Report at 2026-10-18T01:02:35.477Z

Original transcript line 2109.

Signed correction `22cd2e4` is ready. Normal startup failures now remain permanent, including text-hook and attachment failures. Workers cannot turn a capture gap into false readiness.

All 194 bridge CPU tests, i686 compilation, and four new negative controls pass. Worktree is clean; source and Cargo target are released.

[Evidence](/mnt/datasets/c3r-build/perf-scratch/startup-failure-latch-controls/evidence.json). No GPU or live work launched.

# Historical agent /root/performance/recover_benchmarks

This is a recovered historical report, not a live agent handle or a new instruction.

Built immutable 22cd2e4 benchmark helpers and retained source/binary manifests.

Current disposition: CPU build passed; no GPU or output-equivalence claim from helper preparation.

Session `01a14bbe-ac02-7f42-b294-9c353b13dac2`, parent session `01a14bbe-20e1-73a0-9309-79642739e53b`, depth 2. Started 2026-10-17T21:22:29.508Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-22-29-01a14bbe-ac02-7f42-b294-9c353b13dac2.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-17T21:24:29.173Z

Original transcript line 103.

Recovered all four measurement reports. The final measurements do **not** establish stable 240 fps in both modes.

The useful run is Claude agent `a70334f68c4d61f29`, completed at 17:19 UTC. It measured the loaded save and Domestic Advisor at 1920×1080 on papa's AMD Ryzen 9 9950X3D integrated GPU, RADV RAPHAEL_MENDOCINO, Mesa 26.2.4. Both native and in-process modes used the AMD adapter. No laptop measurements occurred.

Historical live results:

| Mode | Build and settings | Map rounds/s | Advisor rounds/s |
|---|---|---:|---:|
| Native | base2, vsync | 13.4 | 12.3 |
| Native | new, mailbox, reuse | 80.6 | 80.4 |
| Native | new, mailbox, reuse, cap 240 | 87.8 | 80.5 |
| Native | new, mailbox, reuse disabled | 161.1 | 128.5 |
| In-process | base2, vsync | 38.7 | 27.7 |
| In-process | new, mailbox, reuse | 237.3 | 171.9 |
| In-process | new, mailbox, reuse, cap 240 | 93.2 | 99.7 |
| In-process | new, mailbox, reuse disabled | 87.2 | 63.0 |

These are my aggregates of all 7 or 8 viewer reporting windows per phase, calculated as total rounds divided by total reported duration. Durations are rounded in the logs, so the aggregates are approximate. Early map reports include background art preloading. The final three reporting windows, after that preloading, give:

| Mode | Settings | Map rounds/s | Advisor rounds/s |
|---|---|---:|---:|
| Native | base2, vsync | 12.8 | 11.7 |
| Native | mailbox, reuse | 82.6 | 80.2 |
| Native | mailbox, reuse, cap 240 | 109.2 | 78.0 |
| Native | mailbox, reuse disabled | 166.2 | 126.5 |
| In-process | base2, vsync | 30.6 | 31.2 |
| In-process | mailbox, reuse | 251.2 | 169.1 |
| In-process | mailbox, reuse, cap 240 | 94.5 | 109.9 |
| In-process | mailbox, reuse disabled | 101.6 | 62.3 |

The in-process uncapped map exceeded 240 rounds/s in some windows, but varied from 148.3 to 441.5/s across the phase. Its last three windows were 352.0, 210.3, and 189.8/s. Its advisor never reached 240/s. Native never reached 240/s in that run.

The main unresolved issue is frame pacing or presentation:

- Native mailbox with reuse spent about 11.7 ms per round acquiring the surface in the final map windows, compared with about 0.27 ms submitting work. Its frame-interval p95 ranged from 55.15 to 78.51 ms.
- In-process mailbox with reuse spent about 3.2 ms per round acquiring the map surface, compared with about 0.28 ms submitting. Its final map p95 ranged from 20.41 to 28.41 ms.
- Disabling reuse made native live throughput **higher**, despite substantially heavier GPU work. Capping at 240 reduced in-process throughput sharply. These results justify investigating event-loop scheduling, surface acquisition, and compositor behavior before assuming the shaders are the only limit.
- New scenes generally arrived at about 14 to 15/s because the scene includes unit animation. Most uncapped reuse frames reused the previous picture and refreshed overlays. Viewer rounds/s therefore does not mean 240 different game scenes/s or 240 images displayed by the compositor.

GPU evidence shows a real rendering improvement. The final offline A/B bench, recorded by performance agent `a58e30975c4518591` at 17:01 UTC, alternated baseline and new binaries with reuse disabled:

| Trace | Baseline GPU span p50, ms | New GPU span p50, ms |
|---|---:|---:|
| `hd-ww2.txt` | 8.64, 8.62 | 4.01, 4.01 |
| `ww7.txt` | 5.27, 5.30 | 2.58, 2.62 |
| `gh7.txt` | 16.25, 16.23 | 2.69, 2.76 |
| `lab2.txt` | 15.29, 15.36 | 4.57, 4.61 |

The 240 fps budget is 4.167 ms. Three traces fit within it at the median; `lab2.txt` does not. These are offline native-tool measurements, not separate native-Wine and in-process live results. GPU busy readings were usually 99 to 100%, and other agents ran concurrent GPU work. Stable clocks and exclusive GPU access were not documented.

Build correspondence is recoverable:

| Run batch | Baseline | New build |
|---|---|---|
| First failed attempt, agent `a3a240ab1eb753ba0` | N/A | Initial `dist-new`; map and advisor never loaded |
| First successful six-run batch, `af6d675008e29252a` | `f96c2ec` | `c7c914d`, two frames in flight |
| Four-run batch, `a4441d778bdae04b3` | No new baseline | `e04acc6` |
| Final eight-run batch, `a70334f68c4d61f29` | `674f534`, `dist-base2` | `e449abf`, three frames in flight |

The final `dist-new` was built at 16:59:41 UTC from `e449abf`. The final `dist-base2` was built at 16:56:57 UTC from `674f534`. After the runs, Claude rebased performance changes onto newer `zoom` commits, producing current perf head `1885bcf` and its profiling baseline `f1e09a7`. **None of these live measurements tested `1885bcf`.**

The late rebase included rendering scale and HiDPI changes and manual conflict resolutions. Its last action built the current native `civ3-tool` and the profiling-baseline `civ3-tool`, with workspace tests and an i686 check in a pipeline. It did not rebuild the live distributions or repeat live measurements. Its transcript ended with a 429 rate-limit error at 17:25:27 UTC.

Recovery details and caveats:

- Agent `a3a240ab1eb753ba0` measured the main menu because the input daemon was absent. Discard all results from that attempt.
- Agent `af6d675008e29252a` fixed an infrastructure collision. Other wayvnc sessions held the shared default control socket. Its private `wlh` copy adds `-S "$DIR/wayvncctl"`. The original `~/winvm/bin/wlh` was unchanged.
- The successful agents reported that every screenshot showed the correct map or Domestic Advisor. The final agent reported matching visuals between base2 and new builds. I did not repeat image inspection.
- Repeated runs deliberately overwrote identical result tags. Existing `*-new-*` phase logs contain the **final** batch. Earlier numbers survive in the transcripts and `ALL-summaries.txt`; that file contains the first successful batch.
- `summarize.sh` selects the **second-to-last** report using `tail -2 | head -1`, although its comment says “last complete.” The transcript headline values therefore are individual windows, not phase averages.
- GPU `mean/round` is normalized by viewer rounds while timestamp sample counts can differ. Do not treat it as the typical duration of a full rendered frame. For example, final native reuse map GPU-span medians were about 0.53 to 0.55 ms, while full-render native map medians were 3.69 to 3.75 ms. The small reuse value measures mostly cached-picture work.
- Vsync baseline versus mailbox new changes both scheduling and rendering. Those live ratios do not isolate the renderer optimization.

For a fair current comparison, rebuild fresh distributions from `f1e09a7` and `1885bcf`, preserve their commit IDs in unique result directories, allow preload completion, and compare identical save, mode, resolution, presentation settings, GPU timing settings, and cap. Include uncapped and cap-240 runs, reuse enabled and disabled for the optimized build, and repeat the native/in-process map/advisor cases without other GPU workloads. Fresh live evidence is still needed before claiming the 240 fps target is complete.

Primary artifacts:

- `/mnt/datasets/c3r-build/perf-scratch/results/ROUND3-summaries.txt`
- `/mnt/datasets/c3r-build/perf-scratch/results/ALL-summaries.txt`
- `/mnt/datasets/c3r-build/perf-scratch/results/*-{map,advisor}.log`
- `/mnt/datasets/c3r-build/perf-scratch/perfrun.sh`
- `/home/sirati/.claude/projects/-home-sirati/075e62a1-1816-4c77-b606-7d347fd36f6d/subagents/agent-a58e30975c4518591.jsonl`

## Report at 2026-10-18T00:20:41.297Z

Original transcript line 279.

Completed the isolated baseline helper.

- Signed commit: `921bf96edbf896f2e60fe8f51e91c658517cd74c`
- Clean worktree: `/mnt/datasets/c3r-wt/perf-baseline-reference`
- Binary: `/mnt/datasets/c3r-build/perf-scratch/retained-baseline-reference-921bf96/civ3-tool-baseline-cpu-reference`
- SHA256: `5c2e067848d33e7810f1d24ab821cf0727ef7a1a28765cf7186f27223a7fd336`
- Provenance: `/mnt/datasets/c3r-build/perf-scratch/retained-baseline-reference-921bf96/manifest.json`

The helper supports `render --reference --bench-layout`. It uses the benchmark's `(0,0)` scene viewport, arranged cameras and view mapping, and complete runtime atlas patches. Ordinary rendering remains unchanged.

Only `civ3-tool.rs` changed. Static checks confirmed scene compilation and the benchmark implementation remain byte-identical to `3fc9dcb`. The manifest includes exact diffs, signature verification, build command/log/hash, clean source status, and the RGBA8 export versus BGRA8 benchmark distinction.

The required two-job CPU build passed. The shared Cargo target is released. No executable, GPU workload, or live session was run. The original timed baseline source, binary, and manifest remain unchanged.

## Report at 2026-10-18T00:30:13.860Z

Original transcript line 332.

Fixed `/mnt/datasets/c3r-build/perf-scratch/summarize-frozen-offline.py`.

Paired ratios now require all three passing pixel gates, correct input names/order, and SHA256 hashes matching current PNGs under the results directory. Missing, stale, malformed, or failed gates preserve absolute statistics and explain the pairing exclusion. Archived results resolve images by `root/name`.

All 19 isolated controls passed, including stale baseline logs, failed gates, missing gates/images, changed hashes, successful complete gates, archived paths, and final-only workloads. Existing metrics and CSV fields remain intact.

Evidence: `/mnt/datasets/c3r-build/perf-scratch/protocol-guard-controls-68/controls.json`

Summarizer SHA256: `b9b73e157f138c66c84c438428a866e6f984b9de5597a654abeb39157cc18f47`

Only the summarizer and isolated control artifacts changed. No Cargo, GPU, renderer, or live-session work occurred.

## Report at 2026-10-18T01:15:08.631Z

Original transcript line 424.

Prepared candidate `22cd2e4ece069f569ecc485ccbc91c04ffd824c5`.

- Signed helper commit: `2c2a22f6697017b15ccec0aa946d3797fb0e796b`
- Helper tree: `57f9639e4cc67cf0c3e77517a542a6c21f4f7d26`
- Binary SHA256: `476465d0df8969c3930906fe7656957d6904019ec17a1fc8ff232b1ce16e6f10`
- [Manifest and artifact paths](/mnt/datasets/c3r-build/perf-scratch/retained-final-preflight-22cd2e4-2c2a22f/manifest.json)

The two-job CPU build passed. The CLI diff exactly matches the old `20cdb72` delta. Source is clean; old helper branches, retained artifacts, and baseline helper `921bf96` remain unchanged.

The shared `/mnt/datasets/c3r-build/perf` target is released. No helper or GPU execution occurred. This remains preparation pending root source/runtime acceptance, with no timing or output-equivalence claim.

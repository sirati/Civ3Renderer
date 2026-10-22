# Intel VM missing HUD and units: archived timing analysis

Read-only source/log/image analysis. No builds, runtime, GUI/input or shipping edits. All original evidence retained.

## Finding

The two cited incomplete map images precede their PE sprite-table adoption. The healthy run subsequently shows complete HUD/minimap/buttons and unit bodies in024-loaded-E-quit-dialog.png. Archived data therefore does not show permanent missing art in healthy E, nor establish that no-device audio caused missing art. GPU upload was already complete well before the PE tables arrived in both runs.

| Event, seconds from bridge epoch | Healthy f7b E | No-device f510 |
| --- | ---: | ---: |
| First loaded world art wanted |303.299|420.050|
| Native art11 compile done |308.833|425.654|
| Native art11 compile duration |5.424|5.575|
| GPU art11 upload complete |308.949|425.866|
| Cited incomplete map image |319.717|452.877|
| PE art11 tables done/adopted |334.276|456.034|
| PE art11 internal job duration |30.909|35.957|
| Log: all visible interface art arrived |336.427|456.097|
| Last missing-unit log |334.242|455.999|
| Existing-unit animations become drawable |after334.276|456.110|
| Later complete healthy HUD/unit image |340.654|none archived before480s cap|

Healthy image023 time uses preserved host PNG mtime17:56:15.928516Z minus bridge epoch1792518656.212. Image024 mtime17:56:36.865791Z gives340.654s. These mtimes are capture proxies, not embedded frame timestamps. Independent visual inspection of024 directly establishes eventual complete HUD/unit art regardless of exact clock correlation. Its quit dialog covers some city labels but underlying HUD/minimap/unit bodies are clearly present.

No-device time is the owner's explicit loaded-screenshot-timeline.json entry18:57:22.345125Z minus bridge epoch1792522189.468, yielding452.877s. The later loaded-quit-dialog image is post-deadline failure UI, not a late responsive-game oracle. No image between PE adoption456.034s and480s cap is archived. Publisher liveness is not proof of native game-thread responsiveness.

## Evidence paths

Healthy log:
/mnt/datasets/c3r-build/vm-audio-production-sol/loaded-E-evidence/home/game/Civilization III Complete/Conquests/civ3-bridge.log

Healthy images:
/mnt/datasets/c3r-build/vm-audio-production-sol/023-loaded-E-map.png
/mnt/datasets/c3r-build/vm-audio-production-sol/024-loaded-E-quit-dialog.png

No-device log:
/mnt/datasets/c3r-build/vm-audio-production-sol/nodevice-retry-sequence/loaded-incomplete-evidence/home/game/Civilization III Complete/Conquests/civ3-bridge.log

No-device capture times:
/mnt/datasets/c3r-build/vm-audio-production-sol/nodevice-retry-sequence/loaded-screenshot-timeline.json

Healthy pertinent log lines:294 art wanted,1604 native art11,1657 upload complete,16795 PE tables done,16848 all visible art arrived. No-device pertinent lines:323 wanted,1686 native art11,1791 upload complete,18697 PE done,18698 all visible art arrived,18699+ animations drawable.

## Bounded FLC fix is carried

intel-manifest.json records renderer/native library origin9b07836fc7743952d53283e41b41999972a781f42d005be67c31e568cdddc205. Its immutable source contains flc::layout_reader and units.rs AnimFile::read opens the resolved File then calls it at lines71-72. It does not read the whole FLC payload there.

The relevant files are byte-identical in immutable9b, f7b and f510 trees:
- crates/civ3-assets/src/flc.rs SHA25678892ce13980aa70dec6a58b060b6ea5caa05746f8399207ea4445f11d528745
- crates/civ3-bridge/src/units.rs SHA25626d7df8a7cebea311096f696a597036fac757fc1aa806797becd44a5e0d20e91

All source references below use:
/mnt/datasets/c3r-build/release-1019/src-9b07836fc7743952d53283e41b41999972a781f42d005be67c31e568cdddc205/

## What the elapsed times locate

ArtDone.took is measured inside ArtJob::run from its own Instant, through crate::assets::grow, before result delivery. Thus30.909/35.957s is PE art-job wall time, not merely a publisher that waited25-30s to drain an already completed result. Native compile and GPU upload finish while the PE job is still running. The recorded large lag is not evidence of a pending SwiftShader texture upload.

assets.rs uses TablesOnly for the PE side. render-desc slice at1439-1445 does not load image/blank pixels when pixels=false unless anchor_marks are present. Placer Target::Sizes stores SpriteDesc metadata. Unit sheets do not need anchor-mark pixel scans. No evidence here supports blaming repeated FLC pixel decoding on the PE tables path.

## Concrete remaining redundant work

The bounded FLC change reduces bytes, but leaves duplicated metadata walks and filesystem operations:

1. units.rs612-635 emits separate named images for a unit file's shadow, smoke and body, plus additional body kinds for armies. Those images share the same FLC source and frame geometry.
2. render-desc1833-1840 caches measured frame sizes by image NAME, not source/layout identity. Distinct shadow/smoke/body image names therefore each invoke frame_sizes.
3. civ3-assets loader.rs327-347 resolves and opens the file on every sizes call. FLC sizes calls flc::dimensions, whose reader seeks EOF and reads every top-level six-byte chunk header by seek/read.
4. flc.rs read_layout seeks and reads once per physical chunk. No pixel payload is read, but the number of filesystem calls scales with physical frames times distinct image variants. Minimum ordinary unit case has three geometry-identical image names, so at least three complete metadata walks per file per compile. Additional recolour/army variants can add more. The measured cache is local to one add_sheets call, so later growths walk again.
5. GameFiles::resolve walks path components. files.rs child checks exact.exists BEFORE consulting its directory-listing cache, so repeated resolutions still issue filesystem existence calls even with cached listings. Art paths also resolve for loader.identity separately from sizes. QA preflight confirms game assets are mounted read-only through SMB.

These are definite source-level redundant operations. Their share of the30-36s PE job is not measured by the archived logs. Wine filesystem calls plus SMB are plausible amplification points, but not a proven duration attribution. Source reading alone cannot distinguish their cost from TOML parsing, table construction or other work.

Additional observable work: healthy log contains16,166 missing-unit DEBUG lines, no-device18,002. units.rs1004 logs every missing piece during repeated publication. This is substantial synchronous logging while assets are absent. It can add CPU/I/O contention, but the archived evidence does not quantify its effect on the art worker.

## Next diagnostic before optimization

During the first loaded art11 batch, record per-art-job stage duration and counts for TOML parse, frame_sizes, source resolution, File open, layout read/seek calls, sheet slicing and table assembly/apply. Group size queries by resolved canonical source plus layout identity, retaining separate requested-frame filters. Capture the PE art-thread stack during the known30-36s interval and one screenshot after the all-visible/adoption marker, rather than using elapsed timeout alone. Existing counts must distinguish bytes from calls; the bounded reader can read few bytes yet incur many remote calls.

If repeated geometry-identical size scans dominate, a scoped metadata cache keyed by resolved source identity within the compiler/loader would remove redundant shadow/smoke/body walks while preserving exact physical frame counts and frame filters. This is an optimization candidate, not an authorized shipping change in this analysis. Do not skip validation, derive physical frame count from nominal header count, clear global generations or increase arbitrary acceptance caps to mask the delay.

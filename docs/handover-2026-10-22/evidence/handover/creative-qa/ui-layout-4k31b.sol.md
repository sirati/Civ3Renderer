# Frozen UI geometry QA

Frozen source: 31b5951a200234422c73a7c8c8f8b97bea77424fc8c9a86d2580003553c3ef58.
Shader artifact: 67f3cefc9e905d7404baf63a3d0a3b54ca775bddc61612f52aa57c9b7de1ceb0. CPU geometry only; no shader dispatch performed.

Standalone test source: /mnt/datasets/c3r-build/ui-qa31b-harness/src/main.rs.
Log: /mnt/datasets/c3r-build/handover/creative-qa/ui-layout-4k31b.log.

15,200 layout cases and 684,000 input round trips passed. Windows were 3840x2160, 3839x2159, 3841x2161, 5120x2160 and 2160x3840. Each used pixel scales 1 through 5, menu scaling disabled/enabled, every integer percentage 25 through 100, and Standard/Vertical1To1/Vertical2To1/Horizontal1To1 splits.

The synthetic title backdrop was 1024x768 on a 1920x1080 driver screen. The test checks the requested scale against an independent height/percent/fit formula, requires the final rectangle inside the canvas, ensures a footer decoration does not change title fitting, ensures split selection does not affect main-menu geometry, and round-trips visible pixel samples in five synthetic control rectangles through canvas/physical/driver coordinates. Samples use ceil pixel boundaries because subpixel rectangle coverage has no rendered pixel to route.

Scale ratios deliberately use terms at most 4096 and round downward. An initial exact numeric tolerance flagged five harmless differences. The final oracle requires no upward overshoot, bounded terms, and at most 0.25 pixel error across a 1024-pixel span. Observed maximum error was 0.15471382392070154 pixel. All clipping, split-invariance and routing checks passed both runs.

The scratch manifest includes the frozen workspace's vendor/winit and vendor/wgpu-hal overrides. The first compilation omitted these overrides and failed on MonitorHandle::work_area. This was a scratch dependency issue; no frozen or working-tree source changed.

This does not validate rendered text/background onset, dithering, conquests previews, real control layout, Accept/Cancel, or city/advisor rendering. The VM owner reports Wine11 mmdevapi stack overflow at mmdevapi+14e63 before game UI interaction. Runtime evidence is /mnt/datasets/c3r-build/vm-qa31b-sol/extended-crash-evidence. CIV3R_TEXT_TRACE=1 and one isolated empty font-cache launch were requested for the next controlled VM run. The VM owner retains the sole GUI/input lease.

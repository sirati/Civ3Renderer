# Shipping f510 native 4K result

The 240 FPS goal is not met. Three actual App repetitions produced 43.22, 43.99 and 43.00 presentation FPS. Full-submit rates were 43.24, 43.99 and 43.00 FPS. The only stock qualification failure in every repetition is the 240 FPS threshold.

| Repetition | App FPS | Full-submit FPS | Full submits | Reuse/replay | Timing drops |
| --- | ---: | ---: | ---: | --- | --- |
| 1 | 43.22 | 43.24 | 2383 | 0/0 | 0 |
| 2 | 43.99 | 43.99 | 2424 | 0/0 | 0 |
| 3 | 43.00 | 43.00 | 2372 | 0/0 | 0 |

The fresh ordinary preflight passed before timing. Each repetition has 11 complete moving intervals, five positive and six negative legs, ten reversals, a fixed inverse-label source anchor, and fixed B. Native 3840 by 2160 internal/output geometry, full UI, current save summary and native map trace were checked in this exact run. The native camera is parked at (0, 0), with the traced visible region -1 through 5. The game files were referenced directly.

Individual complete-window GPU frame p50s range from 19.42 to 20.82 ms. Host encode wall p50s range from 12.64 to 16.49 ms. The latter includes world updates and command preparation through encoder.finish; surface acquire, queue submit and present have separate spans. These wall measurements do not identify CPU busy time. GPU gap p50s range from 2.35 to 2.63 ms and are not called GPU waiting for the CPU.

The supplement maps every vN stage to actual retained camera configurations and preserves each report window. Thin world views v2/v3 have viewport 2 by 720; v4/v5 have viewport 2 by 96. Their non-increment stage p50 ranges are listed separately below. No quantiles are summed into a frame total.

| Stage | Viewport | Individual-window p50 range, ms |
| --- | --- | --- |
| gpu v0 | 1920 by 2160 | 4.74 to 5.77 |
| gpu v1 | 1916 by 2160 | 4.47 to 4.59 |
| gpu v2 | 2 by 720 | 1.25 to 1.28 |
| gpu v3 | 2 by 720 | 1.25 to 1.27 |
| gpu v4 | 2 by 96 | 1.20 to 1.22 |
| gpu v5 | 2 by 96 | 1.20 to 1.23 |

All timestamp drop deltas are zero. Endpoint counts fit the immutable readback ring of 32. Repetition 1 collected 2384 samples versus 2383 submitted in its endpoint interval, so these counts are not claimed as an exact matched submitted-frame cohort. Wine report windows are authoritative here; duplicate bridge reports are not pooled. First and last partial observation intervals remain unobserved.

Unrelated Python/QEMU/long-lived Civ3 work stayed untouched. Repetition boundary load averages were about 24.6 to 30.7. Before/after pressure, GPU clocks, power and temperature records are retained. This is a loaded-host observation and establishes no isolated hardware limit.

No capture, protocol debug, build, hashing, transfer or VM activity occurred during measured intervals. All owned processes are absent and the exclusive runtime lease is released. This proves neither the FPS goal nor macOS loaded-audio, deployment, or all-feature acceptance.

Full proof: `actual-App-f510-native4k-results.json`. Per-camera/window supplement: `actual-App-f510-camera-stage-map.json`.

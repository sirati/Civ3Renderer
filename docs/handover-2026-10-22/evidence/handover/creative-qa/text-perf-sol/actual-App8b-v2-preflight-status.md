# Actual 8b native4K preflight

The native1x/dual0 WW2 workload loaded and showed its map, units, labels and interface. The existing save hash and current Monarch/Germans/Week24 1941AD summary were verified. A one-shot trace confirmed the 256x256 nonwrapping map and parked original camera. No game files were copied. All measured runtime artifacts were byte-matched to the test-only 6029 successor carry.

The capture-free pointer-v2 preflight failed before any timing run. Primary A x positions were 10048,9751,9708,9708,7179,4484,4435,4435. The label shifts were exactly opposite, but there were no positive legs or reversals. Incoming actual OS pointer coordinates were not logged. RFB and WayVNC had been stopped before preflight. The original helper/fixture code and qualification rules were unchanged.

Source review identified a wrong requested right target. A's pane ends at x1920; the exact held divider pixel is 1919. The requested point 1918,800 lies in the open y720..1440 divider hit band, so it cannot request A's rightward scroll. A target-only correction and an actual Layout geometry oracle are prepared separately in `/mnt/datasets/c3r-build/pointer-v3-geometry-sol`. They remain uncompiled pending the CPU gate after audio controls. Pulse traversal and actual pointer delivery remain separate hypotheses.

No accepted timing, 240 FPS result, full-coverage claim, second case or logical1080/crisp4K case was started. Host load was 36.22 before and 49.33 after; the AMD was 100% busy at 2200MHz core/3000MHz memory in the recorded samples. Unrelated Python/QEMU/long-lived Civ3 remained, so these are loaded-host diagnostics, not a hardware limit.

All owned processes were removed, with no forced direct children or survivors and no unrelated processes touched. The runtime lease was released to the broker for the audio exit-fault controls. Application release acceptance remains blocked by loaded-game Quit failing in the macOS VM. Exact evidence and counter accounting are in `actual-App8b-v2-preflight-proof.json`.

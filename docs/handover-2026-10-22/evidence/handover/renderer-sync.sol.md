# Renderer control snapshots and latency diagnostics

Renderer controls now use three small FrameParams/OutputParams slots. Worlds and atlases remain shared. Up to three frames can be encoded before submission. A fourth refuses preparation without overwriting controls. Call controls_submitted only after all prepared command buffers are submitted. Queue ordering, rather than GPU completion fences, protects reuse. Controls are packed into at most two queue writes per frame.

Raw cached and headless submission helpers notify the renderer. The GPU actor has one common notification after its queue submission. Initial and selected vsync configurations request maximum frame latency 2; explicit immediate/mailbox modes retain 3.

Idle atlas completion checks skip device polling when no incomplete callbacks exist. Active transfer completion and retirement accounting remain unchanged. Poll-only transfer/retirement regression passes.

TILE_VIEWER_LATENCY is restored after the GPU actor migration. Main tracks at most 64 frame ids and input Instants, with no scene/world snapshots. GPU completion callbacks capture only a scalar id and an Arc mailbox. The actor forwards callback timestamps after dropping the current Frame envelope, so GPU callbacks never wake main while retaining a world. Main records start and acknowledged presentation timestamps, and reports p50/p95/max/count every 10 seconds. Out-of-order completion and bounded stale tokens have a CPU regression. There are no new waits or device polls. Bitmap completion may precede main bitmap presentation; input timestamps remain available for both samples.

Validation on papa AMD RADV, nix develop, CARGO_INCREMENTAL=0:
- renderer-sync-sol-canonical-render-tests.log: 81 passed, all renderer integration suites.
- renderer-sync-sol-controls-final.log: 3 passed, including Direct and Quantized5 three-unsubmitted snapshots and reuse before a completion poll.
- renderer-sync-sol-viewer-final.log: 400 passed, 3 ignored, before latency restoration.
- renderer-sync-sol-latency-check.log: 6 actor tests passed after scalar callback integration, including drop-before-wake and numeric GPU ownership.
- renderer-sync-sol-viewer-latency-final.log: 410 passed, 0 failed, 3 ignored after latency restoration.

Control uploads measured by tests: Direct snapshot 176B/1 write; Quantized snapshot 208B/2 writes; cached output 32B/1 write; three views 720B/2 writes, previously 4 writes. A 20,000-call idle atlas poll microbenchmark median fell from 8977us to 215us. This is about 0.45us to 0.011us per idle call, not a claim of a large FPS increase.

Renderer suite shader SHA256: 67f3cefc9e905d7404baf63a3d0a3b54ca775bddc61612f52aa57c9b7de1ceb0.
Durable shader: /mnt/datasets/c3r-build/army-solid-artifacts/shader.spv, alongside shader-sources.sha256.
Own build directory: /mnt/datasets/c3r-build/renderer-sync-sol/target. Do not remove while the final viewer test session is active.

Native DX12 direct geometry fix and earlier independent preview oracle evidence remain in preview-smear.sol.md. No GUI/image inspection was performed for this work. Live UI pacing/latency was not measured; diagnostics and numerical regressions are the evidence here.
2318eba5de7f80719b7fc8a29ffa22c5d375ee538193631cfa78e5b603b2caec  crates/tile-render/src/renderer.rs
009cc81551802828afa3b5c930057c79d2522c492364f12be550f7a6905a22f2  crates/tile-render/tests/control_frames.rs
9400cee46f52f84734bf1cb7b2e8846afb8a2a9f120d113c0159cdce952b69c7  crates/tile-viewer/src/cursor.rs
4b211dbae9581d2726c010156a185d788419b47afaf6cbb7c238d5a654355f7b  crates/tile-viewer/src/render_worker.rs
ba3a8be43eaed4dad7c4d369bbf8abf1a390bc0871d9fdf5770772e8d0b47cdc  crates/tile-viewer/src/lib.rs
9400cee46f52f84734bf1cb7b2e8846afb8a2a9f120d113c0159cdce952b69c7  crates/tile-viewer/src/cursor.rs
c6233b04d07d15a6f5a3eabdcc66e0143e37e01b4f8f6b6bcb87e122eec2576e  crates/tile-viewer/src/render_worker.rs
a890b7264f9de2b3cc8d8481f30f60abc3830a13a888d935ce2af49d8c88059f  crates/tile-viewer/src/lib.rs

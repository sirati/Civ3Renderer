# Prepared-frame source update scope

Read-only audit during the FPS quiet window. No build or GPU execution.

The independent glyph-cache gate verifies immutable camera/cache controls across three preparations. Its inter-preparation source changes use set_world, which allocates replacement GPU buffers. Warm update_free cases occur between completed submissions. The gate does not promise immutable source data when update_free runs between encoding and submission.

TileRenderer::update_free writes existing w.free and w.text buffers through Queue::write_buffer when capacity permits. If a caller encodes frame A, calls update_free, encodes B, and only then submits A/B, both commands can observe B's source records. A's separately captured cache metadata may then describe the former glyph placement. This is a public API precondition/documentation issue; no application failure was observed or executed by this audit.

The production render_worker::State frame path updates world/cells/free/markers first, then creates one encoder, renders it, submits its command buffer and calls controls_submitted before processing the next packet. It therefore avoids this interleaving. Root was informed that broad immutable-world-buffer claims would exceed tested scope. Supporting concurrent source mutation would require a separate design; alternatively the renderer API should state and enforce submission before in-place source updates.

Relevant code: crates/tile-render/src/renderer.rs, update_free; crates/tile-viewer/src/render_worker.rs, frame preparation and immediate queue.submit. The cache source used by the exact numeric gate remains source-cache-v2 with manifest SHA256 671a6746d7925fc18b958dc6ebc84c4f8c8edc27ae6a97ac8abfe4764b473514.

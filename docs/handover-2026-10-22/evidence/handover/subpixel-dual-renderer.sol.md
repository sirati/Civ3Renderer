# Subpixel dual-source renderer integration

Source-only integration was authorized by root during the GPU broker quiet window. No build or GPU work was started by this owner.

The renderer adds P::SubpixelDual(into) at indices39 through41, with COUNT42. It uses free_vs and subpixel_dual_fs with the ordinary group0 layout. The pipeline shares TextDual's blend setup. Direct targets use Src1 and OneMinusSrc1; quantized targets use One and OneMinusSrc1. Only RGB channels are written, preserving destination alpha. Legacy Subpixel(into, channel) pipelines remain unchanged.

TileRenderer::new stores a constructor-only choice. Dual-source subpixel drawing is selected only when the device negotiated DUAL_SOURCE_BLENDING and TILE_RENDER_SUBPIXEL_DUAL is not0. An atlas preparer copies the renderer's choice. Each Subpixel run then uses either one new pipeline or the existing three channel pipelines. Drawing order, smoke snapshot boundaries and text cache bindings are unchanged. No new locks or CPU rendering were added.

Shader owner owns subpixel_dual_fs and exhaustive numeric tests. Release owner registered both text_dual_fs and subpixel_dual_fs for dual-source normalization and fallback stripping. Current staging SPIR-V that lacks the new entry must not be reused for a new constructor with the new mode enabled.

Numeric and performance acceptance remain pending. Paired same-binary A/B must explicitly compare default selection against TILE_RENDER_SUBPIXEL_DUAL=0, with all other controls held equal. Feature capability alone is not evidence of a speedup. Direct and quantized destination alpha must remain byte-identical, alongside RGB, indexed coverage, clipping, tint and zoom coverage.

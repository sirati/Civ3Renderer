# Thin-view GPU attribution diagnostic (source-only)

This is a nonshipping plan with no build, GPU run or source edit. It binds the exact f510 native1x baseline and private post-build artifact.

Every arm keeps the same 3840x2160 surface and target, Immediate presentation and full 18-camera order. v2/v3 are 2x720 and v4/v5 are 2x96. v0/v1 and the other cameras keep full world/UI content.

The source has no per-v2-v5 full-target clear, resolve or output. `render_layered` clears before the placement loop and outputs after it. Tiny native1x placements use Load plus scissor with no resolve target.

Use a 4x4 matrix of GPU-visible free/text eligible counts, then repeat the text axis with the existing procedural outline fallback. Keep timer labels and counters unchanged. Report durations and increments separately, because an increment includes the gap since the previous pass end.

Free counts use per-instance category masks so existing GPU category culls run while draw submission remains populated. Text counts use stable prefixes of the same source glyph list and record metadata bytes and draw ranges. Cache-enabled and cache-disabled arms are separate untimed controls. No CPU per-frame culling or repacking is allowed.

The result is attribution only. Root post-build review and a separate AMD-only broker lease are required before execution.

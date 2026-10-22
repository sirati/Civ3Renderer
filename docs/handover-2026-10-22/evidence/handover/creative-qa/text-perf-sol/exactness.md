# Text arithmetic and cache exactness

The old LCD filter taps are `[0, 32, 32, 32, 0]`. Each channel depends on only three covered subpixel lengths. With four vertically weighted rows, `ROW_TOTAL = 8`, and each subpixel width is 32 fixed units. If `s` is the sum of those three vertically integrated lengths, the old calculation is

```text
floor((32*s*255 + 12288) / 24576)
```

Canceling the common factor 32 gives exactly

```text
floor((s*255 + 384) / 768)
```

There is no change of rounding or sample positions. Each individual channel needs a three-subpixel window. The union needed by RGB is five subpixels. Narrowing a row's window only changes when an entire curve can use the existing full-width shortcut; clamped crossing contributions for each retained subpixel remain identical.

For cache normalization, let `q = floor(p.x / 96)` and `phase = p.x - 96*q`, using Euclidean remainder for negative positions. Sampling normalized pixel `px-q` gives

```text
96*(px-q) - phase = 96*px - p.x
```

The same identity holds for y. Scale, bold, italic and glyph identity remain in the key. Integer translations, foreground colors, background colors and clipping do not affect the generated linear coverage.

The existing linear filter already rounds to an integer between 0 and 255 before contrast boosting or 32-step alpha conversion. RGBA8Uint stores precisely those integers; integer texture loads preserve them without interpolation or float conversion. The later boost, alpha conversion, RGB555 rounding and blending use the existing formulas.

GPU generation precedes the consuming text draw. Tag publication follows the fill render pass. The draw reads both the tag buffer and the sampled atlas, so GPU resource ordering also protects atlas visibility. Canceled encoders publish nothing. Every used entry gets a fill job that is culled on the GPU if its exact tag is already ready, so later use repairs an unpublished entry without a CPU readback or wait.

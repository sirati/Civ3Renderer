# Historical agent /root/macos_bridge/scaled_ui_capture

This is a recovered historical report, not a live agent handle or a new instruction.

Captures resized native blits, clipping, palette/opacity and fixed-point sampling with cache identity.

Current disposition: Focused model/screen tests passed; generated-surface resizing remains unsupported.

Session `01a153c6-09dc-7df0-93e4-f85791a8c4a1`, parent session `01a152ae-c5f1-7043-9c75-ce2b3357fa1f`, depth 2. Started 2026-10-19T10:47:30.015Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T12-47-30-01a153c6-09dc-7df0-93e4-f85791a8c4a1.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T10:58:29.146Z

Original transcript line 389.

Implemented in `ui.rs`, `ui_screen.rs`, and `menu/src/screen.rs`.

```rust
Event::ResizedBlit {
    sprite: u32,
    canvas: u32,
    x: i32,
    y: i32,
    size: (i32, i32),
    sample_step: [u32; 2],
    clip: Option<[i32; 4]>,
    palette: Option<menu::screen::PaletteFrom>,
}
```

The pipeline preserves source crop, mask, palette, transparency, opacity, and native 16.16 sampling through clipping, canvas copies, scrolling, and composition. Cached art distinguishes destination sizes and sample steps. City/status unit targets use resized destination bounds while preserving native hit columns.

Passed:

- UiModel: 31 tests.
- UI screen: 23 tests, including copied-icon pixel verification and resized unit targets.
- Menu screen: 8 tests, including actual atlas sampling and cache identity.
- `git diff --check`.

Generated-surface resizing remains unsupported; no required caller was established. No commits or game assets changed.

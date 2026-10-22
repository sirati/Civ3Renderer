# Historical agent /root/alt_setup/setup_decode

This is a recovered historical report, not a live agent handle or a new instruction.

Verified native editable-seed vtable, callbacks and buffer fields.

Current disposition: Read-only evidence delivered to parent; no live setup acceptance.

Session `01a14f65-f6f2-7273-bca0-55ddd3ca602b`, parent session `01a14f64-f10a-73a1-94f6-b47779c6431e`, depth 2. Started 2026-10-18T14:24:04.857Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T16-24-04-01a14f65-f6f2-7273-bca0-55ddd3ca602b.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:31:31.182Z

Original transcript line 387.

Read-only reverse engineering complete.

- World setup vtable `0x670390`: hover `0x5DEA10`, click `0x5DEA80`, indexed hit mask at form `+0x3958`.
- Player setup vtable `0x66E2F0`: hover `0x5835A0`, click `0x583610`, hit mask at `+0x2B60`. This covers civilization, difficulty, aggression and game options.
- Each mask’s JGL image is at PCX `+4`. Read `bits=+0x4C0`, `stride=+0x40`, `bpp=+0x24`, dimensions from rectangle `+0x54`. Eight-bit pixels directly identify clickable choices. Existing rendering suppression preserves these masks.
- Direct choice classes bypass `typed_draw`: grid form vtable `0x671730`, thunk `0x615B00`, target `0x6156F0`; text choice vtable `0x666778`, thunk `0x4270A0`, target `0x61D7C0`.

Title menu has no child controls. Its active flag is byte `0xB425CA`. Eleven rows use:

```text
x = (screen_width - 1024) / 2 + [0x72C2A4]
y = (screen_height - 1024) / 2 + [0x72C2A8]
row_height = [0xB3E274]
```

Skip row zero when `[0xA52CE8] == 0`. Native width is measured label width plus 20; label drawing starts at row origin `+(13,11)`. Use captured text extents to obtain accurate widths. Native click handler is `0x54ED90`; it reads global mouse coordinates `0xA26870/0xA26874`.

No repository files changed.

## Report at 2026-10-18T14:43:07.831Z

Original transcript line 435.

Map seed is editable and should be an Alt target.

- Form vtable: `0x667188`.
- Setup: `0x622470`, called for map seed at `0x5DD70F`.
- Mouse selection: `0x6244D0`; release: `0x624540`; drag hover: `0x6245B0`.
- Character editing: `0x623720`.
- Buffer pointer: `+0x1934`; maximum length: `+0x193C`; caret: `+0x196C`. Map seed allows ten characters.
[Credential-related line redacted from this historical copy.]

Recognizing this exact vtable with the existing visibility and `NO_MOUSE` checks safely includes editable text boxes. No edits made.

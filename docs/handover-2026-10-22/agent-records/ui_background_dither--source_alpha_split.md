# Historical agent /root/ui_background_dither/source_alpha_split

This is a recovered historical report, not a live agent handle or a new instruction.

Added complementary alpha/background-mask and nearest/dither/clip/move regressions.

Current disposition: 28 menu tests passed at checkpoint.

Session `01a1543f-0a47-75b1-a90c-8fb40aae4c59`, parent session `01a15411-6a53-7662-846d-d8778b930330`, depth 2. Started 2026-10-19T12:59:39.977Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T14-59-39-01a1543f-0a47-75b1-a90c-8fb40aae4c59.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T12:59:39.980Z

Original transcript line 18.

Implemented background-only dithering in Rust.

- Applies only to verified parchment texture regions during downsampling.
- Preserves controls, text, borders, icons, world art, alpha masks and source crops.
- Stable across frames, moves, clipping and stacked views.
- Passed 55 existing GPU checks, 4 new GPU cases, 17 shared, 39 recipe and 25 menu tests.

Immutable shader: `/mnt/datasets/c3r-build/macos-port/ui-background-dither/candidates/12a8ce7134808767486cd9b5fe3e01932827eb855d89b4d29f7834f419a1a38f/shader.spv`

Source hashes and checkpoint are beside it in `source-binding-final.json`. Metal validation remains for the presentation owner.

## Report at 2026-10-19T12:59:39.980Z

Original transcript line 22.

Closed native capture gaps. Background metadata now covers 128 verified texture crops across parchment, Civilopedia, popup interiors and seven advisor files. The other 412 preload regions retain nearest sampling, including frames, arrows, separators and controls.

Exact source rectangles, canonical hashes and decoded evidence are in [audit.json](/mnt/datasets/c3r-build/macos-port/ui-background-dither/native-background-coverage/audit.json).

All 25 menu and 8 capture pipeline tests pass. Shader `12a8ce…` and existing text code remain unchanged. Ready for aggregate freeze.

## Report at 2026-10-19T13:06:08.350Z

Original transcript line 209.

Implemented safe Rust source alpha masks in `crates/render-desc/src/lib.rs`.

- Complementary `alpha_keep` / `alpha_clear` preserve geometry, source origins, keyed alpha, partial alpha and draw opacity.
- Indexed storage preserves retained indices and palette remapping; exhausted non-remapped cells fall back to RGBA.
- Cache and growth identities distinguish modified pixels.
- Box sampling and sprite alias masks return explicit errors.

All 49 render-desc tests pass. Shader `12a8ce…` is unchanged. File SHA256: `04f0d6406c8cceadccc11df6a9bff27ec4e56089c5104a91f2b2df1a43f8f832`.

## Report at 2026-10-19T13:09:48.194Z

Original transcript line 329.

Added four CPU regressions in `crates/menu/src/screen/background_tests.rs`.

They cover complementary masks, unchanged geometry, exact nearest rendering through 1/8 scale, texture-only dithering, partial alpha, opacity, clipping, movement and pair readiness.

All 28 menu tests pass. File SHA256: `2f3ed53ee4bbe24a854c3d5121ddf09ae1bff1aafcd4095d08c530dcf431a1d2`.

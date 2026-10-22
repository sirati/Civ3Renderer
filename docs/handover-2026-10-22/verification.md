# Renderer verification record

## Latest recorded checks

The 21 October continuation left the implementation tree clean at `8106ae5a4debbe247cb5e64523d4972474f4fe39`. Experimental wider-color edits were tried, failed to preserve RGB444/RGB555 behavior and were fully reverted. No performance measurement or accepted optimization was added.

| Check | Result | Scope |
| --- | --- | --- |
| `nix develop -c cargo test -p tile-viewer` | 495 unit tests passed, 4 ignored; host-status GPU test passed 1/1. | Current implementation baseline. |
| `nix develop -c cargo test -p tile-render` | Earlier suites passed; the package run stopped at the existing `subpixel_coverage` failure. | Not a passing full renderer suite. |
| Wider subpixel reproduction | Both default dual-source and `TILE_RENDER_SUBPIXEL_DUAL=0` reproduce the mismatch. | AMD Ryzen 9 9950X3D RADV RAPHAEL_MENDOCINO, Mesa 26.2.5. |
| Diagnostic inventory | Six-bit path has 61,325 mismatched pixels; seven-bit path has 134,041. | `TILE_RENDER_WIDER_ROUNDING_INVENTORY=1` permits diagnostic inventory to complete. It does not satisfy strict pixel correctness. |

The first retained mismatch is `bits=6 material=0 dst=0 x=5 y=0`, actual `[0,0,251,37]`, expected `[0,0,255,37]`. The problem is wider quantized rounding, not evidence that culling introduced a regression.

A direct Cargo invocation initially failed because the shell lacked `cc`. The Nix development environment resolved that toolchain issue. Use an owned target under `/mnt/datasets/c3r-build` for future builds, even though the 21 October turn used the repository target directory.

## Reproduction commands

These commands run GPU tests and should be scheduled without competing GPU measurements. They are a restart recipe, not additional test execution during documentation work.

```bash
export CARGO_TARGET_DIR=/mnt/datasets/c3r-build/handover-followup-target
export CARGO_INCREMENTAL=0
export TILE_RENDER_ADAPTER=radv
export WGPU_BACKEND=vulkan
nix develop -c cargo test -p tile-render --test subpixel_coverage -- --nocapture
TILE_RENDER_SUBPIXEL_DUAL=0 nix develop -c cargo test -p tile-render --test subpixel_coverage -- --nocapture
nix develop -c cargo test -p tile-viewer
```

Do not broaden the Microsoft Basic Render Driver FP16 workaround to RADV. Its exact adapter selection is intentional. Earlier attempts to generalize that approach failed wider cases and existing narrow-depth behavior.

## Historical counts

The evidence contains aggregate host, Wine PE, Intel macOS CPU, native Windows and GPU suites at several different source freezes. Their totals overlap and their platform coverage differs. Keep each source-bound result separate. In particular, Papa Wine execution does not substitute for a native Windows guest, Intel macOS SwiftShader does not establish Metal performance, and synthetic numeric tests do not establish full-game UI acceptance.

## Handover checks

The documentation task checks local document links, the 96-agent inventory, all 85 acceptance families, valid JSON, archive hashes, and Git backup restoration. It makes no renderer source changes and does not launch the game, VM or physical Neo app.

The offline HTML copy received static structure and link checks. Visual browser verification was unavailable. Two headless preview attempts returned "the browser exited unexpectedly"; the collaborative browser then failed to start with "Target page, context or browser has been closed".

# Renderer verification record

The six-bit/seven-bit investigation below is P4 backlog. It is not a deployment prerequisite. The current Neo installation is verified in [deployment.json](deployment.json).

## Neo startup fix and 32cb02c install

On Neo the f510 install never showed its window. Startup deadlocked inside Wine: stack samples show the bridge's `inproc-init` thread suspended inside a volume-information query while holding Wine's virtual-memory lock, and the game thread waiting on that lock. Each such query is a mountmgr ioctl that Wine completes asynchronously with a signal. Rust's `exists()`, `is_dir()` and `fs::read` made about 1,500 of them during case-insensitive asset path resolution. A signal arriving where Wine should block it is a defect of the ARM64 Wine/FEX build; commit `fee4096` removes the trigger by resolving paths from directory listings and reading files without the metadata call. Startup now makes 1 such query from renderer threads.

Ruled out on Neo: preloading (`CIV3R_PRELOAD=off` hangs too), the FEX code cache (`FEX_DISKCACHE=0` hangs too) and an OS update (none since 29 September). The f7b0 release that worked on 21 October hangs the same way now, with byte-identical bridge and native libraries, so something on Neo changed after 21 October; what changed is unknown. Early runs over SSH could not reach WindowServer and are not counted.

| Check | Result | Scope |
| --- | --- | --- |
| `cargo test -p tile-viewer` at `32cb02c` | 495 passed, 4 ignored; host GPU test passed 1/1. | Papa, AMD RADV. |
| `cargo test -p civ3-bridge` at `32cb02c` | 457 passed, 4 ignored. | Papa host. |
| `cargo test -p civ3-assets --lib` at `fee4096` | 22 passed, 1 ignored. | Papa host. |
| f510 GUI-session launches before the fix | 4 of 4 hung before the first art compile. | Neo. |
| Installed `32cb02c` via `open`, first set | 3 of 3: in-process viewer attached in 8–10 s, window on screen (CoreGraphics), `civ3-app-macos stop` left no owned processes in 4–6 s. | Neo; no screenshots. |
| Installed `32cb02c` via `open`, in-process (dual stack) checks | 3 of 3: no `--separate-viewer`, no `civ3r-viewer` process or viewer log, window owned by the Wine game process, ARM64 `civ3r_native.so` mapped from `runtime/native/aarch64-unix`, log shows art 1 compiled, `viewer started` and attachment to the host application; viewer attached in 8–9 s; clean stop in 4–5 s. | Neo; no screenshots. |

All launches ran the renderer inside the game's Wine process (dual stack). Wine maps the i686 `civ3r_native.dll` from the copy the launcher places in the private game folder, byte-identical to `runtime/native/i386-windows/civ3r_native.dll`; a `+module` trace shows this.

Not verified on Neo: the loaded map/HUD, input and the in-game Quit. Terminal is denied screen recording there and no available session has input rights; the permissions were not changed. The [Neo record](evidence/neo-window/verification.json) lists hashes, failed attempts and the retained evidence archives on Papa.

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

## Handover and installation checks

The handover checks local links, the actual unfinished tasks, current priorities, JSON and copied evidence hashes. The latest installation also has a separate parent review of six fresh remote hashes and the strict app signature, recorded in [deployment.json](deployment.json). No game launch or gameplay acceptance follows from installation verification.

The offline reading copy receives static structure/link checks. Visual browser preview was unavailable because the headless browser exited unexpectedly and the collaborative browser could not start.

## Scope of the macOS VM comparison

The same-source normal packaged app passed in the Intel macOS VM, with the production starter, `LSUIElement=true` and the default startup window enabled. The native window showed the title/menu and a real loaded map/HUD. Keyboard and mouse input worked. The ordinary Quit confirmation closed the game and launcher, released the launch lock, and left no owned processes. Finder launch does not provide a direct exit-code receipt. No production source changes were needed.

Fresh x86_64 native and paired PE builds use f510/protocol 50. Independent review found all 673 source entries identical to implementation baseline `8106ae5` and checked six runtime artifacts and eight CPU-test binaries against their hashes. The VM suites passed 560 unique tests with 8 ignored. Two tests first failed with `NoAdapter` because the invocation omitted the VM graphics environment. Both exact tests then passed with SwiftShader/Vulkan using the same binary. The initial failures are retained.

The [verification receipt](evidence/macos-vm/verification.json) binds the source, artifacts and evidence. Retained images show the [title](evidence/macos-vm/044-production-title-ready.png), [loaded map](evidence/macos-vm/050-production-map-final-check.png), [Quit confirmation](evidence/macos-vm/051-map-exit-menu.png) and [desktop after exit](evidence/macos-vm/052-normal-exit.png). Help output, heartbeat logs and CLI bypasses are not the basis for acceptance.

The Intel VM uses SwiftShader Vulkan and AppKit bitmap presentation, while Neo uses ARM64 Wine/FEX and Metal. The launcher also selects an Intel-only compatibility runtime in [wine_compat.rs](../../crates/civ3r-launcher/src/unix/wine_compat.rs). VM configuration changes Wine/prefix, asset/font paths and graphics settings, and disables asset preloading. Its startup-status client stayed blank without a Metal device, but the actual native renderer appeared and worked. High host load caused slow software presentation and stale input drops. This control does not qualify performance, physical Metal or Neo's installed package environment, and cannot identify ARM64 as the sole cause. Neo was not contacted during this VM check; the later Neo investigation is recorded above.

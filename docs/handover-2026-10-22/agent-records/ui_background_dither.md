# Historical agent /root/ui_background_dither

This is a recovered historical report, not a live agent handle or a new instruction.

Texture-only background sampling with alpha masks and later preload overflow saturation/admission fix.

Current disposition: Focused CPU/GPU tests passed; later upscaling/preview behavior needs current acceptance.

Session `01a15411-6a53-7662-846d-d8778b930330`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-19T12:09:49.909Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T14-09-49-01a15411-6a53-7662-846d-d8778b930330.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T12:21:21.297Z

Original transcript line 434.

Implemented background-only dithering in Rust.

- Applies only to verified parchment texture regions during downsampling.
- Preserves controls, text, borders, icons, world art, alpha masks and source crops.
- Stable across frames, moves, clipping and stacked views.
- Passed 55 existing GPU checks, 4 new GPU cases, 17 shared, 39 recipe and 25 menu tests.

Immutable shader: `/mnt/datasets/c3r-build/macos-port/ui-background-dither/candidates/12a8ce7134808767486cd9b5fe3e01932827eb855d89b4d29f7834f419a1a38f/shader.spv`

Source hashes and checkpoint are beside it in `source-binding-final.json`. Metal validation remains for the presentation owner.

## Report at 2026-10-19T12:43:05.797Z

Original transcript line 745.

Closed native capture gaps. Background metadata now covers 128 verified texture crops across parchment, Civilopedia, popup interiors and seven advisor files. The other 412 preload regions retain nearest sampling, including frames, arrows, separators and controls.

Exact source rectangles, canonical hashes and decoded evidence are in [audit.json](/mnt/datasets/c3r-build/macos-port/ui-background-dither/native-background-coverage/audit.json).

All 25 menu and 8 capture pipeline tests pass. Shader `12a8ce…` and existing text code remain unchanged. Ready for aggregate freeze.

## Report at 2026-10-19T13:27:33.698Z

Original transcript line 1258.

## Agent/task

You are `/root/ui_background_dither` in shared repo `/home/sirati/devel/rust/civ3-renderer`. Parent `/root` coordinates deployment and other agents.

**Current active task is a final budget-overflow fix requested by parent. It is not yet implemented.** Earlier background implementation is complete and tested, but parent found an aggregation overflow after our “ready” report.

Latest parent request:

> Final review found the budget saturation gap still present despite ready report: preload::Item::Screen uses plain sum::<u64>() and '+' for source_bytes outputs. source_bytes can now saturate to u64::MAX for a huge partial region, and adding a second region wraps in release or panics in debug, bypassing the budget. Fix Screen aggregation with saturating products/addition throughout affected estimates, and add a regression with a huge intersecting mixed region plus another region/fill. Keep estimate fail-closed at u64::MAX. Update final source/test binding; no shader/GPU rerun needed.

## Constraints

- Work autonomously; no user questions.
- Preserve all extensive shared uncommitted work. No commits.
- No Nexus or game asset writes, GUI actions, physical user-session interference, VM lifecycle operations.
- Product code stays safe Rust, no new C/Objective-C or unsafe.
- Keep shader **12a8ce… unchanged** and all existing pixel references/text fixes intact.
- Use `nix develop -c …`, `CARGO_INCREMENTAL=0`, shared target `/mnt/datasets/c3r-build/macos-port/target`.
- Collaboration tools are direct calls, never inside `functions.exec`.
- Skills already read: boss, waiting, unslop, you-are-on-nixos. Avoid polling; do useful independent work or wait.
- Parent wants precise evidence and source hashes before aggregate source freeze.

## Completed background work

Initial shader adds deterministic source-coordinate dithering only for explicitly tagged UI backgrounds when downsampling. It keeps nominal alpha/masks and source crops, does not affect world sprites, controls, text, 1× or upscale.

Immutable module:

```text
/mnt/datasets/c3r-build/macos-port/ui-background-dither/candidates/12a8ce7134808767486cd9b5fe3e01932827eb855d89b4d29f7834f419a1a38f/shader.spv
```

377,616 bytes, SHA256 equals directory name.

Shared text source is unchanged:

```text
7ca79e3e9cdf77bc5a1b8dc9b47d3747bf607c5867b4c199dc6c78f19b968084
```

Earlier shader validation: 55 existing Linux GPU references + 4 new background cases pass. Parent owns native Metal/VM validation.

Parent then required full mixed native artwork coverage and reusable crates. Implemented:

1. **Game-specific source/rectangle policy moved out of `menu` into**
   `crates/civ3-bridge/src/ui_background.rs`.
   It combines prior verified crop rectangles with 55 additional audited rectangles over 13 assets. Final policy covers 15 source names.
2. **Generic menu APIs**, with no game-specific production filenames:
   - `ScreenArt::description_with_backgrounds`
   - `sprites_with_backgrounds`
   - `Element::ready_with_backgrounds`
3. Mixed images use **two full-cell complementary source alpha masks**:
   - original named sprite: `alpha_clear`, ordinary nearest
   - `#background` sibling: `alpha_keep`, background sampling
   Both retain identical dimensions, original source origin, position, clip, opacity, order. This avoids artificial small quads and fractional hairline seams.
4. Description and draw/readiness share the caller policy. Pair publication is atomic; missing sibling prevents drawing a half pair.
5. Generic `render-desc::SheetDesc` gained safe Rust `alpha_keep` / `alpha_clear`.
   - RGBA only changes dropped alpha.
   - Indexed storage reuses an existing transparent palette index or unused retained index outside remap range.
   - Cache/growth pixel identities include masks and replacement index.
   - Exhausted non-remapped index sets fall back to RGBA; remapped sets return explicit error rather than lose recolouring.
   - Box sampling and sprite alias masks reject explicitly.
6. `ui_screen` routes art generation, sprite generation, readiness through bridge policy.
7. `ui_background::source_bytes` estimates mixed masks conservatively at **8 bytes per source pixel**, allowing two RGBA copies.
   This helper saturates, but aggregation/scheduling in `preload.rs` still does not, which is the current blocker.

## Files and last hashes before pending budget fix

```text
crates/civ3-bridge/src/lib.rs
  b98ace3b21361dafcf09db9421192c5bfb1b8ab89466b1abd765144a1760dbe9
crates/civ3-bridge/src/preload.rs
  afd854e9a0f51ada9e3dae384a81cdabc2980310a0aed42539eb1a55701fc12f
crates/civ3-bridge/src/ui_background.rs
  b422f574f5a01140cc6696ec58541f7fbbd68082b0807f77864c3ffa2a29621f
crates/civ3-bridge/src/ui_screen.rs
  abdae81782ad167250ef3d63a5a03f243288e158498b95ffd19b67a678cb1d6c
crates/menu/src/screen.rs
  12a7766ecf4d603db397d9fbab68983158e5b602756467247da182879d172d5b
crates/menu/src/screen/background_tests.rs (new)
  2f3ed53ee4bbe24a854c3d5121ddf09ae1bff1aafcd4095d08c530dcf431a1d2
crates/render-desc/src/lib.rs
  04f0d6406c8cceadccc11df6a9bff27ec4e56089c5104a91f2b2df1a43f8f832
```

There are earlier shared shader/reference edits in this task too. The original module binding is in its candidate directory; later menu/recipe producer changes intentionally did not alter shader code.

## Tests already passing on final background code

- `render-desc` 49/49, original 39 unchanged.
- `menu` 28/28, including four new generic source-mask tests.
- `civ3-bridge` `ui_screen::tests::` 10/10.
- `preload::tests::` 5/5 **before adding current overflow regression**.
- 30 tiny-mask GPU variants:
  1-pixel vertical, horizontal and point masks, five fractional/downsample ratios, hairline on/off, nonzero origin/phase, source partial alpha and opacity137. Exact CPU/GPU comparison passes.
- 20 actual native full-art CPU/GPU comparisons:
  Civilopedia, Preferences, foreign advisor, wonders advisor, title;
  opacity255/137 and zoom1/2,2/3.
  Separate native button and real Lucida Sans text on an opaque control plate remain byte exact against original unsplit GPU image.
  Undithered mask-pair CPU output equals original unsplit CPU output exactly.
  Actual title remains unchanged.
- Saved initial regression failed with **zero changed Civilopedia background pixels**, before mixed splitting. Passing cases change roughly 34k Civilopedia and 62k Preferences pixels at 1/2; only verified backgrounds change.

Real glyph AA / arbitrary partial alpha can differ by ±1 between CPU float blending and GPU UNorm. We did **not** change existing exact pixel references. Actual art tests compare unchanged controls to original same-GPU output; synthetic tiny-strip tests use representable binary colours and retain strict CPU/GPU equality.

## Evidence / final binding needing update after budget fix

Directory:

```text
/mnt/datasets/c3r-build/macos-port/ui-background-dither/native-background-coverage/
```

Current manifest:

```text
background-split-final.json
SHA256 59918b96a0a068037d492ef803426c306f7ff78c12d999ade038d2adfdc3010a
```

**This will become stale after the budget fix.** Update its source hash, test count/log hashes, and frozen source copy under `source/crates/civ3-bridge/src/preload.rs`.

Important logs:

```text
full-mixed-before.log                 # expected failing initial regression
generic-split-cpu-final.log           # menu28, recipe49
ui-screen-split-final.log             # screen10 including GPU tests
preload-split-budget-final.log        # preload5, needs rerun/update
split-background-final-2.log          # actual background cases + tiny GPU pass
```

Images:

```text
actual-full-art/*-{original,split}.png
full-source-audit/verified-pure-rectangles.json
full-source-audit/*-verified-rectangles.png
full-source-audit/*-native.png
```

Full Preferences has 13 verified texture regions. Borders, checkbox/slider wells, separator lines and arrows are outside the policy. Title art contains baked logos/text and no useful pure repeating UI texture, so stays nearest.

Earlier crop-only checkpoint `audit.json` is retained as history, superseded by `background-split-final.json`.

## Current overflow code and necessary next steps

Last read `crates/civ3-bridge/src/preload.rs`:

```rust
pub fn bytes(&self) -> u64 {
    match self {
        Item::Unit(f, _) => f.chunks as u64 * f.size.0 as u64 * f.size.1 as u64,
        Item::Screen(s) => {
            s.regions.iter().map(crate::ui_background::source_bytes).sum::<u64>()
                + s.resized_regions.iter()
                    .map(|(_, (w,h), _)| 4 * u64::from(*w) * u64::from(*h))
                    .sum::<u64>()
                + s.fills.iter()
                    .map(|(_, (w,h))| 4 * *w as u64 * *h as u64)
                    .sum::<u64>()
                + s.dynamic.iter()
                    .map(|(_, (w,h))| 4 * *w as u64 * *h as u64)
                    .sum::<u64>()
        }
        Item::Font(style, _, _) =>
            100 * 4 * style.height.unsigned_abs() as u64
                    * style.height.unsigned_abs() as u64,
    }
}
```

Also inspect estimates around lines217–229 and scheduling around269–295. These plain additions currently exist:

```rust
let mut bytes = 0;
...
if !items.is_empty() && bytes + b > chunk { break; }
if used + bytes + b > self.budget { over = true; break; }
bytes += b;
...
self.sent = (self.sent.0 + items.len(), self.sent.1 + bytes);
```

Do not fix aggregation alone while leaving a saturated estimate able to wrap through scheduler additions. Use saturating estimates and checked/fail-closed scheduling as appropriate. Consider unit/font multiplication too, since the parent requested saturation throughout affected estimates.

Add meaningful regression(s):

- A region with source `"Art/interface/x_pref background.pcx"`, rect `[0,0,i32::MAX,i32::MAX]` intersects verified rectangles and produces `source_bytes == u64::MAX`.
- Add a second ordinary region and/or fill and assert `Item::Screen.bytes() == u64::MAX`.
- Exercise oversized resized/fill/dynamic products if fixing them.
- Ensure `Preload::next` stops at the budget with nonzero `used`, rather than overflowing/scheduling the saturated item.

Existing nearby test:

```rust
fn mixed_native_background_masks_reserve_their_full_pixel_cost_before_preloading()
```

Currently asserts:

```rust
8 * 1024 * 768 + 418 * 570 + 1024 * 768
```

Keep it passing.

After fix:

1. Run only appropriate CPU preload tests, no shader/GPU rerun needed.
2. `git diff --check`.
3. Refresh `background-split-final.json`, source snapshot, changed hashes/test count/log hash.
4. Send parent concise concrete fix and updated manifest/hash; source freeze remains pending parent release.

## Subagents

- `/root/ui_background_dither/background_metadata`: complete, read-only asset audit.
- `/root/ui_background_dither/source_alpha_split`: complete, render-desc masks and generic menu tests.
- Parent `/root/settings_transaction` was told we own narrow ui_screen regression edits.
- `/root/macos_bridge` said no ui_screen edits reserved; waits aggregate freeze for DLL build.
- All our old tool sessions were closed; no background commands remain.

## Report at 2026-10-19T13:27:37.483Z

Original transcript line 1270.

I’m fixing the remaining preload budget overflow and updating the test evidence. The background dithering and artwork split are already implemented; this check ensures oversized artwork cannot bypass the memory limit.

## Report at 2026-10-19T13:29:25.950Z

Original transcript line 1330.

The new boundary tests reproduce three overflow failures. I’m applying the fix to both the artwork estimates and the scheduler, so an oversized estimate stays capped and cannot be admitted through arithmetic overflow.

## Report at 2026-10-19T13:30:31.568Z

Original transcript line 1379.

Fixed the preload overflow in [preload.rs](/home/sirati/devel/rust/civ3-renderer/crates/civ3-bridge/src/preload.rs). Estimates saturate, and checked budget admission rejects oversized items.

All 8 preload tests pass, including three regressions that failed before the fix. `git diff --check` passes. Frozen source and evidence manifest updated; shader unchanged.

Manifest SHA256: `46d4286cdcfa4c5420487cd3f653bd1eab0fcb1afae57672c428e5dd515f0c36`.

# Handover: army units (civ colour + health bar)

## Task
User report: an Army is drawn with the wrong civ colour (flag and tunic pure black on the Mac) and has no health bar.
Goal: match the game exactly (art, palette, HP, LED) and check the other special units (leaders, alternative art, transports).
Add real-data tests. Run host tests, the i686 check and the Wine bridge tests. Deploy to TS Neo.
Repo: /home/sirati/devel/rust/civ3-renderer (shared tree, many other agents; minimal edits only, never revert theirs).

## DONE (uncommitted)
- crates/civ3-sys/src/world.rs: new `Civ3::units_in(&[container ids]) -> Vec<DrawnUnit>` (walks the unit list, by Container_Unit).
- crates/civ3-bridge/src/live.rs (snapshot): `army_members` now holds ALL members of drawn armies (units_in), plus the displayed one as a fallback.
- crates/civ3-bridge/src/trace.rs: writes every member as an `army_unit` line (same format).
- crates/civ3-bridge/src/world.rs: doc of `army_members` updated.
- crates/civ3-bridge/src/units.rs:
  - consts `UTA_ARMY = 0x12` and `MOVE_POINTS_PER_TILE = 3`;
  - `is_army`, `own_recolor` (an army ALWAYS gets its army recolour, including an empty army: the game rewrites entries 64..127 for every army);
  - `army_recolor`: members = army_size clamped to 4 only if army_member is Some, else 0;
  - used in `recolor_of`, `Needed::of`, `pieces`, `still`;
  - `Needed::still` also records the displayed member's chunk;
  - HUD: new `army_contents`, `max_hp`, `strength`, `max_moves` (see RE below); `hud()` uses them.
- docs/civ3-map-rendering.md: section 10.6 has a new "Armies" bullet and the LED note; 10.11 branch note and HUD sentence.
- Tests in units.rs:
  - `an_armys_hud_counts_its_members` (no install needed). PASSED.
  - `armies_draw_in_their_owners_colours`, real data plus GPU: whole compile and growth; the palette per index 0..127 for civs 1, 2 and 3 with 0, 1, 2 and 4 members; the reference picture; the Headless GPU after set_atlas + grow_atlas matches the reference where the army is. PASSED in its earlier form.
  - Existing `units_pick_their_colours...` was adjusted (army_member: Some(5)). PASSED.
  - All 10 units::tests passed at that point.

## IN PROGRESS (exact state)
I appended a tables-only check inside `armies_draw_in_their_owners_colours`, just before `let base_atlas = grown.atlas.clone();`. It does `compile_growing(.., pixels=false ..)` + grow, then asserts that `tables.palette(UnitArmy name) == grown_too.palette(name)`.
- I have NOT run it: other agents' half-finished edits in tile-viewer, menu, ui_screen and city_screen kept breaking the build.
- I EXPECT IT TO FAIL. That failure is the real cause of the black colours on the Mac (next section).
- My own code compiled last time I could build. The tree's current compile state depends on the other agents.
- To land a green tree first: delete that block (search for "The driver of a renderer in another process compiles the"), or keep it as the regression test for the fix below.

## ROOT CAUSE of the black colours (found, NOT fixed)
- In remote/native mode (Mac: civ3r-native or the viewer), the bridge compiles TABLES ONLY (`live.rs`: `pixels = remote.is_none()`). The renderer compiles the pixels. "Both sides agree on nothing but names and values."
- render-desc `Growing::grow` gives a growth's new NAMED palettes the row `rows + own_new_rows`, i.e. after the growth's new OWN rows (FLC palettes).
- Without pixels there are no own rows (`Placer::add` with `cut == None`). So the bridge's palette row for a growth-added named palette is smaller than the renderer's. The cell then points at some other own/zero row, and remapped indices 0..127 come out black.
- Only growth-added named palettes are affected. The `UnitArmy@<table>a<first>x<members>` palettes are the only common case: ntp@/disc@ are in the base description. In-process Windows (pixels=true) is fine, which is why the CPU and GPU tests pass.

Planned fix (civ3-bridge only, no render-desc or renderer change): drop the generated UnitArmy named palettes.
- Draw army art with `remap = [0, 64]` and cell palette `ntp@<owner table>` (a base palette, so both sides agree).
- Bake entries 64..127 into the image's OWN row with loader options (civ3-assets/src/loader.rs options_palette):
  - `palette = "Art/Units/Palettes/ntpFF.pcx"`, `palette_range = [0, 0]`, `palette_map = { 64+16k+j = j for k < members }` (FF = civ 0's table);
  - `palette2 = "Art/Units/Palettes/ntpTT.pcx"`, `palette2_map = { ... for k >= members }` (TT = owner table).
- One image per (table, first, members). Sprite and set names get the kind `#army<table>a<first>x<members>`. Change `Recolor::palette()` to always return `ntp@<table>` and replace `art_kind(bool)` with a per-Recolor string.
- Also update preview.rs:406 (`"#army"` kind) and the description-palette loop in `Needed::entries`.
- Then adjust the tests: remap 0..64 for army art; the expected colours for 64..127 come from the own row (the test's `got` already reads s.palette for unremapped indices). Keep the tables-vs-pixels assertion, on sprite and set names and values instead of the palette.
- Check that own-row quantization matches the named ntp rows: the test compares exact colours.

## NOT done / next
- The fix above, then a full run: host tests (`cargo test -p civ3-bridge --lib`, civ3-sys), the i686 check, and the Wine bridge tests. Deploy (cross-compile on papa ONLY, never build on the Mac).
- Hidden-nationality armies: the game takes 64..127 from the REAL owner's table (army.CivID). We use the colour civ. Minor.
- Special units (task 3): not reviewed yet. Leaders (PRTO_Leader_ERAS_*), the scientific leader, alternative art, and transports with cargo still need checking.
- RoadsMovementRate is assumed to be 3 (General+0xE4 at 0x9C7288). Read it from BIC if it matters.
- A non-army's max moves use movement*3. The game uses 0x5CDD10(type, civ), which may add bonuses. Pre-existing.

## Reverse-engineering findings (Civ3Conquests.exe, the civ3-src copy)
- 0x5BC5C0(u, 0x12) = UTA_Army. Members: units on the army's tile with Container_Unit (U+0x60) == the army's ID.
- 0x5BE2C0 max_hp: army = sum(member max_hp) + army type Hit_Point_Bonus; no members = experience base + bonus; max(1).
- 0x5BE3F0 attack and 0x5BE530 defence: army = (sum + n/2)/n over members; no members = the type's (Army 0/0, so an empty army has NO bar). Non-army defence is halved if U+0x1ED and > 1.
- 0x5BE180 max moves: army = min(member max moves) + [0x9C7288] (General.RoadsMovementRate, one tile); else 0x5CDD10(type, civ).
- 0x5BA460 Unit_draw_hud: skips units inside an army (0x4378F0 container check). The bar uses max_hp and the ARMY's own Damage (U+0x4C). It is drawn if attack > 0 or defence > 0. The LED uses 0x5BE180.
- Palette (0x5CBD25, every army): owner's ntp[0..15] x4 into the FLC table entries 64..127. 0x5CC2C1 (non-empty army): n = min(U+0x1D4, 4), and civ 0's ntp[0..15] is written n times at 64.. (literal 0xA53F50 = leaders[0].Color_Table_ID).
- Army art (Art/Units/Army */Army*Default.flc) uses 0..15 (body civ colour) and 64+16k+{6..13}: four flags, one per member slot. Filled slots therefore show civ 0's colour (barbarian ntp00 = greys), the rest the owner's. This looks intended (a member-count display), not a bug.
- CCM2 scenario army art uses 64..127 heavily as ordinary colours. The game recolours them anyway, and so do we.

## Gotchas
- A unit placed later in world.units than its row order goes to free sprites (`slots`). Insert the army first in tests.
- The GPU and reference pictures differ in alpha outside sprites: compare RGB only.
[Credential-related line redacted from this historical copy.]
- Never compile on the Mac.
- My scratch dirs under /mnt/datasets/c3r-build/army-agent were removed. Use your own CARGO_TARGET_DIR.

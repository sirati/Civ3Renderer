# Tile information box ("Terrain details") picture: shadow and other drawing bugs — handover

## Findings ([X] = read in Civ3Conquests.exe / Conquests/jgl.dll with capstone)
- open_tile_info draws the picture with Map_Renderer 0x9C7560 slot +0x118 (0x5F4450 -> m20, vt+0x50)
  into the box canvas (form +0x274), forced normal zoom, then draws the TileInfo.pcx frame over it.
  Only a diamond window (x 68..188, y 24..88) of the 256x173 picture shows through the frame.
- Resource shadow bug: shadows go through the blend blit 0x5F8560 (-> 0x5F8620 -> jgl 0x100141E0) with
  tables [0x73D9F0]. The tracer (trace.rs blend hook -> ui_trace::on_map_sprite) recorded it as a plain
  colour-keyed blit, so the shadow showed in resources_shadows.pcx's palette colours (greens).
  [X] jgl 0x100141E0: index < 254 -> dst = table[(idx-248)<<15 | dst]; 254/255 not drawn.
  [X] 0x403D40 builds [0x73D9F0]: malloc 0x40000 = four 32768-entry RGB555 tables, c' = floor(c*k/16),
  k = 8, 10, 12, 14 (idx 248..251). Matches civ3.toml's measured rgba.
- Other bug (territory borders in the box): Sprite_draw_with_color_table (0x5F87E0) with blend tables
  [0x9C7304] and a stack-local colour table (owner's ntpNN entries 64,65 copied, 0x4C3B87..0x4C3BEC).
  The mirror drew them as plain keyed blits in Territory.pcx's own palette: the diamond filler index 1
  (which the game turns into 255 before slicing) drawn opaque grey, 64/65 in the file's colours,
  shades 249/252 as colours. [X] jgl 0x100159E0/0x10015BF1 (unscaled): idx<224 colour table,
  224..239 smoke, 240..254 shade level idx-240 (c*(idx-239)/16), 255 skip; clipping is proper
  (the "starts at first pixel" quirk in docs §3 is the scaled path 0x10015C90 only).
- Old trace /mnt/datasets/winvm/civ3r-runs/verify-viewer/M1-popup.txt (form 0x66f5c8 canvas 0xc95314)
  shows the box draws: terrain "map", "blend" 48x49 shadows, "colored" Territory_Images.

## Changes (all compile: host `cargo check -p civ3-bridge --tests` ok; i686 check NOT yet run)
- crates/menu/src/screen.rs: `Indices {transparent, shade}`, `Element::Indexed {region, indices, at, clip}`,
  `ScreenArt.indexed`, description (`ScreenIndexed@...` sheets, images with transparent/rgba options),
  sprites() (scale_den 16 for shades), ready/is_empty/add/minus/is_empty/covers. No to_lines/parse_lines.
- crates/civ3-bridge/src/ui.rs: `Source::Indexed`, `Event::IndexedBlit`, `ScreenDraw::Indexed`, handling in
  apply/crop_move/op_bounds/overdraws_itself/screen_draw/reaches_clip; `pub mod indices` (SHADOW_LEVELS,
  SHADE_LEVELS, blend(), colored(), colored_shades(), blank() = Territory.pcx 0,1); new model test
  `shadows_and_colour_table_sprites_keep_their_index_rules_through_clipped_canvas_copies`.
- crates/civ3-bridge/src/ui_trace.rs: `on_map_blend` (blend tables recognised -> IndexedBlit), `blend_levels`,
  `indexed_blit`, `on_colored(.., table, ct)` -> colour + shade IndexedBlits through shade tables,
  `copied_civ_colors` (ntpNN entries 64,2 by matching the ct's entries); watch() arm.
- crates/civ3-bridge/src/trace.rs: blend hook calls on_map_blend(table); colored passes call.table.
- crates/civ3-sys/src/ui_hooks.rs: color_table_entries, civ_table_with, BlendTables/blend_tables (PCGames).
- ui_screen.rs: ScreenDraw::Indexed -> Element::Indexed; retain_present_by, add_present_art, ArtRetry.
- Minimal `ScreenDraw::Indexed` match arms in city_screen.rs, interface_parts.rs (clip_mut, reach),
  label_check.rs, looks.rs.
- New test crates/civ3-bridge/src/ui_screen/tile_info_tests.rs (CPU game-blit reference vs mirrored frame
  -> items -> compiled art -> tile_render::reference): passes; before-fix 2107 px differ (89 shadow),
  after 0; unframed before 3249, after 0.

## PNGs (/mnt/datasets/c3r-build/tileinfo-render/)
tileinfo-game.png, tileinfo-before.png, tileinfo-after.png, tileinfo-unframed-{game,before,after}.png
(written with C3R_TILEINFO_EVIDENCE_DIR set). Helper scripts there: disasm.py, calls.py.

## State / next steps
- Full `cargo test -p civ3-bridge --lib`: 391 pass, 1 FAIL units::tests::armies_draw_in_their_owners_colours
  (units.rs:1763, another agent's area; not checked whether pre-existing — verify it fails without these changes).
- Not done: `cargo check -p civ3-bridge --target i686-pc-windows-gnu`, Wine bridge tests
  (tools/wine-run, `cargo test --target x86_64-pc-windows-gnu`), `cargo check --workspace --tests`, menu tests.
- Not handled: smoke indices 224..239 of non-FLC colour-table blits (left out); resized blend/colour blits
  (zoomed-out UI) still plain; direct 0x5F8620 callers (unhooked; e.g. 0x5AC98A etc., not the tile box)
  are not mirrored; preload art lists don't carry indexed regions.
- Target dir /mnt/datasets/c3r-build/tileinfo-render-target was deleted (rebuild needed).

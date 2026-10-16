# Civilization III: Conquests (v1.22 + C3X R21) - main map renderer, technical spec

Goal: enough detail to reimplement the main map view pixel-exactly, reading world state from game memory.

## 0. Evidence legend and method

Every claim carries one tag:

| Tag | Meaning |
|-----|---------|
| **[X]** | Confirmed by disassembling the game exe (`Civ3Conquests-Unmodded.exe`, 3,395,072 bytes, the PCGames.de 1.22 build; the installed C3X exe is byte-identical to it except ~1.6 KB of patches). Addresses are PCG addresses unless stated. |
| **[D]** | Confirmed against local data (PCX/FLC pixels, a decompressed SAV/BIQ). |
| **[H]** | Taken from the C3X reverse-engineered header (`Civ3Conquests.h`, `civ_prog_objects.csv`) and cross-checked: I compiled the header with `gcc -m32` (stubbing `windows.h`) and every offset I compared against disassembly matched (Map.Width 0x168, Map.Height 0x154, Map.Flags 0x1F0, Map.Seed 0x1EC, Renderer.Map 0x2E0, Leader.Color_Table_ID 0x10F8, BIC.Map 0x3E64, ...). |
| **[S]** | Stated by an external source (OpenCiv3/C7, CivFanatics, ...), not independently verified. |
| **[I]** | Inferred (plausible, not proven). |
| **[?]** | Open question. |

Local files used (read-only): `/mnt/datasets/winvm/civ3-src/Civilization III Complete/` (`Art/`, `Conquests/Art/`, `civ3PTW/Art/`, `Conquests/Civ3Conquests*.exe`, `Conquests/Civ3Conquests.h`, `civ_prog_objects.csv`, `injected_code.c`, `ep.c`, `conquests.biq`, `*.SAV`). Method: static x86 disassembly (capstone) of the unmodded exe, following the vtables named in the C3X header; PKWare-DCL ("blast") decompression of SAV/BIQ files; PCX/FLC decoding with Pillow/NumPy.

External sources:
- C3X (maxpetul): https://github.com/maxpetul/C3X - `Civ3Conquests.h`, `civ_prog_objects.csv`, `injected_code.c`, `ep.c`.
- OpenCiv3 / C7: https://github.com/C7-Game/Prototype - `C7/MapView.cs`, `C7Engine/TerrainTextureFiles.cs`, `C7/Lua/civ3/textures/*.lua`, `C7/Textures/PCXToGodot.cs`, `ConvertCiv3Media/ReadFlic.cs`, `QueryCiv3/SavSections/Tile.cs`, `C7/Map/UnitLayer.cs`.
- civ3mapreplay `civ3.h`: https://github.com/lucienmaloney/civ3mapreplay/blob/master/civ3.h (tile order, texture section/index bytes, 128x64, 9x9).
- CivFanatics "Converting civ3 maps to other formats" (Quintillus' editor dump of TILE fields: image, file, overlays, baseRealTerrain...): https://forums.civfanatics.com/threads/converting-civ3-maps-to-other-formats.550572/
- CivFanatics "The Civilization III Save Format": https://forums.civfanatics.com/threads/the-civilization-iii-save-format.48270/ ; Apolyton SAV thread: https://apolyton.net/forum/civilization-series/civilization-iii/49840-civilization-iii-sav-file-format ; Apolyton BIC thread: https://apolyton.net/forum/civilization-series/civilization-iii/59815-civilization-iii-bic-file-format-2nd-thread
- CivFanatics "Civ3/C3C/PTW Memory Structures": https://forums.civfanatics.com/threads/civ3-c3c-ptw-memory-structures.509903/

---

## 1. Which exe is installed [X][D]

`Conquests/Civ3Conquests.exe` is the C3X-modded exe (R21): 3,591,168 bytes, 7 PE sections: `.text .rdata .data .rsrc` identical in VA/size to the vanilla exe plus `.c3xinl .c3xdat .c3xtxt`. Its `.text`/`.rdata` differ from `Civ3Conquests-Unmodded.exe` in only 1574 + 78 bytes (the patches). `Civ3Conquests-Unmodded.exe` is 3,395,072 bytes = `pcg_binary.file_size` in `ep.c`, so the base build is **PCGames.de v1.22**, i.e. column 3 ("PCG ADDR") of `civ_prog_objects.csv`. PE image base 0x400000, `.rdata` VA 0x665000 size 0x1A400 (also matches `pcg_binary`). `Civ3Conquests-Original.exe` (6.9 MB, SecuROM) is the retail 1.22 and is not what runs.

How C3X detects the build (`ep.c`, `identify_binary`): by file size: GOG 3,417,464 (rdata 0x665000, column 1), Steam 3,518,464 (rdata 0x682000, column 2), PCG 3,395,072 (rdata 0x665000, column 3). A file is "modded" if it has a section named `.c3x*`; in that case the backup `Civ3Conquests-Unmodded.exe` is identified instead. The injected code gets the column via the constant `exe_version_index` (0 GOG, 1 Steam, 2 PCG; first data row of the csv) and passes it to Trade Net X (`set_exe_version`). For an external tool: read the PE headers of the running exe or `Civ3Conquests-Unmodded.exe`; presence of `.c3xinl` means modded; unmodded size picks the column. Struct layouts are the same in all three builds (one header serves all); only addresses differ.

Key globals (the csv "define" rows are the *addresses of static objects*, not pointers to them: e.g. `0x9C34C8` passed in ECX is the BIC object itself) [X]:

| Object | GOG | Steam | **PCG (installed)** |
|---|---|---|---|
| `BIC` (`p_bic_data`; Map embedded at +0x3E64) | 0x9C3508 | 0x9E5D08 | **0x9C34C8** |
| `Main_Screen_Form` (`p_main_screen_form`) | 0x9F8700 | 0xA1AF00 | **0x9F86C0** |
| `Leader leaders[32]` (stride 0x20E4) | 0xA52E98 | 0xA75698 | **0xA52E58** |
| `Units` object (`p_units`) | 0xA52E80 | 0xA75680 | **0xA52E40** |
| `Cities` object (`p_cities`) | 0xA52E68 | 0xA75668 | **0xA52E28** |
| `TileUnits` (`p_tile_units`) | 0xA52DD4 | 0xA755D0 | **0xA52D94** |
| `p_null_tile` (returned for out-of-range lookups) | 0xCAA330 | 0xCCCB98 | **0xCAA2F0** |
| `p_player_bits` / `p_human_player_bits` | 0xA526C0 / 0xA526BC | 0xA74EB8 / 0xA74EB4 | **0xA52680 / 0xA5267C** |
| `p_current_turn_no` | 0xA526AC | 0xA74EA4 | **0xA5266C** |
| `p_debug_mode_bits` | 0xA52680 | 0xA74E78 | **0xA52640** |
| `tile_at(x,y)` (cdecl function) | 0x437A70 | 0x439620 | **0x437AF0** |

Derived absolute PCG addresses (all [X], seen as literal addresses in disassembly): `bic.Map` 0x9C732C, `bic.is_zoomed_out` byte 0x9C7300, `bic.Map.Width` 0x9C7494, `bic.Map.Height` 0x9C7480, `bic.Map.Flags` 0x9C751C, MSF `TileX_Min/Max/TileY_Min/Max` 0x9FD530/34/38/3C, MSF `camera_x/camera_y` 0x9FD558/0x9FD55C, MSF `Player_CivID` 0x9FD47C.

---

## 2. Memory layout of the world state [H], verified [X] where marked

All offsets are hex, 32-bit process, little endian.

### 2.1 Map (`bic.Map`, at BIC+0x3E64 = PCG 0x9C732C)

| Field | Offset | Notes |
|---|---|---|
| `vtable` | 0x00 | `m10_Get_Map_Zoom` (slot 10, +0x28) returns `bic.is_zoomed_out` [X]; `m12_Get_Tile_by_XY` (slot 12, +0x30) |
| `TileCount` | 0x40 | 16-bit read in `Map_get_tile` [X] |
| `Tiles` | 0x148 | `Tile **` : array of `TileCount` pointers; each `Tile` is a separate 0xDC-byte object [X] |
| `Height` | 0x154 | in half-tile rows [X] |
| `Width` | 0x168 | in half-tile columns [X] |
| `Seed` | 0x1EC | per-map seed; feeds the decoration RNG (section 9) [X] |
| `Flags` | 0x1F0 | bit0 = wrap in X, bit1 = wrap in Y [X]; bit2 is tested by camera clamping [?] |
| `Renderer` (embedded `Map_Renderer`) | 0x234 | `Map_Renderer.Map` (0x2E0 in the renderer) points back to the Map |

`Map_get_tile(index)` (0x5D15D0) = `Tiles[index]` if `0 <= index < TileCount` else `p_null_tile`. `tile_at(x,y)` (0x437AF0) = `Map_get_tile(((Width/2)*y + (x>>1)) & 0xFFFF)` [X]. So tile storage order is row-major over `Width/2` tiles per row, index `i = (W/2)*y + (x>>1)`, inverse `y = i / (W/2)`, `x = (i % (W/2))*2 + (y & 1)` [X][S civ3mapreplay]. Valid tiles have `(x+y)` even; the other half of the (x,y) lattice are "vertices" (section 4).

### 2.2 Tile (0xDC bytes; `Tile.Body` at 0x3C)

| Offset | Field | Meaning |
|---|---|---|
| 0x00 | vtable | method table; `m41` (+0xA4) returns `SquareParts` [X] |
| 0x04 | `River_Code` (u8) | river edge bits, section 6.4 |
| 0x05 | `Territory_OwnerID` (u8) | civ id owning the tile, 0 = unowned [X][D] |
| 0x08 | `ResourceType` (i32) | index into `BIC.ResourceTypes`, -1 = none |
| 0x0C | `TileUnitID` | key into `TileUnits` |
| 0x10 | **`SquareParts`** (u16) | **`(TextureFile << 8) | TextureLocation`**: image file and cell for the *vertex above this tile* (section 5) [X] |
| 0x12 | `VictoryPoint` (u16) | |
| 0x14 | `field_14` | (SAV TILE bytes 14-19 hold VictoryPoint + this) |
| 0x18 | `Barbarian_TribeID` (i16) | -1 none [S QueryCiv3] |
| 0x1A | `CityID` (i16) | -1 none [X] |
| 0x1C | `Tile_BuildingID` (i16) | colony/airfield-ish tile building; `-1` none [X] |
| 0x1E | `ContinentID` (i16) | |
| 0x24 | `Ruins` | non-zero => ruins sprite [X] |
| 0x28 | **`Overlays`** (u32) | improvement bits, section 6.5 [X] |
| 0x2C | **`SquareType`** (u32) | bits 8-11 = underlying "real" terrain id (`m49_Get_Square_RealType`); bits 12-15 = effective terrain id (`m50_Get_Square_BaseType`; hills/mountain/forest/jungle/swamp/volcano/flood-plain/water are *this* value) [X] |
| 0x30 | `Terrain_Overlays` (u32) | bit 15 revealed-by-scenario, bit 16 bonus-shield/special, bit 20 mountain snow-cap, bit 21 forest uses pine art, **bit 29 landmark (LM) terrain**; bits 24/27 feed river logic (section 6.4) [X][D] |
| 0x34, 0x38 | `field_34`, `field_38` | `field_38` is overwritten with the 4-bit neighbour mask by the volcano drawer [X] |
| 0x3C | `Body` | `Tile_Body` |

Terrain ids (enum `SquareTypes`) [H][X]: 0 desert, 1 plains, 2 grassland, 3 tundra, 4 flood plain, 5 hills, 6 mountains, 7 forest, 8 jungle, 9 swamp, 10 volcano, 11 coast, 12 sea, 13 ocean. (For forest tiles, real = grass/plains/tundra, effective = 7; for plain grassland real = effective = 2.)

`Tile_Body` (starts at Tile+0x3C; `Base` is 0x1C bytes): `Fog_Of_War` @Body+0x1C (Tile+0x58) = **explored-by bitmask, bit = civ id** [X]; `FOWStatus` @+0x20, `V3` @+0x24, `Visibility` @+0x28 (Tile+0x5C/0x60/0x64) and a field at Body+0x94 (Tile+0xD0): OR of these = **currently visible to civ** bitmask [X, from the fog pass]; `Visibile_Overlays[32]` @Body+0x72: per-civ remembered overlay byte (read through `Tile::m42_Get_Overlays(civ)`) [H].

SAV TILE record (4 chunks per tile, lengths 36/12/4/128) maps 1:1 onto these fields [D]: chunk0 (36 B): river@0, owner@1, resource@4, topUnit@8, **textureLocation@12, textureFile@13**, flags(6)@14, barb@20, city@22, colony@24, continent@26, ruins@32; chunk1 (12 B): `Overlays`@0, `SquareType`@4 (so SAV byte 5 low nibble = real terrain, high nibble = effective), `Terrain_Overlays`@8. Verified on `MK 10.SAV`: `SquareType` bits 0-7 and 16-31 were zero on all 3200 tiles.

### 2.3 Other objects

- **Main_Screen_Form** (PCG 0x9F86C0, size 0x2FCA8): `Current_Unit` 0x4D74, `Selected_City` 0x4D7C, `Player_CivID` 0x4DBC, `TileX_Min/Max` 0x4E70/0x4E74, `TileY_Min/Max` 0x4E78/0x4E7C, `camera_x` 0x4E98, `camera_y` 0x4E9C, `Mode_Action` 0x4ECC, `mouse_x/y` 0x2E1B0/0x2E1B4 [H][X]. The map is drawn into an off-screen `PCX_Image` at MSF+0x274 (0x9F8934) [X].
- **Leader** (`leaders[civ]`, stride 0x20E4 [X]): `ID` 0x1C, `RaceID` 0x20, `CapitalID` 0x2C, `Status` 0x40, `GovernmentType` 0xA0, `Era` 0xF4, `Unit_Count` 0x18C, `Cities_Count` 0x194, **`Color_Table_ID` 0x10F8** (civ colour 0..31) [X: both `Era` and `Color_Table_ID` are read by the renderer at these offsets].
- **Race** (`BIC.Races`, BIC+0x3CC8; stride 0x974 [X]): `CultureGroupID` 0x90C [X], `DefaultColor` 0x930, `AlternateColor` 0x934, `EraAnimations` 0xEC [H].
- **Units**: `Units` object: `Units` (UnitItem*) at +4, `LastIndex` +0x10, `Capacity` +0x14; `UnitItem` = `{int, Unit_Body *}` (8 bytes), indexed by unit id (`get_unit_ptr` in C3X). Pointer targets are `Unit_Body` (a `Unit*` is body-0x1C). `Unit_Body`: `ID` 0x04, `X` 0x08, `Y` 0x0C, `PrevMoveX/Y` 0x10/0x14, `CivID` 0x18, `RaceID` 0x1C, `UnitTypeID` 0x24, `Combat_Experience` 0x28, `Status` 0x2C (0x04 attacked, 0x40 def. bombard, 0x80 sentry, 0x100 sentry-enemy-only), `Damage` 0x30, `Moves` 0x34, `Job_ID` 0x3C, `Container_Unit` 0x44, `UnitState` 0x48 (1 fortifying, 2 mine, 3 irrigate ... see `UnitStateType`), `automated` 0x4C, `leader_kind` 0x1D8, `always_on_top` 0x232, `Rect` 0x240, `Animation` (embedded `FLC_Animation`) 0x260 [H]. `UnitType` (BIC+0x3CD8, stride 0x138): `Name` 0x08, `Movement` 0x70, `Hit_Point_Bonus` 0xA4, `Unit_Class` 0x9C, `UnitAbilities` 0x88 [H].
- **Cities**: `Cities` object same shape as `Units`; `CityItem = {int, City_Body *}`. `City_Body`: `ID` 0x04, `X` 0x08 (i16), `Y` 0x0A (i16), `CivID` 0x0C (u8), `Status` 0x14, `Buildings` 0x8C, `Citizens` 0xC0, `Population` 0xFC (`Population.Size` at **body+0x11C**, = City+0x138 [X]), `Improvements_1/2` 0x258/0x29C, `CityName` 0x1C4 (char[20]) [H][X]. A `City*` is body-0x1C.
- **BIC**: `ErasCount` 0x890, `ResourceTypeCount` 0x89C, `Player_Count` 0x8A4, `UnitTypeCount` 0x8A8, `RacesCount` 0x8AC, `TileTypesCount` 0x8BC; arrays `Races` 0x3CC8, `Governments` 0x3CD0, `ResourceTypes` 0x3CCC, `Players` 0x3CD4, `UnitTypes` 0x3CD8, `General` 0x3CDC, `TileTypes` 0x3E20; `is_zoomed_out` byte 0x3E38; `Map` 0x3E64 [H]. `ResourceType` (0x5C bytes): `Class` 0x3C (0 bonus, 1 luxury, 2 strategic), `IconID` 0x48.
- **General** (BIC+0x3CDC): `MaximumSize_Town` +0x100, `MaximumSize_City` +0x104 = the city-size thresholds used for city art (section 8) [X].

---

## 3. Map geometry [X unless noted]

- **Tile size 128 x 64** (diamond). The lattice cell (cx, cy) (any integers, **both parities**) has a 128x64 box whose top-left is `(cx*64, cy*32)` in map pixel space. A tile (cx+cy even) box is its own diamond; an odd cell is a *vertex* cell: the point where four tiles meet, box centred on that vertex. Tile (x,y)'s top vertex is the odd cell (x, y-1); its right vertex is (x+1, y), bottom (x, y+1), left (x-1, y).
- **Two zoom levels only**: `bic.is_zoomed_out == 0` -> lattice step 64 x 32 (box 128x64); `!= 0` -> step **32 x 16** (box 64x32) [X: `(0x80 or 0x40)>>1` per axis]. Every sprite draw receives a scale argument 1 or 2 (`Sprite_draw_on_map(spr, renderer, x, y, 1, 1, zoom?2:1, 0)`) and all per-layer pixel offsets are halved in zoomed-out mode (e.g. mountain -24 -> -12) [X]; so zoomed-out = same art downscaled by 2 [I for the filtering].
- **Camera**: `camera_x/camera_y` (MSF+0x4E98/0x4E9C) are **pixels at the current zoom**, the map-pixel coordinate of the viewport's top-left. The function that converts tile -> pixel (`Main_Screen_Form` method 0x4E3BD0; the same arithmetic is inlined in the draw loop):

```
stepX = zoomed ? 32 : 64 ; stepY = zoomed ? 16 : 32
dx = x - TileX_Min ;  if (Map.Flags&1) dx = wrap(dx) into (-W/2 .. W/2]   // subtract/add W
dy = y - TileY_Min ;  if (Map.Flags&2) dy = wrap(dy) similarly
px = (dx - 1) * stepX - (camera_x % stepX)
py = (dy - 1) * stepY - (camera_y % stepY)        // top-left of the cell's box, relative to the map view
```
  `TileX_Min/Max/TileY_Min/Max` are set by the camera setter `Main_Screen_Form` method 0x4DF7C0 (the function that stores `camera_x/camera_y`) [X]:
```
L,T,R,B = screen-space rectangle of the map viewport (globals 0x9CB3FC, 0x9CB400, 0x9CB404, 0x9CB408 in the PCG build)
TileX_Min = (L + camera_x) / stepX - 1       TileX_Max = (R + camera_x) / stepX + 1      // C integer division
TileY_Min = (T + camera_y) / stepY - 1       TileY_Max = (B + camera_y) / stepY + 1
```
  So `camera_x/camera_y` are **pixels at the current zoom: map-space coordinate of screen position (0,0)**, i.e. a lattice cell's box has top-left **screen position `(cx*stepX - camera_x, cy*stepY - camera_y)`**. The renderer works in viewport-relative canvas coordinates, `px = (cx - TileX_Min - 1)*stepX - camera_x % stepX`, which equals `cx*stepX - camera_x - L` whenever `L` is a multiple of `stepX` (the viewport's screen offset is then added back by the final blit) [X]. With X-wrapping `camera_x` is wrapped into about `[0, stepX*Width]`; Y is wrapped likewise or clamped to the map height minus the viewport height when Y does not wrap (code at 0x4DF8C7-0x4DF96A) [X]; `Map.Flags` bit 2 selects an alternative clamp for the non-wrapping case [?]. The viewport rectangle itself is set by the UI layout (initial values are runtime-only; read the globals) [?].
- The inverse (screen -> tile) is `Main_Screen_Form_get_tile_coords_under_mouse` (PCG 0x4E3D20).
- **Horizontal wrapping**: `Map.Flags & 1`. All neighbour lookups use the wrap helpers `Map_wrap_horiz` (0x426C80) / `Map_wrap_vert` (0x426CC0); out-of-range (after wrapping) cells are skipped by every layer. With wrapping the same tile is drawn at several screen positions; the draw loop just iterates the visible lattice range and wraps each coordinate [X].
- **Draw loop rectangle**: rows `TileY_Min..TileY_Max`, columns `TileX_Min..TileX_Max` (`Main_Screen_Form` fields), both inclusive, over *all* lattice cells; per cell the wrapped coordinates are tested for validity.

---

## 4. Frame composition and draw order [X]

Per frame the main form (function 0x4EED00) calls, in this order:

1. `Map_Renderer::m71_Draw_Tiles(civ = Player_CivID, canvas = MSF+0x274, 0)` -> `m20/m21` loops below.
2. **Fog-of-war pass** (function 0x4C4F10, section 6.8) over the same rectangle.
3. Units (section 10): visible units collected per tile (`Main_Screen_Form_find_visible_unit`), drawn afterwards.
4. HUD/labels/city names etc.

`m71` calls `Draw_Tiles_by_Flags` (`m21`, vt+0x54, PCG 0x4C3230) **five times** (function 0x5F44A0), each time sweeping the whole rectangle (`for y in Ymin..Ymax { for x in Xmin..Xmax {...} }`, **row-major, y outer, x inner**), with these flag words (the second operand is `~Map_Renderer.Flags & mask` when `m5` says layers may be hidden; `Map_Renderer.Flags` (+0x2DC) is therefore a layer-hide mask [X for the masking, I for its use]):

| Pass | Flags | What it draws (see per-cell table) |
|---|---|---|
| 1 | `0x09` | odd cells: base terrain (0x01) + flood plains (0x08) |
| 2 | `0x04` | odd cells: rivers that touch a "special" tile (city, fortress, mountain, ...) |
| 3 | `0x1FEF0` | **even cells (real tiles)**: all object layers (table below) |
| 4 | `0x02` | odd cells: all other rivers (drawn over the objects) |
| 5 | `0x100` | map grid lines (if grid on) |

Per cell, `Draw_Tile_by_XY_and_Flags` (`m19`, vt+0x4C, 0x5F4610) first masks the flags by parity: **odd cell** keeps bits 0-3 and 17-31 (`& 0xFFFE000F`), **even cell** clears bits 0-3 (`& ~0xF` of the low byte) and keeps the rest. It then calls the layer methods **in this fixed order** (flag -> method):

| # | Flag(s) | Layer (Map_Renderer vtable slot) | Cell parity |
|---|---|---|---|
| 1 | 0x01 | terrain image (m6) | odd (vertex) |
| 2 | 0x4010 | mountains (0x10, type 6), volcano (0x10, type 10), hills (0x4000, type 5) (m10) | even |
| 3 | 0x0800 | irrigation (m11) | even |
| 4 | 0x0E | rivers / flood plains (m15; bit3 flood plains, bit2/bit1 river passes) | odd |
| 5 | 0x40 | "tnt" ground decorations (m18) | even |
| 6 | 0x20 | forest / jungle / swamp (m8) | even |
| 7 | 0x60 | landmark (LM) terrain overlay (m58, if `Tile::m30` is set) | even |
| 8 | 0x200 | territory borders (m17) | even |
| 9 | 0x400 | roads (m16) then railroads (m14) | even |
| 10 | 0x80 | tile buildings: ruins, airfield, mine, barricade, fortress, goody hut, barbarian camp, colony, radar, outpost (m12) | even |
| 11 | 0x8000 | pollution / craters (m13) | even |
| 12 | 0x1000 | resources (m9) | even |
| 13 | 0x2000 | city (m25 via m7) and victory point (m66) | even |
| 14 | 0x10000 | (null slot) | |
| 15 | 0x100 | grid (m35/m36) | |

Because pass 3 walks rows top to bottom, tall sprites (mountains 88 px, forests 88 px, cities) are overpainted by tiles of later rows, and within a row by larger x. A faithful reimplementation must draw **cell by cell in this order**, not layer by layer, for pass 3. All sprites are drawn at **(px, py) = the cell's top-left** plus the per-layer offsets below, with the zoom scale argument.

Every per-layer method also receives `civ` (m19's first argument = viewing civ id; 0 = see everything/debug) and uses `Tile::m42_Get_Overlays(civ)`-style per-civ views (`Visibile_Overlays`) for improvements [X for roads/rails, H otherwise]. The `m19` fifth argument is a per-cell visibility word computed in `m21`: for an even tile it is "explored by civ" (0/1), for an odd vertex it is a 4-bit mask of which of the four surrounding tiles are explored by civ (`Tile.Body.Fog_Of_War` bit test, with `civ <= 0` meaning explored) [X].

C3X confirms the geometry independently: `patch_Map_Renderer_m19...` draws its 128x64 `TileHighlights.pcx` cells with `Sprite_draw_on_map(sprite, renderer, pixel_x, pixel_y, ...)` only for `(tile_x+tile_y)%2 == 0` [S].

---

## 5. Base terrain

### 5.1 Sheets [X][D]

Nine 1152 x 576 PCX, 9 columns x 9 rows of 128x64 cells = 81 images each. File index (= `SquareParts >> 8`): **0 `xtgc`, 1 `xpgc`, 2 `xdgc`, 3 `xdpc`, 4 `xdgp`, 5 `xggc`, 6 `wCSO`, 7 `wSSS`, 8 `wOOO`**, loaded from `Art/Terrain/<name>.pcx` (loader loops `y 0..0x240 step 0x40`, `x 0..0x480 step 0x80`; image index = row*9+col). Landmark variants: `lxtgc, lxpgc, lxdgc, lxdpc, lxdgp, lxggc, lwCSO, lwSSS, lwOOO` (in `Conquests/Art/Terrain/`), selected when the tile's `Terrain_Overlays` bit 29 is set (`isLM` argument). Sprite for (file f, location l) = `Std_Terrain_Images[f][l]` at Map_Renderer+0x102D0 + (f*81+l)*0x2C, LM at +0x191A4 [X].

Letters: t tundra, g grass, p plains, d desert, c coast; w-sheets: C coast, S sea, O ocean.

### 5.2 Which image a vertex uses [X][D]

- The terrain is drawn per **odd cell (vx, vy)**, i.e. per vertex. The image comes from `SquareParts` of the tile **(vx, vy+1)**, the tile directly *below* the vertex (the vertex is that tile's top corner) [X: `m6` reads `Map.get_tile(x, y+1)->m41_Get_Square_Parts`]. `file = SquareParts >> 8`, `loc = SquareParts & 0xFF`. Skipped if `(vx, vy+1)` is out of the map (after Y-wrap).
- Game memory therefore already holds the final choice per tile: **read `Tile.SquareParts` (0x10) directly; no recomputation is needed.** (Identical to the SAV TILE `textureFile/textureLocation` bytes, and to the BIQ TILE `file/image` bytes.)
- The stored value is a pure function of the four tiles around the vertex: tile T=(x,y), N=(x, y-2), NW=(x-1, y-1), NE=(x+1, y-1) [D: 27,780 tiles from 8 saves, zero violations]. Terrain ids used are the **real** (underlying) ids, `SquareType` bits 8-11, not the hill/forest overlay.

```
digit(file, terrain):  // position of the terrain within the file's three letters
  xtgc: tundra 0, grass 1, coast 2        xpgc: plains 0, grass 1, coast 2
  xdgc: desert 0, grass 1, coast 2        xdpc: desert 0, plains 1, coast 2
  xdgp: desert 0, grass 1, plains 2       wCSO: coast 0, sea 1, ocean 2
  xggc: grass 0 OR 1 (two grass variants, chosen randomly per slot), coast 2
loc = digit(N) + 3*digit(NW) + 9*digit(NE) + 27*digit(T)            // 0..80
wSSS / wOOO (all sea / all ocean): loc = random 0..80
```
  (this is the same base-3 scheme as C7's `TerrainTextureFiles.cs`; I verified it on real saves, including `xggc` digits in {0,1}). Which *file* is picked is not a function of the terrain set alone when several sheets contain all needed terrains (e.g. grass+coast can use xpgc, xdgc, xggc or xtgc); the game stores a random compatible choice. Observed rule over the four tiles' real terrains [D]: all ocean -> wOOO; all sea -> wSSS; only coast/sea/ocean (with at least one coast, or sea+ocean) -> wCSO; any tundra -> xtgc; {D,G,P} -> xdgp; {C,D,P} -> xdpc; {C,D,G} -> xdgc; {C,G,P} -> xpgc; {C,G,T} -> xtgc; pure grass -> xggc; ambiguous sets (single D or P; D+P; D+G; P+G; C+D; C+P; C+G) occur in several sheets (e.g. C+G in xpgc, xdgc, xggc or xtgc), so the sheet is a random compatible pick made at map generation. If you must regenerate it (terraforming while playing), pick any consistent sheet and apply the formula above.
- **Position**: drawn at the vertex cell's top-left `(px, py)`, i.e. the image is centred on the vertex = half a tile above tile T's box centre. No extra offset [X: `m24` passes (px,py) straight to `Sprite_draw_on_map`].
- **Polar ice caps** (`polarICEcaps-final.pcx`, 1024x256, 8x4 cells of 128x64): for tiles in the top two and bottom two map rows `m6` also draws an ice sprite: index `rand_int(8) + (y==1 ? 8 : 0)` for the top rows, `16 + rand_int(8) + (y==Height-2 ? 8 : 0)` for the bottom rows (`rand_int` from the per-vertex RNG); exact pixel placement not decoded [X for index, ? for position].
  - **Correction (verified) [X][D]:** the ice cap is drawn by `m6` itself (`0x5F48C0`, Map_Renderer vtable slot 6), at its end, through `this->vtbl[+0xBC]` (`0x4C3A00`; call sites `0x5F4A71` top, `0x5F4AFA` bottom). It is keyed on the **vertex cell (vx, vy)** that `m6` was called with, not on the tile below it. Condition: `!(Map.flags(0x1F0) & 2)` (no Y-wrap) and `Map.flags & 4`, and `vy` in {0, 1, Height-2, Height-1}. It runs even when the base terrain was skipped (e.g. `vy = Height-1`, whose tile below is off the map). RNG: `a = ((Width/2)*vy + (vx>>1)) & 0xFFFF`, `state = Seed + a*21031`, then **one** `rand_int(8)` with **no** 4-advance warm-up (`r = (((state*0x41C64E6D+0x3039) >> 16) & 0x7FFF) >> 12`). Index = `r` (vy=0), `8+r` (vy=1), `16+r` (vy=Height-1), `24+r` (vy=Height-2). Position: with `(px, py)` = the cell's base-terrain top-left, top rows draw at `(px-64, py-32)` and bottom rows at `(px-64, py+32)` (halved to 32/16 when zoomed out, `Map` vfunc +0x28 = `0x5D1600` returns the zoom byte 0x9C7300). Draw order: right after that cell's base terrain sprite, inside the same `m6` call, so before every later layer. Checked with `/tmp/polar-verify.py` against traces t4 and t6 (two seeds): 66/66 draws match on index and position.
- The unexplored/fog look is *not* baked into terrain; see section 6.8.

---

## 6. Overlay layers

All sprite sheets are loaded by the renderer initialiser (vtable slot 72, PCG 0x4C5A30); the loops below are read from it. `Sprite_slice_pcx(pcx, x, y, w, h, 1, 1)`. Asset resolution: see section 12. Transparent pixels: section 11.

### 6.1 Mountains, volcano, hills (m10) [X]

| Sheet | Grid | Cell | Count |
|---|---|---|---|
| `Mountains.pcx`, `mountain forests.pcx`, `mountain jungles.pcx`, `Mountains-snow.pcx` (512x352) | 4x4 | 128x88 | 4 x 16 (array `Mountains_Images`, contiguous: index = `kind*16 + mask`, kinds 0 plain, 1 forested, 2 jungled, 3 snow) |
| `Volcanos.pcx`, `Volcanos forests.pcx`, `Volcanos jungles.pcx`, `Volcanos-snow.pcx` (512x352, `Conquests/Art/Terrain`) | 4x4 | 128x88 | same scheme |
| `xhills.pcx`, `hill forests.pcx`, `hill jungle.pcx` (512x288) | 4x4 | 128x72 | 3 x 16 (index `kind*16+mask`, kinds 0,1,2) |
| `LMMountains.pcx`, `LMHills.pcx` | same | | LM variants |

- **mask** (4 bits): bit0 NW, bit1 NE, bit2 SW, bit3 SE neighbour at (x-1,y-1),(x+1,y-1),(x-1,y+1),(x+1,y+1) is mountains/hills/volcano (effective type 6/5/10) *and* passes the **edge hash** `keep(mx,my,n)` where `(mx,my) = ((x+nx)/2, (y+ny)/2)` of the shared edge: for n in {NW,SE}: `(3mx-my)%5 != 0 && (mx-2my)%5 != 0`; for n in {NE,SW}: `(3mx-my)%7 != 0 && (mx+2my)%5 != 0 && (7mx+my)%11 != 0`. (The same hash prunes irrigation links and decides border "hilly" segments.) Odd mask semantics in C7 (`getMountainIndex`) match without the hash [S].
- **kind** (vegetation tint): mountains with snow-cap bit (`Terrain_Overlays` bit 20, only for effective type 6) -> kind 3 (snow). Otherwise look at the 4 diagonal neighbours (NE, SE, SW, NW order): if any neighbour that exists is *not* hills/mountain/volcano/forest/jungle -> kind 0; else count forests (type 7) and jungles (type 8): both zero -> 0; forests > jungles -> 1; jungles > forests -> 2; tie -> `rand_int(2)` (0 -> kind 1, else kind 2) using the per-tile RNG (section 9; seed `Seed + 121472*a`, 4 advances then one `rand_int`). Snow sets kind 3 only for mountains; volcanoes use kinds 0-2 only; hills use kinds 0-2.
- **Position**: mountains and volcanoes `(px, py - 24)` (zoomed-out `-12`), hills `(px, py - 8)` (`-4`): the sprites are bottom-aligned to the 64-px tile box (88-64 = 24, 72-64 = 8).
- The volcano drawer stores the mask into `Tile.field_38`.

### 6.2 Forest, jungle, swamp (m8) [X][D]

For tiles with effective type 7, 8, 9. `forests.pcx` layout (1000x884; all cells 128x88):

| File | Rows (y) | Contents |
|---|---|---|
| `grassland forests.pcx` | 0 / 176 / 352 / 528 / 704 | jungle large 4x2 (8), jungle small 6x2 (12), forest large 4x2 (8), forest small 5x2 (10), forest **pines** 6x2 (12) |
| `plains forests.pcx`, `tundra forests.pcx` | same layout; only forest large/small/pines used (kinds 1 / 2) | |
| `marsh.pcx` (Conquests) | 0 / 176 | swamp large 4x2 (8), swamp small 5x2 (10) |
| `LMForests.pcx` | 0 / 176 / 352 | LM large 4x2, small, pines 6x2 |

Selection:
1. `kind` (which forest file): from the underlying terrain: real 1 (plains) -> plains file, 2 (grass) -> grass file, 3 (tundra) -> tundra file; real 9 or 10 -> grass file; real 0 desert falls through to garbage (cannot occur).
2. `small` flag: true if any of the 4 diagonal neighbours (NE, SE, SW, NW) is **coast (id 11 only; sea/ocean do not count)**, or if one of the two *northern* neighbours (NE, NW) is hills/mountain/volcano.
3. Seed `rng = Seed + 1313*a` with `a = ((W/2)*y + (x>>1)) & 0xFFFF`; 4 advances; then exactly one `rand_int(n)` for the variant: forest: pines (Terrain_Overlays bit 21) -> `rand_int(12)` from the pines block; else `small` -> `rand_int(10)`; else `rand_int(8)`. Jungle: `small ? rand_int(12) : rand_int(8)`. Swamp: `small ? rand_int(10) : rand_int(8)`.
4. Sprite index into the block's sprite array = the random value (block cells are row-major in the sheet: jungle-large cell k at col k%4,row k/4; small blocks have 5 or 6 columns).
5. Position: **`(px, py - 12)`** for all of them (128x88 centred on the tile box, 12 above).

### 6.3 Irrigation (m11/m38) [X][D]

Four sheets, each 512x256, 4x4 cells of 128x64: `irrigation DESETT.pcx`, `irrigation PLAINS.pcx`, `irrigation.pcx` (grassland), `irrigation TUNDRA.pcx` -> arrays at Map_Renderer+0xE120/0xE3E0/0xE6A0/0xE960 (desert, plains, grass, tundra: index `kind*16+mask`). Drawn only if the tile has the irrigation overlay bit (Overlays bit 3). `mask`: bit n set if the diagonal neighbour n (0 NW, 1 NE, 2 SW, 3 SE) also has irrigation and passes the same edge hash as in 6.1 (including the %5/%7/%11 conditions). `kind` from the effective terrain (desert 0, plains 1, grass 2, tundra 3; anything else uses grass). Position `(px, py)`.

### 6.4 Rivers, flood plains, waterfalls [X][D]

- Rivers are drawn **per vertex** (odd cell (vx,vy)). `River_Code` bit order (SAV/memory) is 0 N, 1 NE, 2 E, 3 SE, 4 S, 5 SW, 6 W, 7 NW; the bit b of tile T and bit (b+4)%8 of the neighbour at offset (N(0,-2), NE(+1,-1), E(+2,0), SE(+1,+1), S(0,+2), SW(-1,+1), W(-2,0), NW(-1,-1)) are always set together [D: 982/982]. The renderer only reads the diagonal bits; the cardinal bits 0x01/0x04/0x10/0x40 are not used by the drawing code.
- For the vertex V=(vx,vy) let N=(vx,vy-1), E=(vx+1,vy), S=(vx,vy+1), W=(vx-1,vy) (the four tiles around it) and build a 4-bit mask (value = river edge leaving V in that direction):
```
mask |= 1 if N.river & 0x20 (SW) or W.river & 0x02 (NE)      // edge going up-left   (NW)
mask |= 2 if N.river & 0x08 (SE) or E.river & 0x80 (NW)      // up-right             (NE)
mask |= 4 if W.river & 0x08 (SE) or S.river & 0x80 (NW)      // down-left            (SW)
mask |= 8 if E.river & 0x20 (SW) or S.river & 0x02 (NE)      // down-right           (SE)
```
  (identical to C7 `RiverLayer.DeriveTextureIndex`; verified in disassembly. Cells that are off-map are skipped; a contribution whose source tile is a **flood plain** (effective type 4) also goes into a second mask `fp`.)
- **Flood plains**: if `fp != 0` and flags&8 (pass 1): draw `floodplains.pcx` cell `fp` (512x256, 4x4 of 128x64, index = row*4+col = fp) at `(px,py)`; then the river variant is forced to 0.
- **River sprite**: `sprite[mask*2 + variant]` in the interleaved array at Map_Renderer+0x1801C where **variant 0 = `deltaRivers.pcx`** (narrow) and **variant 1 = `mtnRivers.pcx`** (broad); both 512x256, 4x4 cells of 128x64, cell index = row*4+col = mask [X: loader writes deltaRivers to even slots with stride 0x58]. Drawn only if `mask != 0` (or the vertex has special terrain bits 24/27, [?] unverified effect). Variant: if `popcount(mask) == 1` (river end/spring): `variant = 1` unless any of the four tiles has real terrain *coast* (id 11), then 0; otherwise `variant = rand_int(2)` from the vertex RNG (seed `Seed + 0x845FED*a`, `a = ((W/2)*(vy-1) + (vx>>1)) & 0xFFFF`, 4 advances then one `rand_int(2)`), forced to 0 if flood plain contributed. Position `(px,py)` of the vertex cell.
- **Pass**: let `special` = the tile below the vertex (S) is a mountain (6) / volcano (10), or W is one and has river bit 0x02, or E is one and has river bit 0x80 (N never counts) [D: 111 rivers, 3 games; the doc's earlier "any of the four" was wrong], or any of the four tiles has a city, colony, airfield, radar, outpost, fortress, barricade, goody hut, barbarian camp or ruins. If `special` the river is drawn in pass 2 (flags&4, *before* objects so buildings/mountains cover it), else in pass 4 (flags&2, *after* objects).
- `waterfalls.pcx` (512x64, 4 cells of 128x64) is loaded into `Waterfalls_Images[4]` [X]; I did not find the selection logic [?].

### 6.5 Overlay bits (`Tile.Overlays`, per-civ view via `Visibile_Overlays`) [X][D]

bit 0 road, 1 railroad, 2 mine, 3 irrigation, 4 fortress, 5 goody hut, 6 pollution, 7 barbarian camp, 8 craters, 28 barricade, 29 airfield, 30 radar tower, 31 outpost. (Matches SAV flags2: road bit0 ... byte3 bit4 barricade, bit5 airfield, bit6 radar, bit7 outpost.) `Tile_BuildingID != -1` = colony/other tile building [X].

### 6.6 Roads and railroads (m16 / m14 -> m52 / m50) [X][D]

- `roads.pcx`: 2048x1024, 16x16 cells of 128x64 = 256 images. `railroads.pcx`: 2048x1088, 16x17 = **272** images (row 16 = 16 extra images, index 256..271). Sprite `Roads_Images[i]` (+0x8424), `Railroads_Images[i]` (+0x5564), drawn at `(px,py)` of the tile.
- Bit i of the index = connection to neighbour i+1 of the spiral numbering `neighbor_index_to_diff` (0x5E6D80): 1 NE(+1,-1), 2 E(+2,0), 3 SE(+1,+1), 4 S(0,+2), 5 SW(-1,+1), 6 W(-2,0), 7 NW(-1,-1), 8 N(0,-2) => **bit0 NE, bit1 E, bit2 SE, bit3 S, bit4 SW, bit5 W, bit6 NW, bit7 N** [X]. Pixel check: `roads.pcx` cell 1 has its stroke in the upper-right, 2 horizontal right, 4 lower-right, 8 vertical down, 16 lower-left, 32 horizontal left, 64 upper-left, 128 vertical up [D].
- Road mask: for each of the 8 neighbours inside the map: set the bit if the neighbour has a road (`Check_Roads(civ)`), **except** when both this tile and the neighbour have railroad (then that link is a railroad link). If mask == 0 and the tile has no city and no railroad: draw sprite 0 (the isolated road stub); if a city is present nothing.
- Railroad mask: same with `Check_Railroads`; for tiles whose rail neighbours... the 16 extra images for rails are used for rail-over-road junction cases; the exact rule for indices 256-271 was not decoded [?]. Roads are drawn before railroads in the same cell.
- Roads/rails respect the *viewing civ's* remembered overlays (civ id argument).

### 6.7 Fortress, mines, camps, airfield, radar, outpost, pollution, craters, ruins, goody huts, tnt, resources

`TerrainBuildings.PCX` (Conquests version 512x256; the PTW base one is 384x256): 4x4 cells of 128x64 [X]: col 0 = fortress, rows 0-3 = eras (`Terrain_Buldings_Fortress[4]`, +0xB184); col 1 = colony/camp rows 0-3 (`Terrain_Buldings_Camp[4]`, +0xB0D4); col 2 row 0 = barbarian camp (+0x3EB4), col 2 row 1 = **mine** (+0x3EE0); col 3 = barricade rows 0-3 (+0xB234). Era row choice is [?] (likely the owner's era). All drawn at `(px,py)`; mine at `(px,py)`.

`x_airfields and detect.PCX` (+ `X_AIRfields and detect_shadow.PCX`; in `civ3PTW/Art/Terrain` and scenario folders): airfield 2 cells 128x64 at y=0 (x 0,128); outposts 3 cells 128x128 at y=64 (x 0,128,256); radar 1 cell 128x128 at (0,192); shadow file has the same layout. [X for slicing; ? for placement offsets].

`pollution.pcx` and `craters.pcx` (640x320): 5x5 = 25 cells of 128x64 each. Index: 4-bit mask of diagonal neighbours that also have pollution/craters; `mask != 0 -> index mask + 9` (10..24), else `rand_int(10)` (0..9); seed `Seed + 21030*a` [X for formula; ? for neighbour bit order, assumed NW,NE,SW,SE].

`goodyhuts.pcx` (384x192): 8 cells of 128x64 in a 3-column grid (cell i at `x = (i%3)*128, y = (i/3)*64`); chosen by `rand_int(...)` [X; seed in `m12`: `Seed + 0x20C7D*a`].

`tnt.pcx` (384x384): 6 rows x 3 cells of 128x64 (small ground decorations: tufts, rocks, birds). Variant `rand_int(3)` (seed `Seed + 64536*a`); row by effective terrain: desert -> file row 2, plains -> file row 1, grassland -> file row 0 (special/bonus grass variant [?]), tundra -> file row 3, flood plain -> file row 4 (file row 5 is used by another branch [?]) [X partially]. Drawn at `(px,py)`.

`StartLoc.pcx` (128x64) is the editor start-location marker (not part of game rendering).

**Resources** (`Art/resources.pcx` + `resources_shadows.pcx`, 300x300): 6x6 grid, each cell **49x49 at (1+50*col, 1+50*row)**; column index lines (x = 0,50,...) are palette index 254 [D]. `icon = BIC.ResourceTypes[Tile.ResourceType].IconID` (BIQ GOOD record: name[24], pedia[32], class, appearance, disappearance, icon, prereq, food, shield, commerce; conquests.biq: Horses 0, Iron 1, Saltpeter 2, Coal 3, Oil 4, Rubber 5, Aluminum 6, Uranium 7, Wines 8, Furs 9, Dyes 10, Incense 11, Spices 12, Ivory 13, Silks 14, Gems 15, Whales 16, Game 17, Fish 18, Cattle 19, Wheat 20, Gold 21, Tropical Fruit 22, Oasis 23, Sugar 24, Tobacco 25) -> `col = icon%6, row = icon/6` [X][D]. Draw position: icon top-left = `(px + 64 - 24, py + 32 - 24) = (px+40, py+8)` (zoomed-out `(px+20, py+4)`; i.e. `49/2 = 24`, `49/4 = 12` offset for scale 1/2) [X]. The shadow sprite (same cell index, `ResourcesShadows`) is drawn first at the same position, then the icon [X: 0x4C4AE0 draws the shadows array first]. Condition: `Tile_get_resource_visible_to(civ) != -1` (player knows the tech that reveals it) unless debug bit 8 [X].

### 6.7a Blending in RGB555 (measured) [D]

The game composes into an RGB555 frame buffer; translucent sprites darken
through blend tables. Measured from screenshots against our unblended
render (5-bit channel values c):
- Fog of war (`FogOfWar.pcx`, palette index i = 0..15 of the grey ramp):
  `c' = floor(c * i / 16)` (95.7 % of pixels exact by this alone; the
  rest were inference noise / borders).
- Resource shadows (`resources_shadows.pcx`, indices 248..251):
  `c' = floor(c * k / 16)` with k = 8, 10, 12, 14 (99.8 % of
  non-occluded shadow pixels exact, WW2 Pacific).
- Screen values are the 5-bit values expanded by bit replication
  (`(c << 3) | (c >> 2)`).

### 6.8 Fog of war / unexplored (pass after the tile sweep) [X][D]

`FogOfWar.pcx` (1152x576, 9x9 cells of 128x64 = 81 images, `Fog_Of_War_Images[81]` at +0x41F8). Function 0x4C4E60 builds the index from two 4-bit masks over the **four tiles around an odd vertex cell** (bit order N=0? see below): per corner k = 0..3, digit d_k = **2** if that tile is *currently visible* to the viewer, **1** if explored but not visible (fogged), **0** if unexplored; `index = d0 + 3*d1 + 9*d2 + 27*d3` (weights 1,3,9,27). **Corner order k = 0,1,2,3 = N, W, E, S** of the vertex, i.e. tiles `(vx, vy-1)`, `(vx-1, vy)`, `(vx+1, vy)`, `(vx, vy+1)` [X: the explored-mask loop in `m21` (0x4C34EA) uses x' = vx + k/2 - (k&1), y' = vy + (k+1)/2 - 1; D: the alpha centroids of cells 1/3/9/27 sit south/east/west/north of the diamond centre, i.e. the lit corner is N/W/E/S]. The same N,W,E,S bit order is the `param_5` word given to the terrain/other layers. Visible = OR of `Body.FOWStatus | V3 | Visibility | (Body+0x94)` has bit civ_id; explored = `Body.Fog_Of_War` bit civ_id; `civ <= 0` / debug mode sees everything; hot-seat sharing is added by C3X.
Pixel semantics [D]: palette is a grey ramp (`R = idx*17`); the sprite is **black with alpha = 255 - R**: cell 0 (all unexplored) is opaque black, cell 80 (all visible) is fully transparent (only 254/255), cell 40 (all fogged) is idx 9 -> alpha 102 (~40 %). Palette entries 254/255 are transparent. Drawn at the vertex cell `(px,py)` over everything drawn so far (terrain, objects, cities) but *before* units are drawn [X for order].

### 6.9 Territory borders (m17 -> m55) [X][D]

`Territory.pcx` (256x288): 2 columns x 4 rows of **128x72**, `Territory_Images[row*2 + col]` (+0x1859C). Drawn for even cells whose `Territory_OwnerID != 0`: for each diagonal neighbour n (0 NW, 1 NE, 2 SW, 3 SE) whose owner differs, draw sprite `n*2 + hilly` at `(px, py - 8)` (zoomed-out `py - 4`), where `hilly = 1` only if **both** this tile and the neighbour are hills/mountain/volcano **and** the same edge hash as 6.1 passes (the %5 test for NW/SE, the %7/%5/%11 tests for NE/SW). The sprite is recoloured with the owner's colour: `leaders[owner].Color_Table_ID` selects an entry of the colour-table array at 0x73DA04 (built from `Art/Units/Palettes/ntpNN.pcx`) and the sprite is drawn with `Sprite_draw_with_color_table` (0x5F87E0). C7 treats palette index 1 as the tinted index and 254/255 as transparent in this file [S]; which indices of `Territory.pcx` are remapped by the engine I did not verify [?].

---

## 7. Landmark (LM) terrain [X partly]

`Terrain_Overlays` bit 29. For such tiles: base terrain uses the `l*` sheets (`SquareParts` still indexes them), hills/mountains/forests use `LMHills/LMMountains/LMForests`, and the `m58` layer draws an additional `landmark_terrain.pcx` sprite: 290x448, 7 cells of 128x64 stacked vertically (`LM_Terrain[7]`, +0xC204); the cell is chosen from the tile's effective terrain via `Tile_Type.LM_*` data [?]. `Tile_Type` in the BIQ has `LM_Name`, `LM_FoodBase` etc. [H].

---

## 8. Cities [X]

- Sheets: `Art/Cities/r{AMER,EURO,ROMAN,MIDEAST,ASIAN}.PCX` (501x380) and `{...}WALL.PCX` (167x380). **Culture group index 0 AMER, 1 EURO, 2 ROMAN, 3 MIDEAST, 4 ASIAN** (loader switch order), = `Races[leader.RaceID].CultureGroupID`. `r*.PCX`: 3 columns (size class 0/1/2) x 4 rows (era 0..3) of **167 x 95**; `*WALL.PCX`: 1 column x 4 rows of 167x95 (size class 3).
- Sprite array at 0xB3CE60 (stride 0x2C): `index = size_class*20 + era*5 + culture_group`, 80 entries.
- `era = leaders[city.CivID].Era` (0..3). `size_class`: `pop > General.MaximumSize_City` -> 2; else `pop > General.MaximumSize_Town` -> 1; else 0, where `pop = Population.Size` (body+0x11C). **The capital** (`city.ID == leaders[civ].CapitalID`) is drawn one size class larger (`+1`, max 2) (C3X comments confirm). Size class 3 (the wall sheet) replaces class 0 for walled towns; exact rule under "Walled towns" below [X].
- Position: anchor = tile centre `(px + 64, py + 32)` (zoomed-out `(px+32, py+16)`); top-left = `anchor - (sprite.Width >> z, sprite.Height >> z)` with `z = 1` normal, `2` zoomed-out, giving `(px + 64 - 83, py + 32 - 47) = (px - 19, py - 15)` at normal zoom (the sprite is centred on the tile) [X from `City_draw_on_map` 0x4BFC80 and caller 0x4C37A0; the `>> z` with z in {1,2} is the scale handling].
- City ruins: `DESTROY.PCX` (Conquests, 501x95: 3 cells of 167x95), randomly picked (`rand_int`, seed `Seed + 0x20C7D*a`) in the tile-building layer [X].
- City name banner, size number, production icon, flags (`City_draw_hud_icon`, `m25` helper 0x4E5640) are UI-level drawing done after the unit pass; not specified here [?].
- Colony: `Tile_has_colony` draws `Terrain_Buldings_Camp[era?]`; unverified era choice [?].

**Walled towns [X].** The rule below uses City_Body-relative offsets (`b = City+0x1C`, which is what `CityItem.City` points at). The BIC is a static object at PCG 0x9C34C8 (C3X's csv lists it as `p_bic_data`), so `General.MaximumSize_Town` = 0x9C72A4 (BIC+0x3DDC) and `General.MaximumSize_City` = 0x9C72A8 (BIC+0x3DE0); `ImprovementsCount` = 0x9C3D40 (BIC+0x878); `Improvements` (pointer, stride 0x110) = 0x9C406C (BIC+0xBA4); `leaders` = 0xA52E58 (stride 0x20E4).
```
size = b.Population.Size (b+0x11C)
L    = leaders[b.CivID (b+0xC, byte)]
cls  = size > MaxCity ? 2 : size > MaxTown ? 1 : 0      // 0x4BFC85..0x4BFCA8 (inline copy of 0x4275C0)
if b.ID (b+0x4) == L.CapitalID (L+0x2C):                 // 0x4BFCC1
    cls = min(cls + 1, 2)                                // capital bump happens first, so a capital never gets walls
if cls == 0 and town_class(city) <= 0                    // 0x4275C0: the same 0/1/2 test without the capital bump; always 0 here
            and walls_value(city) > 0:                   // 0x4C0D00
    cls = 3                                              // 0x4BFCFE
walls_value(city):                                       // 0x4C0D00
    if size > MaxCity or size > MaxTown: return 0
    best = 0
    for i in 0..ImprovementsCount:
        imp = Improvements[i]
        if not City_has_improvement(city, i, include_auto=1): continue   // 0x4ACBE0
        if imp.ObsoleteID (+0xE0) >= 0 and Leader_has_tech(L, imp.ObsoleteID): continue  // 0x5613F0
        best = max(best, (Leader_count_wonders_with_flag(L, ITW_Doubles_City_Defenses=0x1000, NULL) + 1)
                         * imp.Combat_Bombard (+0x9C))   // 0x55A880
    return best
City_has_improvement(city, i, include_auto):             // 0x4ACBE0
    if i == -1: return true
    if imp.GovernmentID (+0xD4) != -1 and != L.GovernmentType (L+0xA0): return false
    if include_auto and L.Auto_Improvements (L+0x20D0, hash table) is non-empty:
        if it contains key i: return true                // civ-wide free building
        c = Map_get_tile((b.X>>1) + b.Y*(Map.Width>>1))->m46_Get_ContinentID()   // b+0x8/+0xA shorts; Map.Width = Map+0x168
        if it contains key (c+1)*ImprovementsCount + i: return true              // continent-wide free building
    return bit i of b.Improvements_1.Improvements (b+0x274, byte i>>3, bit i&7)  // City_Improvements_contains 0x5DF800
```
The multiplier `(count+1)` is always >= 1, so the test reduces to this: the city is not the capital, `size <= MaxTown` (and `<= MaxCity`), and it has, directly or as a civ- or continent-wide auto improvement, a building whose government requirement is met, that is not made obsolete by a tech the owner knows, and that has `Combat_Bombard` (+0x9C) > 0. In the BIQ BLDG record this field is the first int after Culture (BIQ name "bombard defence"; the C3X header calls it `Combat_Bombard`). In `conquests.biq` only Walls (id 7, value 8, ObsoleteID -1, GovernmentID -1) has a nonzero value. The Great Wall (id 36) has `Gain_Building_Continent = 7`, so towns on the owner's Great Wall continent also show walls through the continent key in `Auto_Improvements`. This doc does not check whether that auto entry is removed when the wonder goes obsolete (ObsoleteID 38). Other C3X details: `City_draw_on_map` starts with a hook (`patch_City_draw_on_map`, 0xCFDF75). When `ai_multi_city_start > 1`, it temporarily sets `CapitalID` so that an AI city with an extra palace gets the capital bump and so never shows walls. `City_Improvements_contains` is hooked only for ids >= 256 (`remove_city_improvement_limit`). With `optimize_improvement_loops`, C3X's own `City_get_building_defense_bonus` patch uses the same "Combat_Bombard > 0 counts only at town size" idea.

---

## 9. Per-tile deterministic randomness [X]

Every "random" art choice is deterministic: a 32-bit LCG object is seeded from the tile index and `Map.Seed`:

```
a = ((Width/2)*y + (x>>1)) & 0xFFFF            // y,x of the tile (for rivers: (vx, vy-1))
state = Seed + a*K         with K = 1313 (forest/jungle/swamp), 121472 (hills/mountains/volcano),
                                 0x845FED (rivers), 64536 (tnt), 21030 (pollution/craters),
                                 0x20C7D (buildings/ruins/goody huts)
advance(): state = state*0x41C64E6D + 0x3039  (u32 wrap);  r = (state >> 16) & 0x7FFF
next_float() = r / 32768.0
rand_int(n)  = floor(next_float() * n) = (r*n) >> 15        // consumes one advance
every consumer does 4 plain advance() calls first, then its rand_int(...) calls
```
(`0x60B9B0` = advance+float, `0x60B9E0` = rand_int; constant 3.0517578125e-05 at 0x6716C8.) This is the ANSI-C `rand()` constants; reproduces the in-game forest/mountain/river variants exactly. `Seed` is `Map.Seed` (0x1EC), identical to the BIQ/SAV WRLD seed.

---

## 10. Units [X for the pipeline; most details are [I]/[S]/[?]]

- One unit per tile is shown: `Main_Screen_Form_find_visible_unit(x, y, ...)` (0x4E3F50) picks the displayed unit for each tile of the visible rectangle (list built by 0x4EF4D0, capped at 0x3FF, stored at MSF+8, count at MSF+0x1008); drawn after tiles+fog.
- Animation files: `Art/Units/<UnitTypeName>/<Name>.INI` (C3X header `UnitType.Name`; Conquests units under `Conquests/Art/Units/<name>/`). The INI sections: `[Speed]` (`Normal Speed=`, `Fast Speed=`), `[Animations]` (BLANK, DEFAULT, WALK, RUN, ATTACK1-3, DEFEND, DEATH, DEAD, FORTIFY, FORTIFYHOLD, FIDGET, VICTORY, TURNLEFT, TURNRIGHT, BUILD, ROAD, MINE, IRRIGATE, FORTRESS, CAPTURE, STOP_AT_LAST_FRAME, PauseROAD/MINE/IRRIGATE), `[Timing]` (a float per animation, 0.5 in Archer), `[Sound Effects]` [D: `Archer.INI`].
- **FLC format (Civ3 variant)** [D]: header `size u32, magic 0xAF12 @4, frames @6, width @8, height @10, depth 8, speed @16 (u32, ms/frame)`, first-frame chunk offset @80, **@0x60 u16 = 8 (directions)**, **@0x62 u16 = frames per direction**, @100 u16 OffsetLeft, @102 OffsetTop, @104/@106 original canvas 240x240, @108 animation time (ms). E.g. `ArcherDefault.flc`: 46x50 per frame, 8 dirs x 15 frames (header `frames = 120`), speed 125 ms, offset (96, 81) inside a 240x240 canvas; the file holds **8 x (15 + 1 ring frame) = 128 frame chunks** (`0xF1FA`): each direction's frames followed by a ring (loop-back) frame that must be skipped [D][S C7]. Chunk types seen in `ArcherDefault.flc`: 4 (COLOR_256 palette, in the first frame), 15 (BYTE_RUN keyframe; positive count = repeat byte, negative = literal copy), 7 (DELTA_FLC / SS2 word-based deltas); the same three are what C7's `ReadFlic.cs` handles. My decoder (C7's logic) walked all 128 chunks of that file without error [D]; correctness of every pixel and behaviour for other units (e.g. other chunk types, per-direction keyframes) was not checked [?].
- **Direction order inside the file** (animation index): 0 SW, 1 S, 2 SE, 3 E, 4 NE, 5 N, 6 NW, 7 W [S: C7 `AnimationManager.flicRowToAnimationDirection`]. Unit sprites are placed relative to the tile centre: `top_left = tile_centre + animation_move_offset + (OffsetLeft, OffsetTop) - (OriginalWidth/2, OriginalHeight/2)` (frame is `width x height` cropped from the 240x240 canvas) [S: C7 `GetFramePosition`; consistent with the FLC header] [I].
- Frame timing: `speed` ms per frame from the FLC header (125 ms for Archer, 15 frames = `animation time` 1875 ms); INI `[Timing]` value scales it (0.5) and `[Speed]` gives the normal/fast animation setting [I/?].
- **Palette handling** [D][S]: the FLC carries its own 256-colour palette (entries 0..~99 meaningful, 100..254 are placeholder green (0,255,0), 255 magenta). **Civ colour**: indices **0..15 and the even indices 16..62** (and, from palette spread analysis, 64..69) are *replaced* by the same index of `Art/Units/Palettes/ntpNN.pcx` where **NN = `leaders[civ].Color_Table_ID`** (0..31; `ntp00` is white/neutral). In the 32 `ntp` files, entries 0..69 differ between civs (spread 142-171 on the tinted ones, only ~4 on the odd 17..63 which stay civ-independent) and entries 70..255 are magenta placeholders [D]. C7 uses `tinted = idx < 16 || (idx < 64 && idx % 2 == 0)` [S]; the 64..69 range is my addition from the palette analysis [I]. The `ntp*.pcx` files are 1x1 images used only for their palette. (`otp00-16.pcx` are an older 17-colour set.)
- **Shadows** [D][S]: FLC index 255 = transparent, indices 251-254 form a soft elliptical ground shadow (254 outermost, 253, 252, 251 darkest in the Archer sprite). C7 renders indices 240-254 as black with alpha `(255 - idx)*16/255` (254 = 6 %, 253 = 12.5 %, 252 = 19 %, 251 = 25 %); the engine's own blend (JGL.dll) I could not read [I]. C7 uses 224-239 as a second shadow ramp in its tint code; I saw no use of it in Archer [?].
- Health bar, fortify frame, movement LED, stack indicators, active-unit blink (`Main_Screen_Form.Current_Unit`, timers in the form), unit flag colours: rendered by UI code I did not trace. C7's reconstruction (for reference, not verified against the original): bar 2 px wide at tile-centre `-26 px` in x, one rectangle per HP with 1 px gaps, colour by HP fraction, white outline when fortified, movement LED from `Art/interface/MovementLED.pcx` (6x6 cells, index 0 full, 2 partial, 4 none) [S].

---

## 11. PCX / colour conventions [X][D]

- All art is 8-bit indexed PCX v5 (RLE, 256-colour palette at the end after 0x0C). All terrain/object sheets end their palette with **index 254 = (0,255,0) green (or (200,0,200) in some sheets) and 255 = (255,0,255) magenta** [D: palette tails of xpgc, wCSO, roads, Mountains, FogOfWar, resources].
- **255 is the transparent colour** in every sprite sheet. **254 is not drawn either**: in the terrain/overlay sheets it is the filler outside the sprite outline or the sheet gutter (e.g. `resources.pcx` gutters at x = 0,50,...; `roads.pcx` cell 0 has 4076 pixels of 254 inside the cell); C7 treats {254, 255} as transparent for every sheet [S], and the engine slices to exact rectangles so the gutter is never drawn [X for slicing]. For maximum fidelity treat both as transparent.
- Fog sheet: grey ramp palette used as alpha (section 6.8). Shadow sheets (`resources_shadows.pcx`, `*_shadow.PCX`): used indices 248-251 have palette (0,255,0), (100,255,100), (175,255,175), (248,255,248) and 254 is the (200,0,200)/(200,0,255) grid colour: these are the shadow strengths (index -> opacity ramp), exact mapping not verified [?].
- Unit palettes: see section 10. Civ colours: 32 palettes `Art/Units/Palettes/ntp00..31.pcx`; the colour of a leader is `Leader.Color_Table_ID` (0x10F8); race defaults in `Race.DefaultColor/AlternateColor`.
- Sprite files may be absent from the mod folder: resolution (BIC_get_asset_path 0x5982A0): the relative path (`Art\Terrain\xtgc.pcx`) is tried under the scenario folder (`BIC+0x128`), then under every `;`-separated root stored at 0x9C4144; in this install effectively `Conquests/Conquests/<scenario>/...` -> `Conquests/` -> `civ3PTW/` -> install root [X partially; roots order I]. E.g. `x_airfields and detect.PCX` exists only in `civ3PTW/Art/Terrain` and scenario folders; `TerrainBuildings.PCX`, `marsh.pcx`, `Volcanos*.pcx`, `craters.pcx`, `LM*.pcx`, `l?*.pcx` come from `Conquests/Art/Terrain`.

---

## 12. Sprite sheet catalogue (grid as sliced by the loader) [X]

| File (Art\Terrain unless noted) | PCX size | Grid x cell | Images | Map_Renderer array (+offset) |
|---|---|---|---|---|
| xtgc,xpgc,xdgc,xdpc,xdgp,xggc,wCSO,wSSS,wOOO | 1152x576 | 9x9 of 128x64 | 81 x 9 | `Std_Terrain_Images` +0x102D0 |
| lxtgc ... lwOOO (Conquests) | 1152x576 | same | 81 x 9 | `LM_Terrain_Images` +0x191A4 |
| FogOfWar | 1152x576 | 9x9 of 128x64 | 81 | `Fog_Of_War_Images` +0x41F8 |
| polarICEcaps-final | 1024x256 | 8x4 of 128x64 | 32 | +0x4FE4 |
| roads | 2048x1024 | 16x16 of 128x64 | 256 | +0x8424 |
| railroads | 2048x1088 | 16x17 of 128x64 | 272 | +0x5564 |
| floodplains | 512x256 | 4x4 of 128x64 | 16 | +0x3F38 |
| deltaRivers / mtnRivers | 512x256 each | 4x4 of 128x64 | 16+16 interleaved | +0x1801C (idx = mask*2+variant) |
| waterfalls | 512x64 | 4x1 of 128x64 | 4 | +0xC154 |
| irrigation (x4) | 512x256 | 4x4 of 128x64 | 4x16 | +0xE120 (desert, plains, grass, tundra) |
| xhills, hill forests, hill jungle, LMHills | 512x288 | 4x4 of 128x72 | 16 each | +0xFA90 (+LM +0x18EE4) |
| Mountains, mountain forests, mountain jungles, Mountains-snow, LMMountains | 512x352 | 4x4 of 128x88 | 16 each | +0xEF90 (+LM +0x186FC) |
| Volcanos, Volcanos forests, Volcanos jungles, Volcanos-snow | 512x352 | 4x4 of 128x88 | 16 each | +0xC6A8 |
| grassland/plains/tundra forests | 1000x884 | blocks of 128x88 | jungle L8,S12; forest L8,S10; pines 12 | +0xD1A8.. (Grassland jungles +0xEC20/+0xED80) |
| marsh | 1000x884 | 128x88 | large 8, small 10 | +0xC338 / +0xC498 |
| LMForests | 1000x884 | 128x88 | large 8, small, pines 12 | +0x189BC.. |
| Territory | 256x288 | 2x4 of 128x72 | 8 | +0x1859C |
| goodyhuts | 384x192 | 3x3 of 128x64 (8 used) | 8 | +0xB2E4 |
| tnt | 384x384 | 3x6 of 128x64 | 18 | +0xBE3C |
| TerrainBuildings.PCX | 512x256 (Conquests) | 4x4 of 128x64 | see 6.7 | various |
| x_airfields and detect (+shadow) | (civ3PTW) | 128x64 / 128x128 | airfield 2, outpost 3, radar 1 | +0xB024 / +0xB444 / +0xBDE4 |
| pollution, craters | 640x320 | 5x5 of 128x64 | 25 | +0xB54C / +0xB998 |
| landmark_terrain | 290x448 | 1x7 of 128x64 | 7 | +0xC204 |
| x_victory (Conquests) | 128x64 | 1 | 1 | +0x3F0C |
| `Art/resources.pcx`, `resources_shadows.pcx` | 300x300 | 6x6 of 49x49 at (1+50c, 1+50r) | up to 36 | `Resources` +0x3EAC / +0x3EB0 (pointers to `Sprite[]`) |
| `Art/Cities/r*.PCX`, `*WALL.PCX` | 501x380 / 167x380 | 3x4 / 1x4 of 167x95 | 80 total | static array 0xB3CE60 |

---

## 13. Reference pseudocode (pass 3 cell, normal zoom)

```
for (cy = TileY_Min ..= TileY_Max) for (cx = TileX_Min ..= TileX_Max) {           // row-major
    (wx, wy) = wrap(cx, cy);  skip if outside map
    px = (cx - TileX_Min - 1)*64 - camera_x % 64;  py = (cy - TileY_Min - 1)*32 - camera_y % 32;
    if ((wx + wy) odd)  { /* vertex: pass 1,2,4 only: terrain(vertex), flood plain, river */ }
    else /* tile */     {
       mountain/volcano/hills at (px, py-24 | py-8)   // 6.1
       irrigation (px,py);   tnt (px,py);   forest/jungle/swamp (px, py-12);   LM overlay
       territory borders (px, py-8);   roads then railroads (px,py);
       ruins, airfield, mine, barricade, fortress, goody hut, barb camp, colony, radar, outpost (px,py);
       pollution/craters (px,py);   resource shadow + icon (px+40, py+8);   city (px-19, py-15)
    }
}
// then fog pass over the same rectangle (vertex sprites by 4-corner state), then units, labels.
```

---

## 14. Open questions (explicit)

1. **Units**: exact screen anchor, per-frame offsets, health-bar/flag/LED rendering, stack indicator, blink timing of the selected unit, the `[Speed]/[Timing]` INI semantics; the draw is in the 0x4EFC30 unit-animation path and was not decoded.
2. **Shadow/transparency alpha**: JGL.dll (the blitter) was not analysed; the exact blend for indices 240-254 (units), 248-251 (shadow sheets) and the use of 254 in terrain sheets are inferred from palettes and C7.
3. Initial values of the viewport rectangle globals (0x9CB3FC..0x9CB408, UI-layout dependent), the exact camera clamping rule for non-wrapping maps and `Map.Flags` bit 2.
4. Waterfall sprite selection, polar ice-cap pixel placement, railroad sprite indices 256-271, pollution neighbour bit order, `tnt` file-row/terrain mapping for grass vs bonus grass, goody-hut index selection, fortress/camp era rows, airfield/radar/outpost placement offsets.
5. City: the "walled" size class 3 trigger, ruins/colony details, city name banner placement and size-number font.
6. LM terrain: how the 7 `landmark_terrain` cells map to terrain types.
7. Zoomed-out filtering: halving is certain (steps and offsets), the resampling kernel is not.
8. The meaning of `Terrain_Overlays` bits 17 (set on 1480 of 3200 tiles in `MK 10.SAV`), 19, 24, 27, 28 beyond what is listed.
9. BIC_get_asset_path root list at 0x9C4144 (content built at runtime).

## 15. How the findings were reproduced (so they can be rechecked)

- PCX sizes: `perl` PCX header read (xmax/ymax). Sprite-sheet grids: the loop bounds in the renderer initialiser (0x4C5A30), listed in section 12.
- Struct offsets: `gcc -m32 -fsyntax-only`-style `offsetof` dump of `Civ3Conquests.h` with a stub `windows.h`.
- SAV/BIQ: PKWare DCL decompression (port of zlib `blast.c`; C7 uses the same, `Blast` project), then TILE chunks (4 per tile).
- Image-index formula validation: 27,780 interior tiles from `Rom D 4.SAV`, `MK 10.SAV` and 12 saves in `Conquests/Saves`: `loc = d(N) + 3 d(NW) + 9 d(NE) + 27 d(T)` matched for sheets 0-4 and 6 with zero mismatches.
- River bit mirroring: 982/982 mirrored pairs in `MK 10.SAV`.

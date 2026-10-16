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
- **Zoomed-out resampling, verified [X]** (replaces the [I] above). `Sprite_draw_on_map` (0x5F8060) receives `(canvas, x, y, num_x = 1, num_y = 1, den = zoomed ? 2 : 1, color_table = 0)`. It saves the JGL global scale (JGL object at `[0xCAB28C]`, vtable +0xA0 get, +0x9C set; jgl.dll stores it in its globals 0x1006C0FC/0x1006C100/0x1006C104 = num_x/num_y/den, set by 0x1003C0E0), sets `num/den`, adds the sprite's frame offset scaled (`x += Sprite+0x20 * num_x / den`, `y += Sprite+0x24 * num_y / den`, C division; 0 for sheet cells), blits through the image's vtable +0x44, and restores the old scale. In jgl.dll (`Conquests/jgl.dll`, 2004-03-17) the blit dispatcher (around 0x10008200) picks a routine by source/destination depth; the 8-bit indexed source to 16-bit canvas routine is 0x1000ADA0 (variant 0x10034400 when image flag `+0x18 & 1` is set; same scheme). The scaled path is **nearest neighbour, no filtering**:
  - destination size = `ftol(W * num/den)`, `ftol(H * num/den)` with `ftol` truncating, i.e. **`floor(W/2) x floor(H/2)`** (128x64 -> 64x32, 128x88 -> 64x44, 128x72 -> 64x36, 49x49 -> 24x24, 167x95 -> 83x47);
  - source step per destination pixel = `(den << 16) / num` in 16.16 fixed point = exactly 2.0 in both axes; the source start is `(clip_dx * step, clip_dy * step)` where `clip_d*` is how far the canvas clip moved the destination start (0 when the sprite is not clipped);
  - so **destination pixel (i, j) = source pixel (2i, 2j)**: every second column and row starting with the cell's (0,0); a sprite clipped at the left/top canvas edge still samples even source coordinates;
  - source indices >= 254 (`cmp byte [esi], 0xFE; jae skip`) are not written (both 254 and 255 transparent); others go through the 16-bit colour table.
  The shadow/blend blit 0x5F8560 (airfield/radar/outpost shadows) takes the same scale arguments.
- **Zoomed-out offsets [X]** (normal -> zoomed out): lattice step 64x32 -> 32x16; terrain, rivers, flood plains, fog, irrigation, tnt, roads/rails, borders' x, all `TerrainBuildings` cells, goody huts, pollution/craters, victory, LM marker: `(px, py)` both zooms; polar caps `(px-64, py+-32)` -> `(px-32, py+-16)`; mountains/volcanoes `py-24` -> `py-12`; hills `py-8` -> `py-4`; borders `py-8` -> `py-4`; **forests/jungles/marsh `py-12` in both zooms** (m8 passes a constant `-0xC`, 0x5F650F/0x5F654B/0x5F6589/0x5F65C9/0x5F6640; the callees do not correct it), so zoomed-out forests sit 6 px higher than a proportional scale would put them; radar and outposts `py-64` -> `py-32` (`64 / den`); resources `(px+40, py+8)` -> `(px+20, py+4)`; cities and ruins `px + halfW - W/(2*den)`, `py + halfH - H/(2*den)` with halfW/halfH = 64/32 -> 32/16: `(px-19, py-15)` -> `(px-9, py-7)` for 167x95 cells.
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
- **Waterfalls are never drawn [X].** The only reader of `Waterfalls_Images` (+0xC154) besides the constructor/destructor/loader is Map_Renderer vtable slot 67 (+0x10C, 0x4C3D60: draws `Waterfalls_Images[arg1]` at `(px, py)`). Its address occurs once in the exe (the vtable at 0x66A614) and no `call 0x4C3D60` exists; the four `call [reg+0x10C]` sites in the exe (0x5D2258, 0x5D9317, 0x5E9EF4, 0x618778) are on other objects (Tile and others) and pass one argument, while the waterfall method takes four (`ret 0x10`). A reimplementation draws no waterfalls.

### 6.5 Overlay bits (`Tile.Overlays`, per-civ view via `Visibile_Overlays`) [X][D]

bit 0 road, 1 railroad, 2 mine, 3 irrigation, 4 fortress, 5 goody hut, 6 pollution, 7 barbarian camp, 8 craters, 28 barricade, 29 airfield, 30 radar tower, 31 outpost. (Matches SAV flags2: road bit0 ... byte3 bit4 barricade, bit5 airfield, bit6 radar, bit7 outpost.) `Tile_BuildingID != -1` = colony/other tile building [X].

**Per-civ view, exact [X]** (`Tile::m42_Get_Overlays(civ)`, 0x5DC230; C3X wraps it only for hot-seat shared vision). `civ == 0` -> `Tile.Overlays` (Tile+0x28). Otherwise, if the tile is **currently visible** to civ (`(Tile+0xD0 | +0x64 | +0x60 | +0x5C) & (1 << civ)`, the same test as the fog pass) -> `Tile.Overlays`; else -> the remembered **byte** `Tile+0xAE+civ` (`Visibile_Overlays[civ]`) zero-extended. So on a tile that is explored but not currently visible only bits 0-7 survive (road, rail, mine, irrigation, fortress, goody hut, pollution, barbarian camp); **craters (bit 8) and barricades (bit 28) are not drawn there**, and a barricaded fortress falls back to the plain fortress sprite (bit 4 is in the byte). Airfield, radar and outpost are always tested with `civ = 0` (actual state), so they show through fog. Our `Civ3World::seen_overlays` (`(Overlays & !0xFF) | remembered byte`) differs from this: it keeps bits 8-31 on fogged tiles and uses the remembered byte even on visible tiles.

The `civ` every layer receives is the first argument of `m19`, i.e. the **viewing player** (`Player_CivID`), not the tile owner [X].

Tile vtable (0x6701C8) checks used by the object layers [X]: +0x18 `m06` airfield (bit 29), +0x1C `m07` barbarian camp (bit 7), +0x34 fortress (bit 4), +0x38 barricade (bit 28), +0x3C goody hut (bit 5), +0x48 mine (bit 2), +0x4C outpost (bit 31), +0x50 pollution (bit 6), +0x54 craters (bit 8), +0x5C railroad (bit 1), +0x78 LM (`Terrain_Overlays` bit 29), +0x84 radar (bit 30), +0x88 victory point (`(u16)Tile+0x22 != 0xFFFF`), +0x90 ruins (`Tile+0x24`, dword), +0xB0 `Barbarian_TribeID` (Tile+0x18), +0xB4 `CityID` (Tile+0x1A), +0xBC `Tile_BuildingID` (Tile+0x1C), +0xC8 effective terrain. `Tile_has_city` 0x5EA5F0 = `CityID != -1`; **`Tile_has_colony` 0x5EA610 = `Tile_BuildingID != -1` and not airfield(0), not radar(0), not outpost(0)** (airfields, radar towers and outposts are tile buildings too and also carry their overlay bit).

### 6.6 Roads and railroads (m16 / m14 -> m52 / m50) [X][D]

- `roads.pcx`: 2048x1024, 16x16 cells of 128x64 = 256 images. `railroads.pcx`: 2048x1088, 16x17 = **272** images (row 16 = 16 extra images, index 256..271). Sprite `Roads_Images[i]` (+0x8424), `Railroads_Images[i]` (+0x5564), drawn at `(px,py)` of the tile.
- Bit i of the index = connection to neighbour i+1 of the spiral numbering `neighbor_index_to_diff` (0x5E6D80): 1 NE(+1,-1), 2 E(+2,0), 3 SE(+1,+1), 4 S(0,+2), 5 SW(-1,+1), 6 W(-2,0), 7 NW(-1,-1), 8 N(0,-2) => **bit0 NE, bit1 E, bit2 SE, bit3 S, bit4 SW, bit5 W, bit6 NW, bit7 N** [X]. Pixel check: `roads.pcx` cell 1 has its stroke in the upper-right, 2 horizontal right, 4 lower-right, 8 vertical down, 16 lower-left, 32 horizontal left, 64 upper-left, 128 vertical up [D].
- Road mask: for each of the 8 neighbours inside the map: set the bit if the neighbour has a road (`Check_Roads(civ)`), **except** when both this tile and the neighbour have railroad (then that link is a railroad link). If mask == 0 and the tile has no city and no railroad: draw sprite 0 (the isolated road stub); if a city is present nothing.
- Railroad mask: same with `Check_Railroads`; for tiles whose rail neighbours... the 16 extra images for rails are used for rail-over-road junction cases; the exact rule for indices 256-271 was not decoded [?]. Roads are drawn before railroads in the same cell.
- **Railroad indices 255-271, verified [X]** (`m14` 0x5F6880 -> slot 50 `Railroads_Images[i]`): `mask` = bit k-1 set for each neighbour k = 1..8 (`neighbor_index_to_diff`) inside the map that has a railroad in the viewer's overlays (no road/rail exclusion here). If `mask == 0` and the tile has a city, nothing is drawn. Otherwise, **if `mask == 0xFF`** (rail on all eight neighbours): `index = 255 + rand_int(17)` (255..271: the normal full junction or one of the 16 variants in sheet row 16); else `index = mask`. RNG: `a = ((W/2)*y + (x>>1)) & 0xFFFF`, `state = Seed + a * 15221` (computed as `a + 4*5*(a + 8*(96a - a))`), **three** plain advances (not four; 0x5F69E5/0x5F69F0/0x5F69FB), then the single `rand_int(17)`. The seeding and the three advances run for every drawn rail sprite, but only the all-neighbours case consumes the `rand_int`. Position `(px, py)`.
- Roads/rails respect the *viewing civ's* remembered overlays (civ id argument).

### 6.7 Fortress, mines, camps, airfield, radar, outpost, pollution, craters, ruins, goody huts, tnt, resources

`TerrainBuildings.PCX` (Conquests version 512x256; the PTW base one is 384x256): 4x4 cells of 128x64 [X]: col 0 = fortress, rows 0-3 = eras (`Terrain_Buldings_Fortress[4]`, +0xB184); col 1 = colony/camp rows 0-3 (`Terrain_Buldings_Camp[4]`, +0xB0D4); col 2 row 0 = barbarian camp (+0x3EB4), col 2 row 1 = **mine** (+0x3EE0); col 3 = barricade rows 0-3 (+0xB234). Era row choice is [?] (likely the owner's era). All drawn at `(px,py)`; mine at `(px,py)`.

`x_airfields and detect.PCX` (+ `X_AIRfields and detect_shadow.PCX`; in `civ3PTW/Art/Terrain` and scenario folders): airfield 2 cells 128x64 at y=0 (x 0,128); outposts 3 cells 128x128 at y=64 (x 0,128,256); radar 1 cell 128x128 at (0,192); shadow file has the same layout. [X for slicing; ? for placement offsets].

`pollution.pcx` and `craters.pcx` (640x320): 5x5 = 25 cells of 128x64 each. Index: 4-bit mask of diagonal neighbours that also have pollution/craters; `mask != 0 -> index mask + 9` (10..24), else `rand_int(10)` (0..9); seed `Seed + 21030*a` [X for formula; ? for neighbour bit order, assumed NW,NE,SW,SE].

`goodyhuts.pcx` (384x192): 8 cells of 128x64 in a 3-column grid (cell i at `x = (i%3)*128, y = (i/3)*64`); chosen by `rand_int(...)` [X; seed in `m12`: `Seed + 0x20C7D*a`].

`tnt.pcx` (384x384): 6 rows x 3 cells of 128x64 (small ground decorations: tufts, rocks, birds). Variant `rand_int(3)` (seed `Seed + 64536*a`); row by effective terrain: desert -> file row 2, plains -> file row 1, grassland -> file row 0 (special/bonus grass variant [?]), tundra -> file row 3, flood plain -> file row 4 (file row 5 is used by another branch [?]) [X partially]. Drawn at `(px,py)`.

`StartLoc.pcx` (128x64) is the editor start-location marker (not part of game rendering).

**Resources** (`Art/resources.pcx` + `resources_shadows.pcx`, 300x300): 6x6 grid, each cell **49x49 at (1+50*col, 1+50*row)**; column index lines (x = 0,50,...) are palette index 254 [D]. `icon = BIC.ResourceTypes[Tile.ResourceType].IconID` (BIQ GOOD record: name[24], pedia[32], class, appearance, disappearance, icon, prereq, food, shield, commerce; conquests.biq: Horses 0, Iron 1, Saltpeter 2, Coal 3, Oil 4, Rubber 5, Aluminum 6, Uranium 7, Wines 8, Furs 9, Dyes 10, Incense 11, Spices 12, Ivory 13, Silks 14, Gems 15, Whales 16, Game 17, Fish 18, Cattle 19, Wheat 20, Gold 21, Tropical Fruit 22, Oasis 23, Sugar 24, Tobacco 25) -> `col = icon%6, row = icon/6` [X][D]. Draw position: icon top-left = `(px + 64 - 24, py + 32 - 24) = (px+40, py+8)` (zoomed-out `(px+20, py+4)`; i.e. `49/2 = 24`, `49/4 = 12` offset for scale 1/2) [X]. The shadow sprite (same cell index, `ResourcesShadows`) is drawn first at the same position, then the icon [X: 0x4C4AE0 draws the shadows array first]. Condition: `Tile_get_resource_visible_to(civ) != -1` (player knows the tech that reveals it) unless debug bit 8 [X].

### 6.7b Tile buildings (m12) and pollution/craters (m13), exact rules [X]

`civ` below = the viewer (m19 arg 1). `Era(c)` = `leaders[c].Era` (Leader+0xF4, 0..3). All sprites go through `Sprite_draw_on_map(..., 1, 1, zoomed ? 2 : 1, 0)` (section 3). `a = ((W/2)*y + (x>>1)) & 0xFFFF`.

**m12 `Draw_Tile_Buildings`** (slot 12, 0x5F6A50; flag 0x80 in pass 3, after roads/railroads, before m13). On entry it seeds one RNG `state = Seed + a * 0x20C7D` and does the 4 plain advances **unconditionally**; the `rand_int` calls below draw from this one state in this order. Then, in this fixed order:

| # | Condition | Draw (Map_Renderer slot) | Sprite | Position |
|---|---|---|---|---|
| 1 | `Tile+0x24` (ruins) != 0 | slot 53 (0x4C4A30) | `DESTROY.PCX` cell `rand_int(3)`: static array 0xB3E790 (3 cells of `W/3` x H = 167x95, sliced at x = 0, 167, 334 by 0x549CEE; the file is `art\Cities\destroy.pcx` via `BIC_get_asset_path`, so `Conquests/Art/Cities/DESTROY.PCX`) | centred: `(px + 64/den - W/(2*den), py + 32/den - H/(2*den))` = `(px-19, py-15)`; zoomed `(px-9, py-7)` |
| 2 | airfield bit 29 of `Overlays` (civ 0 = actual) | slot 22 (0x4C4700) | first the shadow `Terrain_Buldings_Airfields_Shadow[i]` (+0xB07C) with the blend blit 0x5F8560 (blend table `[0x73D9F0]`), then `Terrain_Buldings_Airfields[i]` (+0xB024); **`i = (Era(civ) - 1) / 2`** (C division, truncating): 0 for eras 0-2, 1 for era 3 | `(px, py)` |
| 3 | mine bit 2 (viewer overlays) | slot 41 (0x4C44A0) | `Terrain_Buldings_Mines` (+0x3EE0, TerrainBuildings col 2 row 1) | `(px, py)` |
| 4 | barricade bit 28 (viewer) | slot 32 (0x4C45C0) | `Terrain_Buldings_Barricade[Era(civ)]` (+0xB234) | `(px, py)` |
| 4' | else fortress bit 4 (viewer) | slot 31 (0x4C4560) | `Terrain_Buldings_Fortress[Era(civ)]` (+0xB184) | `(px, py)` |
| 5 | goody hut bit 5 (viewer) | slot 34 (0x4C4620) | `Goody_Huts_Images[rand_int(8)]` | `(px, py)` |
| 6 | barbarian camp bit 7 in **both** `Overlays(0)` and `Overlays(civ)` | slot 23 (0x4C4660) | `Terrain_Buldings_Barbarian_Camp` (+0x3EB4, TerrainBuildings col 2 row 0). The caller passes `Barbarian_TribeID`, the drawer ignores it: one sprite for all tribes | `(px, py)` |
| 7 | `Tile_has_colony` (see 6.5; no visibility test) | slot 26 (0x4C46A0) | `Terrain_Buldings_Camp[Era(civ)]` (+0xB0D4, TerrainBuildings col 1, rows = era) | `(px, py)` |
| 8 | radar bit 30 (actual) | slot 57 (0x4C4790) | shadow `Terrain_Buldings_Radar_Shadow` (+0xBE10, blend blit) then `Terrain_Buldings_Radar` (+0xBDE4). The index is computed from the owner's era but reduces to `min(era, 0)` = **always 0** | `(px, py - 64)` (128x128 cell bottom-aligned to the tile box); zoomed `(px, py - 32)` |
| 9 | outpost bit 31 (actual) | slot 46 (0x4C48C0) | shadow `Terrain_Buldings_Outposts_Shadow[i]` (+0xB4C8) then `Terrain_Buldings_Outposts[i]` (+0xB444); **`i = min(Era(owner), 2)`**, owner = `OwnerID` (Body+0x10, byte read at object+0x2C) of entry `Tile_BuildingID` in the outpost list `[0xA52DCC]` (`LastIndex` at 0xA52DD8; item = `{int, body*}`, object = body-0x1C); if the list or entry is missing, owner = viewer. (Radar uses the list `[0xA52DE4]`/0xA52DF0 the same way.) | `(px, py - 64)`; zoomed `(px, py - 32)` |
| 10 | victory point (`(u16)Tile+0x22 != 0xFFFF`) and **no city** | slot 66 (0x4C49F0) | `Victory_Image` (+0x3F0C) | `(px, py)` |

So the ruins `rand_int(3)` (if any) comes before the goody hut's `rand_int(8)`. Victory points on city tiles are drawn later by `m7` (flag 0x2000) right after the city sprite (0x5F7152..0x5F7187: `has_city` -> city via slot 25, then the same victory test without the city condition -> slot 66). Sprite cells (loader 0x4C5A30): `x_airfields and detect.PCX` airfields at (0,0),(128,0) 128x64; outposts at (0,64),(128,64),(256,64) 128x128; radar at (0,192) 128x128; the `_shadow` file has the same layout.

**m13 `Draw_Tile_Pollution`** (slot 13, 0x5F6DA0; flag 0x8000, after m12, before resources). One RNG: `state = Seed + a * 21030`, 4 plain advances on entry (unconditional). Then:
1. **Craters first**: if craters (bit 8, viewer overlays): `mask` over k = 0..3 with neighbour `(x + ((k+1)&2) - 1, y + 2*(k/2) - 1)` = **k0 NW (-1,-1), k1 NE (+1,-1), k2 SE (+1,+1), k3 SW (-1,+1)** (wrapped; off-map skipped), bit k set if that neighbour has craters (viewer overlays). Index `mask + 9` if `mask != 0`, else `rand_int(10)`. Sprite `Craters[index]` (+0xB998, `craters.pcx`) via slot 49 at `(px, py)`.
2. **Then pollution**: same with bit 6, sprite `Pollution[index]` (+0xB54C, `pollution.pcx`) via slot 48 at `(px, py)`. Its `rand_int(10)` continues the same RNG state, so on a tile with both and both masks 0, pollution uses the second draw.

Corrections to section 6.7 above: the neighbour bit order is **NW, NE, SE, SW** (bit 2 = SE, bit 3 = SW), not NW, NE, SW, SE; craters are drawn **before** pollution; both share one RNG stream.

**Order relative to our pass-3 layer list.** Engine order inside one even cell: mountain/hills (0x4010) -> irrigation -> tnt -> forest -> LM marker -> borders -> road -> railroad -> [m12: ruins -> airfield shadow, airfield -> mine -> barricade | fortress -> goody hut -> barbarian camp -> colony -> radar shadow, radar -> outpost shadow, outpost -> victory (non-city tile)] -> [m13: craters -> pollution] -> resource shadow, resource -> city -> victory (city tile).

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

**Verified rules [X]** (LM test = `Tile::m30` 0x5EA8B0 = `Terrain_Overlays` bit 29):
- **Base terrain (m6, 0x5F48C0)**: the image comes from `SquareParts` of tile `(vx, vy+1)` as before, but the `isLM` flag passed to `m24` (slot 24, 0x4C38A0) comes from `Map.m12_Get_Tile_by_XY(vx, vy)` = `tile_at(vx, vy)` (0x5DC0F0, plain `Tiles[(W/2)*vy + (vx>>1)]`, no parity check, `vy` not wrapped) **for the odd vertex cell itself**. That index is the tile at `(2*(vx>>1) + (vy&1), vy)`: the **W neighbour `(vx-1, vy)` when `vy` is even, the E neighbour `(vx+1, vy)` when `vy` is odd**. If that tile has bit 29, the vertex is drawn from `LM_Terrain_Images[file][loc]` (`lxtgc ... lwOOO`) with the same `file/loc`. Polar caps are unaffected.
- **Mountains (m10 -> slot 42, 0x4C3A40)**: if the tile is LM, draw `LM_Mountains_Images[mask]` (+0x186FC; `mask` as in 6.1, the vegetation/snow `kind` is ignored) at `(px, py-24)` (zoomed `-12`). **Hills (slot 37, 0x4C3AD0)**: `LM_Hills_Images[mask]` (+0x18EE4) at `(px, py-8)` (`-4`). **Volcanoes** (slot 45, 0x4C4330) take no LM flag: normal volcano art. The RNG for the `kind` tie-break is still consumed exactly as for a non-LM tile.
- **Forests (m8 -> slots 28/29/30, 0x4C40E0/0x4C4170/0x4C4200)**: same seed, `small`/`pines` decision and `rand_int` as 6.2; LM tiles use `LM_Forests_Large[r]` (+0x189BC), `LM_Forests_Pines[r]`, `LM_Forests_Small[r]` (`LMForests.pcx`) instead of the grassland/plains/tundra block, at `(px, py-12)`. **Jungle and marsh** have no LM variant (slots 39/40/43/44 take no flag).
- **LM marker (slot 58, 0x4C3DA0; m19 flag 0x60, after forests, before borders)**: called with the tile's **effective** terrain (`m50`, +0xC8) when the tile is LM. Cell of `landmark_terrain.pcx` (the sheet is a debug key: a magenta (transparent) diamond with a small yellow diamond in the centre and a text label "Forest/Hills/Mountains/Desert/Plains/Grassland/Sea" outside the 128x64 cell): desert 0 -> 3, plains 1 -> 4, grassland 2 -> 5, hills 5 -> 1, mountains 6 -> 2, forest 7 -> 0, jungle 8 -> 0, sea 12 -> 6; tundra, flood plain, swamp, volcano, coast, ocean -> nothing. **Drawn only if `p_preferences` (0xA52638) & 0x1000** (C3X header name `P_SHOW_YIELDS_ON_MAP`); otherwise nothing. Position `(px, py)`; the visible part is just the small yellow diamond at the tile centre.
- Irrigation, tnt, rivers, roads, buildings: no LM handling.

---

## 8. Cities [X]

- Sheets: `Art/Cities/r{AMER,EURO,ROMAN,MIDEAST,ASIAN}.PCX` (501x380) and `{...}WALL.PCX` (167x380). **Culture group index 0 AMER, 1 EURO, 2 ROMAN, 3 MIDEAST, 4 ASIAN** (loader switch order), = `Races[leader.RaceID].CultureGroupID`. `r*.PCX`: 3 columns (size class 0/1/2) x 4 rows (era 0..3) of **167 x 95**; `*WALL.PCX`: 1 column x 4 rows of 167x95 (size class 3).
- Sprite array at 0xB3CE60 (stride 0x2C): `index = size_class*20 + era*5 + culture_group`, 80 entries.
- `era = leaders[city.CivID].Era` (0..3). `size_class`: `pop > General.MaximumSize_City` -> 2; else `pop > General.MaximumSize_Town` -> 1; else 0, where `pop = Population.Size` (body+0x11C). **The capital** (`city.ID == leaders[civ].CapitalID`) is drawn one size class larger (`+1`, max 2) (C3X comments confirm). Size class 3 (the wall sheet) replaces class 0 for walled towns; exact rule under "Walled towns" below [X].
- Position: anchor = tile centre `(px + 64, py + 32)` (zoomed-out `(px+32, py+16)`); top-left = `anchor - (sprite.Width >> z, sprite.Height >> z)` with `z = 1` normal, `2` zoomed-out, giving `(px + 64 - 83, py + 32 - 47) = (px - 19, py - 15)` at normal zoom (the sprite is centred on the tile) [X from `City_draw_on_map` 0x4BFC80 and caller 0x4C37A0; the `>> z` with z in {1,2} is the scale handling].
- City ruins: `DESTROY.PCX` (Conquests, 501x95: 3 cells of 167x95), randomly picked (`rand_int`, seed `Seed + 0x20C7D*a`) in the tile-building layer [X].
- City name banner, size number, production icon, flags (`City_draw_hud_icon`, `m25` helper 0x4E5640) are UI-level drawing done after the unit pass; not specified here [?].
- Colony: `Tile_has_colony` draws `Terrain_Buldings_Camp[era?]`; unverified era choice [?]. **Resolved [X]:** `Terrain_Buldings_Camp[leaders[viewer].Era]`, see 6.7b row 7. Ruins: 6.7b row 1 (any `Tile+0x24 != 0`, cell `rand_int(3)` from the shared building RNG, centred like a city). Note that fortress, barricade, colony and airfield art all follow the **viewer's** era, not the owner's.

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

## 10. Units

Tags as in section 0. **[D]** here means checked against the WW2-Pacific trace `ww2j.txt` and its screenshot `ww2j-game.png`: 31 unit draws, the art of each unit identified by brute force over every `*default*.flc` in the install, 8 directions x all frames x the 5 civ palettes in use. All 31 matched pixel-exactly (Infantry 20, Guerilla 9, Japanese Rifleman 1, Fighter 1). Struct offsets come from `Civ3Conquests.h` compiled with `i686-w64-mingw32-gcc` (as in `tools/gen-civ3-layout`). Addresses are PCG. This section replaces the unit parts of section 4 item 3 and section 14 item 1. In particular, the draw list lives in the **Animator** (`Main_Screen_Form.animator`, MSF+0x2E1B8 = **0xA26878**), not at MSF+8.

### 10.1 Unit data [H][X]

`Unit` = `{vtable, field_4, "UNIT", field_C[4], Unit_Body Body}`. `Body` is at **Unit+0x1C**. `sizeof(Unit)` = 0x404. Below, "B+" is the offset in `Unit_Body` and "U+" the offset in `Unit`. The renderer code works with `Unit*` (U+).

| Field | B+ | U+ | Notes |
|---|---|---|---|
| `ID` | 0x04 | 0x20 | = index in the `Units` list; the draw-list builder appends always-on-top units only if `ID >= 0` [X 0x4EF7DB] |
| `X`, `Y` | 0x08, 0x0C | 0x24, 0x28 | tile coordinates [X] |
| `CivID` | 0x18 | 0x34 | owner. Colour = `leaders[CivID].Color_Table_ID` (Leader+0x10F8) [X] |
| `UnitTypeID` | 0x24 | 0x40 | `BIC.UnitTypes` (0x9C71A0 = BIC+0x3CD8), stride 0x138: `Name`+0x08, `Civilopedia_Entry`+0x28, `Bombard_Strength`+0x48, `Unit_Class`+0x9C (0 land, 1 sea, 2 air), `Hit_Point_Bonus`+0xA4 [X] |
| `Combat_Experience` | 0x28 | 0x44 | index into `BIC.CombatExperience` (BIC+0xBC4, stride 0x2C, `Base_Hit_Points`+0x24) [X 0x5BE3A5] |
| `Status` | 0x2C | 0x48 | 0x04 attacked, 0x40 def. bombard, 0x80 sentry, 0x100 sentry-enemy-only. Not read by the drawing code [H] |
| `Damage` | 0x30 | 0x4C | HP lost [X] |
| `Moves` | 0x34 | 0x50 | move points **used**. Remaining = `Unit_get_max_move_points` (0x5BE180) - `Moves` [X 0x5BA391] |
| `Container_Unit` | 0x44 | 0x60 | -1 = not loaded. Loaded units (transport or army) are never the displayed unit [X 0x4E4117] |
| `UnitState` | 0x48 | 0x64 | `UnitStateType`; **1 = fortified/fortifying** (drives the fortify bonus in 10.2 and the white bar frame in 10.6) [X] |
| `automated` | 0x4C | 0x68 | byte [X 0x4E410C] |
| byte at B+0x41 | 0x41 | 0x5D | the high byte of the header's `short Active`. It acts as a "moving / animating" flag: the unit is returned at once by `find_visible_unit`, the stack lines are suppressed, and the unit under it is drawn too (10.2) [X; meaning I] |
| `field_1CC` | 0x1CC | 0x1E8 | byte; if set and `0x5B2BE0(unit)` is true, the unit is skipped [X; meaning ?] |
| `always_on_top` | 0x232 | 0x24E | byte [X] |
| `Animation` (`FLC_Animation`, 0x188 bytes) | 0x260 | **0x27C** | see below |

Fields inside `Unit.Body.Animation` (anim = U+0x27C) [X][H]:

| Field | anim+ | U+ | Meaning |
|---|---|---|---|
| `summary.direction` | 0x08 | 0x284 | queued facing (`enum direction`) |
| `summary.tile_x/y` | 0x10/0x14 | 0x28C/0x290 | |
| **`summary.pixel_loc_x/y`** | 0x18/0x1C | **0x294/0x298** | world pixel of the unit's anchor (normal zoom). The setter 0x403A80 stores `(px+64, py+32)`. Callers pass the tile box top-left `(64x, 32y)` (e.g. 0x46CC6A..0x46CC92), so an idle unit's anchor is the **tile centre `(64x+64, 32y+32)`**. Moving units are interpolated between tiles (0x5BA9F6) [X] |
| **`summary.direction_2`** | 0x20 | **0x29C** | **displayed facing**. Copied from `direction` when the frame advances (0x4033C2) [X] |
| **`summary.current_anim_type`** | 0x24 | **0x2A0** | `AnimationType`: 0 BLANK, 1 DEFAULT, 2 RUN, 3-5 ATTACK1-3, 6 DEATH, 7 FORTIFY, 8 FIDGET, 9 VICTORY, 10 CAPTURE, 11 FORTRESS, 12 BUILD, 13 ROAD, 14 MINE, 15 IRRIGATE, 16 JUNGLE, 17 FOREST, 18 PLANT [H, names X from the table at 0x680758] |
| `Frame_1` (`FLC_Frame_Image`) | 0x58 | 0x2D4 | the decoded current frame |
| **`Frame_1.sprite`** | 0x60 | **0x2DC** | **the `Sprite` that is blitted** (see 10.3) |
| `Animation_Info*` | 0xE4 | 0x360 | per unit type (and era) art; `Animations[type]` (`Flic_Anim_Info*`) at +0x13C, `float *frame_time` at +0x1D8, `int *frame_count` at +0x1E8 [X 0x4058C0..0x4058F0] |
| **frame index** (`field_FC`) | 0xFC | **0x378** | current frame within the direction block [X 0x402631, 0x403386] |
| time accumulator / frame time | 0xEC / 0x100 | 0x368 / 0x37C | [X 0x403330] |
| `field_110` | 0x110 | 0x38C | "stop at last frame" for FORTIFY/DEATH/BUILD [X 0x402821] |
| `field_111` | 0x111 | 0x38D | "plays a combat animation" [X] |
| fidget timer (float, s) | 0x130 | 0x3AC | [X, 10.4] |
| visible flag | 0x184 | 0x400 | the animator skips the draw if 0. Set to 1 by the constructor, cleared when DEATH ends [X 0x4F0BFD, 0x402332, 0x402800] |

**Iterating units** [X]: `Units` object at 0xA52E40: `UnitItem *Units` at +4 (**0xA52E44**), `LastIndex` at +0x10 (**0xA52E50**). `UnitItem = {int field_0; Unit_Body *body}` (8 bytes). **The item points at the `Unit_Body`**. Every lookup does `p = Units[i].body; if (p) unit = p - 0x1C` (e.g. 0x4E401C..0x4E4028, 0x4EF53C..0x4EF544). Valid ids are `0..LastIndex` inclusive, and a NULL item is a free slot. Units on a tile: `Tile` vfunc m40 (+0xA0) gives a key, `TileUnits` (0xA52D94) method 0x426D00 returns the first unit id and an iterator index, and the loop follows the node array at 0xA52D98 (`{next_index, unit_id}`, `LastIndex` at 0xA52DA4; -1 ends it) [X 0x4E3FD3..0x4E42BD; node semantics I]. For an offline renderer it is equivalent to group `Units` by `(X, Y)`, except for the tie order in 10.2.

### 10.2 Which unit a tile shows, and draw order [X][D]

**Draw list.** `Animator::build_unit_list` (0x4EF4D0) clears `Animator.Units_Count` (+0x1008) and fills `Animator.Units[1024]` (+0x8, cap 0x3FF), as follows:

```
for y in TileY_Min ..= TileY_Max:                     // MSF 0x9FD538 / 0x9FD53C
  for x in TileX_Max-1 down to TileX_Min:             // 0x9FD534-1 .. 0x9FD530, DESCENDING
    (wx, wy) = wrap(x, y); skip if off-map or (wx ^ wy) & 1
    u = find_visible_unit(wx, wy, excluded = NULL)    // 0x4E3F50
    skip if !u or u.U+0x259
    if !u.U+0x24F and !u.always_on_top:
        skip unless tile_visible_now(u, Player_CivID, wx, wy)   // 0x4F0DE0
    if u.U+0x5D (moving) or u.always_on_top:          // draw the unit under it as well
        u2 = find_visible_unit(wx, wy, excluded = u)
        if u2 and u2.Container_Unit == -1 and (debug-see-all or 0x4A83E0(...) or leaders[viewer]+0xEB0[u2.CivID] bit 6):
            append u2
    mark u (U+0x24D = 1); if !u.always_on_top: append u
append every always-on-top unit kept in the vector at Animator+0x1900 (if ID >= 0)
```

`tile_visible_now` (0x4F0DE0) is true in debug mode (`p_debug_mode_bits` & 8), or when the tile's `Body+0x20|0x24|0x28|0x94` (Tile+0x5C/0x60/0x64/0xD0) has the viewer's bit, or when `leaders[viewer]+0xEB0+4*unit.CivID` has bit 6 (shared vision) [X]. Units on fogged or unexplored tiles are therefore never drawn.

So the order is **row-major, top row first, and right-to-left inside a row** (x decreasing). **[D]:** the 31 unit draws of the trace follow it with no violation, and each is on a different tile.

**`Main_Screen_Form_find_visible_unit(x, y, excluded)`** (0x4E3F50; in the installed exe its first 5 bytes jump to C3X's `patch_Main_Screen_Form_find_visible_unit`, which returns `is->unit_display_override` if set and otherwise calls the original). It walks the tile's units and:

1. skips `excluded`, units inside an army (`Unit_get_containing_army` 0x5BC7A0 != NULL), and units with `U+0x1E8 && 0x5B2BE0(u)`;
2. **returns at once** a unit with `always_on_top`, `U+0x1DC` (byte) or `U+0x5D` set;
3. skips units the viewer cannot see (`Unit_is_visible_to_civ(Player_CivID, 1)`, 0x5BB360; this is where invisible units drop out [I]), unless in debug-see-all mode or the unit plays ATTACK1/ATTACK2/DEATH;
4. **returns at once** a unit whose `current_anim_type` is in 3..6 or 9 (attack, death, victory);
5. **returns at once** `Main_Screen_Form.Current_Unit` (0x9FD434) if it belongs to the viewer and is not `automated`;
6. skips loaded units (`Container_Unit != -1`);
7. otherwise scores `s = Unit_get_defense_strength(u) * (100 + f) * hp / 100`, with `hp = clamp(Unit_get_max_hp(u) - Damage, 0, 9999)`. `f = [0x9C72B0]` (a BIC `General` percentage, the fortification bonus [I]) applies only for a land unit on a land tile with `UnitState == 1` and moves left (`clamp(max_moves - Moves, 0, 9999) > 0`), else 0. It keeps the best per `Unit_Class` with the comparator `0x4A1360(u, s, best, best_s, 0)`:
   - a King (`UTA_King` 0x1D, via C3X's hooked check 0xCF8161) loses to a non-King when `s > 0`;
   - the higher `s` wins;
   - on a tie, the unit with **fewer** carried units (`Unit_count_contained_units`, 0 for armies) wins, then lower `Unit_get_attack_strength` (0x5BE3F0), then lower `UnitType.Bombard_Strength`, then lower max HP;
   - otherwise the earlier unit in tile order stays.
8. Result: on a **water** tile (`Tile` vfunc m35 `Is_Water`, +0x8C) the best **sea** unit, else the best air unit, else the best land unit. On a **land** tile the best **land** unit, else sea, else air.

So the map shows **the strongest defender** (HP- and fortification-weighted), not the top of the stack. The exceptions are the selected own unit, animating or attacking units, and always-on-top units.

**Frame composition** (`Animator` tick 0x4EED00) [X]:
1. When the camera moved: `Map_Renderer` m71 (all terrain passes, section 4) and the fog pass 0x4C4F10, both into the map canvas MSF+0x274 (0x9F8934). Cities are part of terrain pass 3.
2. `build_unit_list` (0x4EF4D0), then 0x4EFC30.
3. `0x405FF0` (ecx = `Units_Image_Data` 0x73DA00) advances every listed unit's animation by the elapsed time. Then the non-unit effect animations in the vector at Animator+0x18DC are drawn.
4. **`Animator::draw_units` (0x4F09B0)**: for each list entry in order, skip off-map units and update the frame image (`0x402620(0, -1)` on U+0x27C). Then pick the camera copy for map wrapping (0x4F0C50: `Animator+0x191C/0x1920` = `camera_x/camera_y` (0x9FD558/55C), +0x1924.. = the same minus one map width or height). If the visible flag `U+0x400` is set, call **`Unit_draw(unit, canvas2 = 0x9FD80C, cam_x, cam_y, 1)` (0x5CBC60)**.
5. 0x4F0840 (other effect animations), then UI.

Units are therefore drawn **after terrain, cities and fog**, which matches the trace: every `colored 240 240` draw comes after the last fog draw, and the sequence is repeated 173 times while the idle animation runs. `0x9FD80C` is the canvas of `Main_Screen_Form.Units_Control` (MSF+0x4ED8, canvas at +0x274) [X].

**`Unit_draw` (0x5CBC60)** in order [X]:
1. Return unless `Unit_is_visible_to_civ(Player_CivID, 1)` or debug-see-all.
2. Colour civ: `c = CivID`. If the unit has `UTA_Hidden_Nationality` (0x11) and the viewer is a real civ other than the owner, `c = 0` (barbarian colours), unless debug bit 8 is set. Colour table: `FLC_Animation::civ_color_table(c)` (0x403040), see 10.5. Armies (`UTA_Army`) get palette entries 64..127 rewritten at 0x5CBD25 and use their own draw routine 0x5CC140 when `U+0x1D0 != -1` (see 10.11).
3. `(x, y) = pixel_loc` (halved when zoomed out) `- (cam_x, cam_y)`. This is the **unit anchor in canvas pixels**.
4. **Units in cities**: unless `p_preferences` (0xA52638) has `P_SHOW_UNITS_OVER_CITIES` (0x2000), a unit on a city tile is not drawn. Exceptions: it is `Current_Unit`, `always_on_top`, or (`field_111` and anim in 3..6). [D: in this game 11 of the 31 drawn units stand on city tiles, so the preference is on.]
5. **Team-colour disc**: if `p_preferences & P_SHOW_TEAM_COLOR_DISC` (0x800) and the tile has **no city**, draw the disc sprite (10.6).
6. If the unit is `Current_Unit` (0x9FD434): the **selection cursor** `Animator::draw_cursor(x, y)` (0x4F04A0, 10.6).
7. **The unit sprite** (10.3/10.4).
8. **HUD**: `Unit_draw_hud(canvas2, x - 32, y - 32, stack_flag)` (0x5BA460; -16/-16 zoomed out). `stack_flag` = 1 when called from the animator and the unit is neither moving (`U+0x5D`) nor always-on-top.

### 10.3 Draw position and the drawn Sprite [X][D]

`Sprite_draw_with_color_table(sprite, canvas, canvas2, px, py, flags, color_table)` (0x5F87E0) blits through JGL vfunc +0x8C at **`(px + sprite.+0x20, py + sprite.+0x24)`**. Sprite +0x20/+0x24 (the header's `Width2/Height2`) hold the FLC frame offset, and `flags` = `[0x9C7304]` = 0x0A000020 [X 0x5F8835..0x5F8849]. `color_table` is passed as `table->jgl` (+4). The trace tool labels the four extra columns `canvas, canvas2, table, p6`, but the third (`0xa000020`) is really this flags word (the same holds for the `Territory_Images` draws of section 6.9). The **fourth (`0x1104eb50` etc.) is the colour table, which is the FLC's own `Flic_Anim_Info.Color_Table`** (10.5). It therefore identifies the art: 4 distinct values in `ww2j` for 4 distinct (unit type, animation) pairs: 20 Infantry, 9 Guerilla, 1 Japanese Rifleman, 1 Fighter [D].

The sprite is `&unit->Body.Animation.Frame_1.sprite`, so **`Unit* = sprite_address - 0x2DC`** (`Unit_Body* = sprite - 0x2C0`). The sprite is a per-unit object whose JGL bitmap is overwritten with the current frame, which is why its address stays the same over all 173 redraws. [X: `lea ebx,[esi+0x2D4]` + `0x5F7CF0` (returns +8) at 0x5CBFEE..0x5CC049; D: all 31 addresses are 4 mod 8, as expected for an 8-aligned heap block + 0x2DC.] From the `Unit*` everything else follows: type U+0x40, anim U+0x2A0, facing U+0x29C, frame U+0x378, owner U+0x34. No per-unit-type sprite array exists. Frames are decoded on demand from the `Flic_Anim_Info` into the unit's own `Frame_1`.

Sprite geometry (set by `FLC_Frame_Image::load` 0x5F7A90 from the FLC header) [X]: JGL bitmap = frame size (`@8` width, `@10` height). Sprite `Width/Height` (+0x18/+0x1C) = `@0x68/@0x6A` (the 240x240 canvas), +0x20/+0x24 = `@0x64/@0x66` (`OffsetLeft/OffsetTop`).

Position, normal zoom [X 0x5CBDD6..0x5CC04B][D]:

```
x = 64*tile_x + 64 - camera_x          // unit anchor = tile centre, canvas coordinates
y = 32*tile_y + 32 - camera_y
px = x - (W >> 1)        with W = FLC@0x68 (240)  -> x - 120    (the trace's X)
py = y - (H >> 1)        with H = FLC@0x6A (240)  -> y - 120    (the trace's Y)
frame image top-left = (px + OffsetLeft, py + OffsetTop)   // e.g. Infantry: (93, 82), frame 49x47
```

With `camera = (64, 580)` this reproduces all 31 trace draws, e.g. `(840,-60)` = tile (15,19) and `(776,356)` = tile (14,32). The tile assignment is unambiguous: with the anchor at the tile centre, 11 units land on city tiles and none on water; the shifted alternative gives 0 cities and a unit on water. The frame pixels then match the screenshot **exactly at this position** for all 31 units [D]. Screen = canvas coordinates in this capture (viewport L = T = 0, as for terrain).

Zoomed out [X 0x5CC052..0x5CC0B1]: the anchor is `pixel_loc/2 - camera`, the sprite is drawn with the scaled colour blit 0x5F8870 (scale 2) at `x - W/4, y - H/4`. The HUD offsets become 16 [X]. The resampling is not checked [?].

### 10.4 Unit art, animation, direction, frame [X][D]

**Path** (`BIC` method 0x5987B0, `Art\Units\` 0x72E1A8, `.INI` 0x72E1A0): look up `Text\PediaIcons.txt` key **`#ANIMNAME_<UnitType.Civilopedia_Entry>_<Eras[era].Civilopedia_Entry>`** first (e.g. `#ANIMNAME_PRTO_Settler_ERAS_Industrial_Age` -> `Settler Modern Times`), then **`#ANIMNAME_<Civilopedia_Entry>`** (`#ANIMNAME_PRTO_Infantry` -> `Infantry`). The value `N` gives **`Art\Units\N\N.INI`**, resolved through `BIC_get_asset_path` (scenario folder first, section 11). Kings and flag units (`UTA_King` 0x1D, `UTA_Flag_unit` 0xD) take a race-specific name from 0x72D9A8.. (C3X hook 0xCF81DA) [X; which era index is passed: ?]. [D: the Guerilla art came from `civ3PTW/Art/Units/Guerilla/`, the Japanese Rifleman from the scenario folder.]

**INI** (`Animation_Info::load` around 0x404B98..0x4057DB) [X]:
- `[Speed]`: `Normal Speed` (default 100) to +0x12C, `Fast Speed` (default 150) to +0x130. These are movement speed, not frame timing [I].
- `[Version]`/`[Palette]`: optional.
- `[Animations]`: `ShowLaunch` (default 1), `ShowDrop` (default 1), `ShowAttackAnimOnce` (default 0). Then one key per `AnimationType` name from the table at 0x680758: `BLANK DEFAULT RUN ATTACK1 ATTACK2 ATTACK3 DEATH FORTIFY FIDGET VICTORY CAPTURE FORTRESS BUILD ROAD MINE IRRIGATE JUNGLE FOREST PLANT`, each value an FLC file name relative to the INI. Keys the engine never reads, although the shipped INIs contain them: `WALK DEFEND DEAD FORTIFYHOLD TURNLEFT TURNRIGHT STOP_AT_LAST_FRAME`.
- `[Timing]`: a float per name (default 0.5) is read (0x4053C4) **but not used for frame timing**.
- **Frame time** = `FLC@0x6C (animation time, ms) / FLC@0x62 (frames per direction) * 0.001` s (0x4053F7..0x405442). For Infantry: 1875/15 = 0.125 s.
- A missing animation falls back to DEFAULT (`0x4026A0`: `Animations[type] == NULL` gives type 1).

**FLC layout** [D]: header `@0x60` directions (8), `@0x62` frames per direction `n`. The file holds `8*(n+1)` frame chunks: each direction block has `n` frames plus one ring frame. **Block `b`, frame `k` = chunk `b*(n+1) + k`** (verified by the exact matches). The decoder is the one in `crates/civ3-assets/src/flc.rs`.

**Direction to block** (table 0x73DD7C, filled at runtime by 0x406D60) [X]: `enum direction` -> FLC block: `DIR_ZERO 0 -> 0`, `NE 1 -> 4`, `E 2 -> 3`, `SE 3 -> 2`, `S 4 -> 1`, `SW 5 -> 0`, `W 6 -> 7`, `NW 7 -> 6`, `N 8 -> 5`. **The blocks in file order are therefore SW, S, SE, E, NE, N, NW, W** (C7 agrees). `0x402620` loads `Animations[current_anim_type]` and seeks to `(block[direction_2], frame U+0x378)`.

**Default facing** [X][D]: the `FLC_Animation` constructor (0x4036D0 -> `AnimationSummary` ctor 0x403C30) sets `direction = direction_2 = 3` (**SE**, block 2) and `current_anim_type = 1` (DEFAULT). All 31 units in `ww2j` show block 2 (SE).

**Idle animation and frame** [X][D]:
- An idle unit loops DEFAULT. The frame index advances by elapsed real time (QueryPerformanceCounter delta in seconds, 0x4EED00) in steps of the frame time (0x403330).
- FORTIFY, DEATH and BUILD with `field_110` stop on their last frame (`frame = count-1`, 0x402841..0x402866). FORTIFY is the fortify animation, which then holds [I].
- **Fidget**: each `FLC_Animation` has a timer (anim+0x130) initialised to `90 + 0.1*(rand() % 1000)` s (90.0..189.9 s, 0x402355..0x402369; constants 0.1 at 0x665440 and 90.0 at 0x66543C) and decreased by the tick's dt (0x402F8D). When an idle unit (DEFAULT, back to idle) finds the timer negative, it re-arms it the same way and plays **FIDGET** once (0x40270C..0x40275D), then returns to DEFAULT.
- For a static reimplementation: read `current_anim_type` (U+0x2A0), facing (U+0x29C) and frame (U+0x378) from memory. Without memory, use DEFAULT, block 2, frame 0. [D: in `ww2j` every 15-frame DEFAULT unit (Infantry, Guerilla, Japanese Rifleman) was on **frame 4**, all in sync. The Fighter's DEFAULT has 1 frame per direction and showed frame 0.]

### 10.5 Colours and shadows [X][D]

**Civ colour** (`FLC_Animation::civ_color_table(civ)`, 0x403040) [X]: take `Units_Image_Data.Color_Tables[leaders[civ].Color_Table_ID]` (array at **0x73DA04**, 32 tables loaded from `Art\Units\Palettes\ntpNN.pcx`, NN two-digit, 0x406570). Read entries **0..63** of that table and write them into entries **0..63** of the current animation's own FLC table (`Flic_Anim_Info+0x78`), using JGL get/set `(buf, 0, 0x40)`. That FLC table is passed to the blit. So **palette indices 0-63 come from `ntpNN`, and 64-254 keep the FLC's own palette** (ntp entries 64-69 are UI colours, e.g. 66 for the team disc). [D: over all 31 units every opaque pixel (index < 224) matched exactly: ntp03 for Chinese units (civ 1, `Color_Table_ID` 3), ntp01 for the Japanese one (civ 5, id 1).]

**Transparency and shadows** [D]: index 255 is transparent. Shadow indices darken what is already on the canvas, per RGB555 channel:

```
c' = floor(c * (idx - 239) / 16)        // idx 248 -> 9/16, 249 -> 10/16, ..., 254 -> 15/16
```

Measured on all 31 units against the offline terrain render (`civ3-tool render`): 248 1323/1323, 249 3366/3366, 250 3088/3108, 251 1686/1695, 252 2210/2301, 253 2523/2802, 254 4386/4731 channel samples exact. The best k was `idx - 239` for every unit and index. The misses at the outer indices 252-254 are where the shadow falls on the team disc, which the offline render lacks [I]. This is the same `floor(c*k/16)` rule as section 6.7a, with k = idx - 239. Indices 224-247 did not occur in these frames, so extending the rule to 240-247 is [I] and the meaning of 224-239 is [?]. The JGL `flags` word 0x0A000020 is not decoded [?].

### 10.6 Selection cursor, team disc, health bar, LED, stack lines, fortify frame [X][D]

All positions below are canvas pixels relative to the anchor `(x, y)` of 10.3 (normal zoom). `X = x - 32` and `Y = y - 32` are the HUD origin.

- **Selection cursor** (`Animator::draw_cursor` 0x4F04A0, only for `Current_Unit`, drawn *before* the unit sprite). Skipped while `Animator+0x1914 & 1` (the blink phase [I]). It draws the frame of `Animator.Unit_Cursor_Animation` (Animator+0x193C, art `Art\Animations\Cursor\Cursor.INI` -> `Cursor.flc`, block for direction 5, running frame) with `Sprite_draw_with_color_table` at `(x - W/2, y - H/2)`, coloured with the viewer's civ table [X]. It is not present in `ww2j` (no unit selected).
- **Team-colour disc** (only with `P_SHOW_TEAM_COLOR_DISC`, not on city tiles; drawn before the cursor and the unit): sprite 0xB3F2BC = `Art\Cursor.pcx` cell (259, 155, 128x64), a 1-px green (index 247) ellipse ring. Zoomed out it is sprite 0xB3F290, cell (259, 90, 128x64) (0x54B048..0x54B088). It is drawn with 0x5F83E0 at `(x - 64, y - 32)` (the tile box; zoomed out `x - W/4, y - H/4`) in a single colour = **`ntpNN` palette entry 66** of the owner's `Color_Table_ID` (`Leader` 0x55A350 -> 0x539E20 reads entry 0x42 and converts it to 555/565). [D: ring pixels at (896, 476) - (64, 32) are (231,231,0) = ntp03[66] (230,230,0) in RGB555; no ring on city tiles.]
- **Health bar** (`Unit_draw_hud` 0x5BA460). Skipped entirely for units inside an army. Drawn only if attack > 0 or defence > 0. With `max = Unit_get_max_hp` = `max(1, CombatExperience[exp].Base_Hit_Points + UnitType.Hit_Point_Bonus)` (0x5BE2C0) and `hp = clamp(max - Damage, 0, 9999)`:
  ```
  pitch = (5*max - 1 > 40) ? max(2, 41 / max) : 5
  barH  = min(pitch*max - 1, 40)
  rect  = [X+6, X+8) x [Y+24-barH, Y+24)        // 2 px wide, bottom row y-9; X -= 4 for PRTO_Mech_Infantry unless 0x539310(2)
  fill rect black; colour = green (0,255,0) if 3*Damage < max or Damage == 0,
                            yellow (255,255,0) if 3*Damage < 2*max, else red (255,0,0); red if hp <= 1
  for i in 0 .. min(hp, 20): fill [X+6, X+8) x [bottom - pitch + 1, bottom) with colour; bottom -= pitch
  ```
  So each HP is a `pitch-1` px segment with a 1 px black gap, filled from the bottom. [D: unit at anchor (896, 476), Infantry with 3 HP: black/green column at x = 870-871, rows 454-467, green 454-457, 459-462, 464-467, black 458 and 463.]
- **Fortified frame**: for a land unit on land with `UnitState == 1`, fortification bonus `0x467D60(u) > 0` and `[0x9C72B0] > 0` (armies use 0x42B630), the outline 0x6008D0 is drawn in white at `(left-1, top-1, right, bottom)` around the bar [X; not in the data]. Exact pixels and condition: 10.10.
- **Movement LED** (0x5BA380, own units only, i.e. `CivID == Player_CivID`). The 6x6 sprites are cut from `Art\interface\MovementLED.pcx` at (1,1) green, (15,1) yellow and (29,1) red (0x54B986..0x54B9C1); magenta is transparent. Selection: remaining move points `max_moves - Moves <= 0` (and not `U+0x5D`) -> red; remaining != max -> yellow; full -> green. Position: `((left + right - 6)/2, top - 6)` = `(X+4, Y+18-barH)`, drawn with `Sprite_draw` 0x5F8210 [X]. [D: (868, 448) holds the red LED exactly.]
- **Stack lines** (when `stack_flag`): `n = 1 +` the number of other units on the tile for which `0x4E3E50(u')` is false, clamped to 8. If `n > 1`, draw `n` white lines from `(X+5, Y+26+2i)` to `(X+8, Y+26+2i)` (0x5FFFB0), i.e. below the bar [X]. [D: the unit at anchor (640, 92) in a 4-unit city shows white 4-px lines at x = 613-616, y = 86, 88, 90, 92.]
- City labels and the other UI are drawn later by the form, outside this pipeline.

### 10.7 Open points

- The meaning of `U+0x1DC`, `U+0x1E8`/0x5B2BE0, `U+0x24F`, `U+0x259`. (Armies and `Unit_is_visible_to_civ`: now 10.11 and 10.12.)
- The JGL blit flags `0x0A000020`, shadow indices 224-247, and zoomed-out resampling.
- Which era index selects the `_ERAS_` art variant (probably the owner's era).
- Wrapping: the alternate camera copies at Animator+0x1924..0x1938 are set only when the view straddles the map edge (0x4EEF49..0x4EF067); not exercised by `ww2j` (no wrap).

### 10.8 Harbour / airport / barracks icons beside cities (the small white diamonds) [X][D]

The small white diamond icons near some units are **city icons**, not unit markers. A city that has a harbour-, airport- or barracks-type building shows a 23x15 white diamond with an anchor, a plane or a sword next to its tile. They are drawn by the city-label pass, **on top of the units**. So when a unit stands on the tile south-east of such a city, the icon covers the unit's head. [D: in `u2` the sword icon of Taiyuan (city at (16,22)) covers the Infantry at (17,23), and the one of the city at (11,41) covers the Guerilla at (12,42).]

**Where** [X]: `Main_Screen_Form::draw_city_hud` (PCG **0x4E5640**; C3X calls it `Main_Screen_Form_draw_city_hud`, GOG 0x4E5580). It is called from 0x4C3100 (the draw method of an overlay control; the call at 0x4C3150, followed by the colony labels 0x4E4990), not from the Animator. It loops over all cities (`Cities` list 0xA52E2C, `LastIndex` 0xA52E38). For each city it draws the name/production label boxes (OpenGL lines plus text), then the capital/HUD star (`City_draw_hud_icon` 0x4B9CF0, beside the label), then these icons. The order relative to units is [I] (separate overlay control). The [D] overlap above shows that it lands above the unit sprites.

**City filter** (0x4E5748..0x4E57C2) [X]: drawn if debug-see-all (`0xA52640 & 8`, via 0x427390) or if the city tile is **explored** by the viewer (`0x4EDF90`: Tile+0x58 bit of the viewer). The tile must also pass 0x4E6AB0 (on-screen test [I]).

**Position** (normal zoom) [X 0x4E68BD..0x4E69FD][D]: `0x4E3BD0(city.x, city.y)` returns the city tile's box top-left `(sx, sy)`. In canvas coordinates this is `(64*cx - camera_x, 32*cy - camera_y)` = city anchor - (64, 32). The icons are plain `Sprite_draw` (0x5F8210, index 255 transparent) at the sprite cell's top-left:

| Icon | Sprite | Art cell (x, y, w, h) | Draw position | Condition |
|---|---|---|---|---|
| Harbour (anchor) | 0xB3F340 | `Art\Cities\airAndHarb.pcx` (27, 1, 25, 17) | `(sx, sy + 47)` | `City_shows_harbor_icon` (C3X hook 0xCF8744, called at 0x4E68B4) |
| Airport (plane) | 0xB3F314 | `Art\Cities\airAndHarb.pcx` (1, 1, 25, 17) | `(sx + 103, sy + 47)` | `City_shows_airport_icon` (C3X hook 0xCF87B3, called at 0x4E6902) |
| Barracks (sword) | 0xB3F2E8 | `Art\Cities\barracks.pcx` (1, 1, 25, 17) (only in `Conquests\Art\Cities\`) | `(sx + 103, sy + 29)`, i.e. 18 px above the airport slot | `0x4AE160(city)` **and** `city.CivID == Player_CivID` (own cities only) |

Relative to the city anchor `(ax, ay)` the three cells sit at `(ax-64, ay+15)`, `(ax+39, ay+15)` and `(ax+39, ay-3)`. The visible diamond is the cell minus its 1-px transparent border, so 23x15 starting at cell + (1, 1). The sprites are cut in 0x54A5B0 by `0x5F7EC0(pcx, x, y, w, h, 1, 1)`. Paths go through `BIC_get_asset_path` (0x5982A0), so scenario folders can override them. Zoomed out the code subtracts the sprite width (`[0xB3F350]`, `[0xB3F324]`, `[0xB3F2F8]`) and uses the half-size tile constants. Zoomed-out positions are [X] in code but not checked.

[D] With camera (64, 580), **every** non-label magenta cluster in `u2-diff2.png` sits exactly at one of these slots. Examples: barracks (1064,154) of city (16,22) and (744,762) of city (11,41); airport (1320,44) of a Japanese city; harbour (1345,684) plus barracks (1448,666) of Foochow (22,38). Foreign cities never show the sword.

**Conditions** [X]:
- `0x4AE160` (barracks): count the buildings `i` (`BIC.ImprovementsCount` 0x9C3D40, `Improvements` 0x9C406C, stride 0x110) that the city has active (`0x4ACBE0(city, i, 1)`), that are not obsolete for the owner (`Improvement+0xE0` = obsoleting advance; obsolete if >= 0 and the owner knows it, 0x5613F0), and whose `ImprovementFlags` (+0xEC) has `ITF_Veteran_Ground_Units` (0x2). Show the icon if the count > 0.
- Harbour and airport: C3X replaces both calls. With `city_icons_show_unit_effects_not_trade = true` (the value in the installed `default.c3x_config.ini`, no custom config present), the test is `City_count_improvements_with_flag` (0x4B2020, the same loop as above) with `ITF_Veteran_Sea_Units` (0x20000) for the harbour and `ITF_Veteran_Air_Units` (0x40000) for the airport. With the option off it is vanilla `City_can_trade_via_water` (0x4AE0C0) / `City_can_trade_via_air` (0x4AE020), i.e. `ITF_Allows_Water_Trade` 0x100000 / `ITF_Allows_Air_Trade` 0x200000. Under standard rules both variants pick Harbour and Airport.
- The icons are independent of units and of the label preference bits. They need the city's building list (the trace's `city ... <hex bits>` column) and `BIC.Improvements[].ImprovementFlags`.

Side note [D]: `u2-diff2.png` also shows 6 missing white stack lines at x = 1189-1192, y = 246..256. They belong to the Japanese unit 450 in Kaifeng (19,25), anchor (1216,252): `X+5..X+8`, `Y+26+2i` as in 10.6. The renderer does not draw that stack. The cause (probably stacked units missing from the trace) is not investigated here.

### 10.9 City-site radius preview for a selected settler (the thin orange/red diamond) [X][D]

**What it is**: when the selected unit can found a city, the game outlines the tiles the new city would work. In a fresh game that is the 3x3 block around the settler (radius-1 culture border). The lines are drawn with the OpenGL line renderer, so the sprite tracer never sees them. This is vanilla code (C3X only wraps the line primitive, see below).

**Call site** [X]: `Unit_draw` (0x5CBC60), right after `draw_cursor` (0x5CBFB2..0x5CBFE4):
```
if unit == Current_Unit (0x9FD434):
    Animator::draw_cursor(x, y)
    if Unit_can_perform_command(unit, UCV_Build_City = 0x20000002)   // 0x5C17E0 (C3X inlead)
       and unit.current_anim_type (U+0x2A0) == 1 (DEFAULT, i.e. not walking):
        Main_Screen_Form::draw_city_radius_preview()                  // 0x4DD8C0, ecx = MSF 0x9F86C0
```
`0x4DD8C0` returns at once if `MSF+0x2E1AD` is set. It sets that flag (and MSF+0x2FAC7) when done. The Animator clears the flag at 0x4EF0E6, right after it redraws terrain (m71) and fog (0x4C4F10) into the map canvas. So the preview is drawn **once per terrain redraw, into the map canvas MSF+0x274 (0x9F8934), after terrain, cities and fog and before all units** (units go to the Units_Control canvas). [D: the lines are not darkened where they cross fog.]

**Centre tile** [X 0x4DD8D8..0x4DD986]: `MSF+0x4E8C/0x4E90` (`field_4E80[3..4]`) hold the tile under the mouse [I: cleared to -1 by the mouse handler at 0x4EAEE6, set from `get_tile_coords_under_mouse`].
- If that tile is on the map and the viewer has **explored** it (Tile+0x58 bit), the radius is centred on the **mouse tile**.
- If it is on the map but unexplored, nothing is drawn.
- If it is -1/off the map (mouse not over the map), the radius is centred on the **unit's own tile** (U+0x24/0x28).

In `sel-game.png` it is centred on the settler, so the mouse was off the map.

**Line style** [X 0x4DD98A..0x4DD9E0]: `OpenGLRenderer_initialize(MSF+0x274)`, `set_color(Leader 0x55A350(unit.CivID))` = the owner's **ntp palette entry 66** (the team-disc colour, as RGB555/565), `set_line_width(2)`, `set_opacity(0x80)`. [D: Rome's entry 66 = (231,8,8) (also the disc ring in the same shot). Line pixels like (181,66,24) over grass (132,132,49) are exactly `(colour + bg)/2` truncated to RGB555 (176|5, 64|2, 24|0 after 5-bit expansion), i.e. 50 % alpha.]

**Which edges** [X 0x4DDA1A..0x4DDD8A, cached per (unit ID, x, y) in `MSF+0x4DC0..0x4DC8`; the edge mask is 21x4 bytes at `MSF+0x4DCC`]: first the centre must pass `Map_check_city_location` (0x5F3090, C3X-hooked) == 0, otherwise no lines. Then it walks the 21 tiles `n = 0..20` of the city radius in spiral order (`0x5E6D80(n)` gives the offset; n = 0 centre, 1..8 the inner ring, 9..20 the outer ring of the fat cross). A tile is **in** the area if:
- `n < 9`: `0x5D41C0(x, y)` (owner the tile would have, [I]) is 0 or the unit's civ;
- `n >= 9`: the tile's territory owner (`Tile` vfunc +0x98) is the unit's civ, or `Map 0x5D42A0(x, y, k)` (k = the neighbour index toward the centre, from 0x5E6C50) returns the unit's civ [semantics I].

For each tile that is in, and each diagonal neighbour `d = 0..3` with `(dx, dy)` from the tables 0x67055C / 0x67056C = `(+1,-1) NE, (+1,+1) SE, (-1,+1) SW, (-1,-1) NW`, the edge is flagged if the neighbour is not in the area. That means it lies outside the 21-tile radius (spiral index >= 21 via 0x5F3E80), or it fails the same owner test as above.

**Geometry** [X 0x4DDDCD..0x4DE269]: for each flagged edge of tile T, with `(cx, cy)` = T's centre in canvas coordinates (`0x4E3BD0` box top-left + (64, 32); wraps handled):
```
OpenGLRenderer_draw_line(cx + 64*dx, cy,  cx, cy + 32*dy)     // one tile-diamond edge, vertex to vertex
```
So the outline follows the tile-diamond edges, exactly one tile beyond the settler's ring. [D: `sel` (camera (1984,292), settler at (45,25), anchor (960,540)): the outline is the 3x3 block's diamond with vertices (960,444), (1151/1152,540), (960,636), (768,540). No outer-ring tiles are included because nothing is owned yet.]

**Rasterisation** [D, measured; exact GL rule I]: a width-2 line of slope 1/2 covers 4 consecutive pixels per row (2 pixels per column). On the NW edge `x_ideal = 960 - 2*(y - 444)` the pixels are `x_ideal-2 .. x_ideal+1` (e.g. y = 446: x = 954..957). The ends at the left/right vertices thin out (rows 538..542 near x = 768..773). Each pixel is `(c + bg)/2` per channel, as RGB555.

C3X note: with `draw_lines_using_gdi_plus = wine` (installed default) and the game running under Wine, `patch_OpenGLRenderer_draw_line` (injected_code.c) draws the same line with GDI+ (`GdipDrawLineI`, antialiased, pen width 2, colour = rgb555 expanded, alpha 0x80) instead of OpenGL. The screenshots here come from the Windows VM, so OpenGL is used and the pixels above apply.

### 10.10 Fortified frame around the health bar (exact) [X]

In `Unit_draw_hud` (0x5BA460), after the bar segments (0x5BA67B..0x5BA731):
```
cont = Units.get(U.Container_Unit (U+0x60))
if cont and Unit_has_ability(cont, UTA_Army 0x12):        // dead path: the HUD already returned at 0x5BA486 for army members
    bonus = 0x42B630(cont)
else:
    if UnitType.Unit_Class (+0x9C) != 0  -> no frame        // land units only
    if tile_at(U.x, U.y) is water (Tile vfunc +0x8C) -> no frame
    if U.UnitState (U+0x64) != 1         -> no frame        // fortified
    if 0x467D60(u) <= 0                  -> no frame        // remaining moves = clamp(max_moves - Moves, 0, 9999)
    bonus = [0x9C72B0]                                      // BIC.General.DefenceBonus_Fortification (General+0x10C) [H]
if bonus > 0:
    PCX 0x6008D0(left-1, top-1, right, bottom, white, white)
```
- **0x467D60 is not a fortification bonus**: it returns the unit's remaining move points, `clamp(Unit_get_max_move_points(u) - U.Moves, 0, 9999)` [X]. So a fortified unit that has used all its moves this turn has **no frame**.
- **`[0x9C72B0]`** = `BIC+0x3DE8` = `General.DefenceBonus_Fortification` (offset 0x10C in `General` at BIC+0x3CDC) [H, offsets X]. It only gates the frame (the rules value, normally 25). 0x42B630 is the general "fortification bonus" helper: it walks up to the outermost army, then returns `[0x9C72B0]` under the same land/land/state 1/moves left conditions.
- `left, top, right, bottom` are the bar rectangle of 10.6: `left = X+6` (-4 for Mech Infantry), `right = X+8`, `top = Y+24-barH`, `bottom = Y+24` (X = x-32, Y = y-32).
- **0x6008D0** draws four 1-px JGL lines (vfunc +0x64; endpoints inclusive, as for the stack lines): top `(x0+1..x1, y0)`, bottom `(x0..x1-1, y1)`, left `(x0, y0..y1-1)`, right `(x1, y0+1..y1)`. With `(x0,y0,x1,y1) = (left-1, top-1, right, bottom)` this is the **closed 1-px outline of the inclusive box `[X+5, X+8] x [Y+23-barH, Y+24]`**: one pixel left of, one right of, one above and one below the 2-px bar.
- Colour: white, `0x8000FFFF` in 565 mode / `0x80007FFF` in 555 mode (bit 31 is a JGL flag) [X].
- Order: frame after the bar, **before** the movement LED (0x5BA736..). The 6x6 LED at `(X+4, Y+18-barH)` covers `x = X+4..X+9`, `y = Y+18-barH..Y+23-barH`, so it overwrites the frame's top row except where the LED is transparent (magenta).
- No fortified unit (`state == 1`) occurs in any trace in `/mnt/datasets/winvm/civ3r-runs` (all `state` = 0), so this is [X] only.

### 10.11 Armies [X]

Army unit = a unit whose type has `UTA_Army` (0x12; `Unit_has_ability` 0x5BC5C0). Fields [X]: **`U+0x1D0`** (`Body.field_1B0[1]`) = **ID of the displayed member**, i.e. the best defender. When a unit joins (0x5BC9E9..0x5BCB48) the game increments **`U+0x1D4`** (`field_1B0[2]`) = **member count**. It then scores the old and the new member like `find_visible_unit` (defence x (100 + fortify bonus) x HP, comparator 0x4A1360) and stores the winner's ID in U+0x1D0. Both are reset to 0 / -1 at creation (0x5BB7E9). Members have `Container_Unit` = the army and are never picked by `find_visible_unit` (10.2) and get no HUD.

**Palette** (`Unit_draw` 0x5CBD10..0x5CBD95, every army, before the branch): get entries 0..15 of `Units_Image_Data.Color_Tables[leaders[army.CivID].Color_Table_ID]` (0x73DA04; JGL vfunc +0x10 `get(buf, 0, 16)`, 3 bytes per entry). Replicate them 4 times (3 x `memcpy 0x30`). Write the 64 entries into the army animation's FLC table (`ebp` = result of `civ_color_table`) at **entries 64..127** (vfunc +0x14 `set(buf, 64, 64)`). So **army art index `64 + k` gets `ntp[k mod 16]`**, on top of the normal 0..63 civ replacement (10.5). (The old note's "3-colour table" was wrong.)

**Branch**: if `U+0x1D0 == -1` (empty army) the normal path (10.2/10.3) draws the army's own art. Otherwise `0x5CC140(canvas, cam_x, cam_y, table, stack_flag)` draws it and `Unit_draw` returns:
```
M = Units.get(A.U+0x1D0)                                  // displayed member
(mx, my) = A.U+0x5D (moving) ? A.pixel_loc : M.pixel_loc;  (ax, ay) = A.pixel_loc     // both /2 when zoomed out
mx -= cam_x; my -= cam_y; ax = ax - cam_x + 40 (20 zoomed out); ay -= cam_y
if A.current_anim_type == 3 (ATTACK1) and A.direction_2 == 2 (E): ay -= 20 (10 zoomed out)
civ = A.CivID, or 0 for UTA_Hidden_Nationality seen by another real civ (as 10.2 step 2)
tableM = civ_color_table(M.anim, civ)                      // M's FLC table, entries 0..63 from ntp
n = min(A.U+0x1D4, 4); copy ntp[0..15] of leaders[0] (literal 0xA53F50 = leaders[0].Color_Table_ID) n times
    and set entries 64 .. 64+16n-1 of `table` (A's FLC table)       [X; using civ 0 here looks like a game bug, effect ?]
return unless Unit_is_visible_to_civ(A, Player_CivID, 1) or debug-see-all
units-in-cities rule as 10.2 step 4, but the only exceptions are Current_Unit and anim 3..6
team disc (P_SHOW_TEAM_COLOR_DISC, no city) at (mx - 64, my - 32), civ colour
if A == Current_Unit: draw_cursor(mx, my)                 // no settler radius on this path
Sprite_draw_with_color_table(A.frame, 0x9F8934, canvas, ax - W/2, ay - H/2, [0x9C7304], table)    // army banner art, 40 px right
Sprite_draw_with_color_table(M.frame, 0x9F8934, canvas, mx - W/2, my - H/2, [0x9C7304], tableM)   // member figure, in front
   (zoomed out: 0x5F8870 scale 2 at x - W/4, y - H/4)
Unit_draw_hud(A, canvas, mx - 32, my - 32, stack_flag)     // HUD of the army, at the member's anchor (-16 zoomed out)
```
So an army shows as the **strongest member's figure at the tile anchor, plus the army's own (banner) animation 40 px to the right drawn behind it**. The health bar, LED and stack lines are the army's own. The member's `Frame_1` must be animated too. It is not in the draw list itself, so its frame is advanced only through the army [I]. No army occurs in the available traces.

### 10.12 `Unit_is_visible_to_civ(unit, civ, param_2)` (0x5BB360) [X]

The installed exe jumps to C3X `patch_Unit_is_visible_to_civ` first. That only adds hot-seat shared vision (`share_visibility_in_hoseat`, offline non-PBEM hot-seat games: visible if visible to any other human player) and otherwise returns the original result. Original:
```
if unit.CivID == civ: return 1
if unit.Container_Unit != -1: return Unit_is_visible_to_civ(container, civ, param_2)   // 1 if the container ID is invalid
t2 = 0x5BC3E0(unit)            // armies: a member's type (else -1)
invisible = UnitType[unit.type] has UTA_Invisible (0x7)  or  (t2 != -1 and UnitType[t2] has UTA_Invisible)
if !invisible: return 1        // ordinary units: always "visible" here; fog/visibility is handled by 10.2 tile_visible_now
if civ == -1: return 0
if race slot(owner) != 0 and race slot(owner) == race slot(civ): return 1
        // 0x5E22F0 on BIC.Game (0x9C4090) with leaders[].RaceID (+0x20): same team/slot [I]
if leaders[civ]+0xEB0+4*owner has bit 6: return 1      // the same diplomatic bit as 10.2's shared-vision test
for n in 1 .. 24:                                        // spiral 0x5E6D80: ring 1 (8 tiles) and ring 2 (16 tiles); the unit's own tile (n = 0) is NOT scanned
    (x, y) = unit tile + offset(n), wrapped; skip if off-map (Map_in_range 0x426C50)
    for each unit d on (x, y) (TileUnits list):
        if d.CivID == civ and (UnitType[d.type] or UnitType[0x5BC3E0(d)]) has UTA_Detect_Invisible (0x16; C3X hook 0xD00314)
           and 0x5B9D20(d, unit.x, unit.y):              // detector can see the target tile [range rule I: uses tile distance 0x44A950;
               return 1                                  //  sea detectors that are fortified (state 1) or in state 0xF get special handling at distance <= 3]
return 0
```
Renderer consequences [X]: `Unit_draw` (both paths) and `find_visible_unit` (10.2 step 3) call it with `civ = Player_CivID`, `param_2 = 1`. Invisible units (e.g. submarines, `UTA_Invisible`) of other civs are skipped unless they are on the same team, have the bit-6 diplomatic relation, or a viewer-owned detector stands within the 24 surrounding tiles and passes 0x5B9D20. Debug-see-all (`0xA52640 & 8`) bypasses it in both callers. The trace's `visible` column is the result of this call for the viewer [I].

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

---

## 16. User interface drawing

Numbered 16 because section 15 already exists. This section covers everything that is not the map: forms (screens, dialogs, advisors, city screen), controls (buttons), the HUD, the cursor and tooltips. It gives the hook points needed to rebuild every UI blit as *(art file, source rect) -> (screen x, y)*. All addresses are PCG; none of the functions below are patched at their entry by C3X (I compared the first 16 bytes in `Civ3Conquests.exe` and `Civ3Conquests-Unmodded.exe`) [X]. Struct offsets come from the C3X header compiled with `gcc -m32` [H] unless the disassembly reads them, in which case they are tagged [X]. Calling convention: every method is `__thiscall` (`this` in ECX, args on the stack, callee pops; `ret N` gives the arg count). C3X writes them as `__fastcall` with a dummy EDX.

### 16.1 Layers and objects

```
art file --PCX_Image_read_file--> PCX_Image (8-bit indexed, palette)
            --Sprite_slice_pcx--> Sprite (owns a JGLSprite: copy of the pixels of a sub-rect)
                  --Sprite_draw*--> PCX_Image used as a canvas (16-bit, one per form/control, plus temporaries)
                          --Base_Form m79 (0x602710)--> back buffer PCX_Image at 0xCACFF0 (screen-sized, 16-bit)
                                  --jgl BitBlt 1:1--> window
```

The blitter is `jgl.dll`. It is loaded with `LoadLibraryA` and has one export, `get_graphsy_object_ptr`. The returned "graphics" object is stored at **0xCAB28C** [X: 0x6306E0]. Its vtable (jgl RVA 0x685F8) creates every pixel object:

| graphics vt slot | jgl impl (base 0x10000000) | meaning |
|---|---|---|
| +0x04 | 0x1003B400 | create window (controller 0xCACD30, ..., back buffer 0xCACFF0, ...) [X: call at 0x620288] |
| +0x30 | 0x1003BBB0 | `InvalidateRect(hwnd, rect, FALSE)` [X] |
| +0x7C | 0x1003BFF0 | new `JGL_Image` (0x4D4 bytes, vtable jgl RVA 0x68238) [X] |
| +0x80 | 0x1003C060 | new `JGLSprite` (0x50 bytes, vtable jgl RVA 0x68440) [X]. Called from exactly 5 places: 0x5F8A72, 0x5F8AC0, 0x5F8B30, 0x62AD0B, 0x62B217 (= the 5 sprite constructors in 16.3) |
| +0x9C / +0xA0 | 0x1003C0E0 / 0x1003C100 | set / get the global draw scale `(num_x, num_y, den)` (jgl globals RVA 0x6C0FC/0x6C100/0x6C104) [X]. UI code sets 1/1/1 (0x60272A, 0x6083C8); the map sets 1/2 when zoomed out |
| +0xA4 | 0x1003BAA0 | present: `BitBlt(window DC, r.left, r.top, w, h, backbuffer DC, r.left, r.top, SRCCOPY)` for a dirty rect (whole buffer if NULL) [X] |
| +0xB4 | 0x1003BF10 | pixel format query (used ~800x for colour conversion) [I] |

**Back buffer = screen.** `PCX_Image` at **0xCACFF0** (its `JGL_Image*` is at 0xCACFF4) is passed to Create_Window [X: 0x62027B], stored in graphics+0x148, and BitBlt'd to the window at the same coordinates [X: 0x1003BAA0]. A pixel at (x, y) of 0xCACFF0 is window client pixel (x, y). No scaling happens after this point (StretchBlt is imported but not used on this path).

### 16.2 PCX_Image (images and canvases)

`PCX_Image` (0x2B8 bytes): +0x00 vtable (0x6711D0), **+0x04 `JGL_Image*`** (NULL when empty), +0x08 image type, +0x20 creation arg, **+0xAC owned `PCX_Color_Table*`** (palette, created by the loader when `flags & 1`) [X]. The `JGL_Image` holds the pixels. Read its size with JGL_Image vt+0xD8 `Get_Width()` and vt+0xDC `Get_Height()` [X: used everywhere]. vt+0xE4 returns a format struct whose first dword is the bit depth (8 or 16) [X]. Other fields: +0x24 BitCount, +0x40 stride, +0x44 Clip_Rect, +0x54 Image_Rect, +0x80 back-pointer to the owning `PCX_Image` [X: ctor 0x100015F0 stores its arg there], +0x4BC HDC, +0x4C0 Bits_Data (DIB section) [H].

| Function | Address | Signature | Notes |
|---|---|---|---|
| `PCX_Image_construct` | 0x5FC300 | `(this)` | empty image [X] |
| `PCX_Image_destruct` | 0x5FC460 | `(this)` | calls clear [X] |
| `PCX_Image_clear` | 0x5FC4D0 | `(this)` | frees the JGL_Image and the owned colour table [X] |
| **`PCX_Image_read_file`** | **0x5FC700** | `int (this, char* path, PCX_Color_Table* ct, int ct_start, int ct_count, unsigned flags)`, `ret 0x14` | [X] see below |
| `PCX_Image_create` (blank surface) | 0x5FC5F0 | `int (this, int w, int h, int bpp, int type, int ?, int a6)`, `ret 0x18` | graphics vt+0x7C then JGL_Image vt+0x04(w, h, bpp, type). Canvases are created with bpp 16 (`0x10`) [X: 0x60A13E] |
| `PCX_Image_resize` | 0x5FC690 | `(this, w, h, ?)` | re-creates with the same type [X] |

**Load hook (read_file, 0x5FC700)** [X]: at entry, ECX = `PCX_Image*` and `[esp+4]` = `char* path`. Copy the string immediately: callers pass the static result of `BIC_get_asset_path` (0x5982A0, `char* (BIC* this=0x9C34C8, char* rel_path, bool popup)`), so the path is already resolved (scenario folder, then roots, section 11). If the path has no '.', ".pcx" is appended to a local copy. `flags & 1`: a new palette is allocated into `img+0xAC`, and `ct` must be NULL. `flags & 2`: the shared palette at 0xCAB270 is used. Otherwise the caller's `ct` is used. Return 0 = success. After return, `img+4` holds an **8-bit** JGL_Image with the PCX's size. In 378 of 382 call sites the image is a stack temporary: load, slice sprites, destroy. **The same `PCX_Image*` address is therefore reused for different files**, so keep "current file of this PCX_Image", update it on every read_file / create / clear, and look it up at slice time. 0xCAA808 is a shared scratch image used by "lazy" sprites (16.3) [X].

### 16.3 Sprites: creation and source mapping

`Sprite` (0x2C bytes) [X: 0x5F7D80, 0x62B170]:

| Off | Field | Meaning |
|---|---|---|
| +0x04 | `JGLSprite*` | pixels (a private copy of the source rect, 8-bit, colour key 0xFF set by JGLSprite vt+0x18(0xFF)) |
| +0x08 | `char*` | "lazy" file path (only for `Sprite_from_file`); if set, the sprite draws the whole file |
| +0x0C | `PCX_Color_Table*` | palette taken over from the PCX (only `Sprite_slice_pcx_with_color_table`) |
| +0x10 / +0x14 | Width3 / Height3 | **cropped** size = size of the JGL pixels |
| +0x18 / +0x1C | Width / Height | **requested** cell size (w after the odd-width fix, h) |
| +0x20 / +0x24 | Width2 / Height2 | **crop offset**: number of fully transparent (0xFF) columns on the left / rows on the top that were trimmed |
| +0x28 | type | JGLSprite type (slice arg 6; usually 1) |

**The Sprite stores neither the source PCX nor the source x/y**, so the hook has to capture them at creation:

| Constructor | Address | Signature | Source recorded |
|---|---|---|---|
| **`Sprite_slice_pcx`** | **0x5F7EC0** | `int (Sprite* this, PCX_Image* img, int x, int y, int w, int h, int type, bool crop)`, `ret 0x1C` | [X] Does nothing (returns 0) unless `img` is 8-bit. **If `w > 1` and odd, `w` is decremented** (so a 49x49 resource cell becomes 48 wide). Forwards to the worker 0x62B170 (only caller), which checks `x+w <= W`, `y+h <= H`, creates the JGLSprite and, if `crop`, trims fully-0xFF rows/columns on all four sides (fills +0x10..+0x24). Source pixels = `(x + spr[0x20], y + spr[0x24], spr[0x10], spr[0x14])` in the file. Typical call: `(img, x, y, w, h, 1, 1)` |
| `Sprite_slice_pcx_with_color_table` | 0x5F7FB0 | same args | calls 0x5F7EC0, then moves `img+0xAC` (palette) to `spr+0x0C` [X]. Hooking 0x5F7EC0 covers it |
| slice with colour key | 0x5F7F30 | 10 args, `ret 0x28` -> worker 0x62AC60 | like 0x62B170 but the transparent key is an argument (JGLSprite vt+0x18(arg)). Only caller is 0x5F8000 (palette-taking wrapper), called from 0x5CE5C5 [X] |
| copy constructor | 0x5F8A20 | `Sprite* (this, Sprite* src)`, `ret 4` | copies all fields, duplicates the JGL pixels (vt+0x0C); inherits src's origin [X]. 1 caller (0x60165A) |
| blank 8-bit | 0x5F8AA0 | `(this, int w, int h)` | generated; callers 0x5F7AEE, 0x5F7C09 [X] |
| blank screen format | 0x5F8B10 | `(this, int w, int h)` | generated; callers 0x625436.., 0x630626 [X] |
| lazy file sprite | 0x5F7E20 | `int (this, char* path, int w, int h)`, `ret 0xC` | stores a copy of the path in +0x08. Every draw loads the file into 0xCAA808 and blits the whole image (`PCX_Image_draw_onto`). 1 caller (0x6112A0) [X] |
| reset/free | 0x5F7DB0 | `(this)` | releases the JGLSprite; drop the mapping [X] |
| `Sprite_construct` | 0x5F7D80 | `(this)` | zeroes the fields [X] |

A sprite sliced from an image that was never loaded from a file (a canvas, the minimap image, a temporary composed image) has no file origin: treat it as "generated, from surface S".

### 16.4 Blits onto canvases

The exe wrappers add the crop offset (`spr+0x20/+0x24`, multiplied by the current scale, which is 1 in UI code) and then call a JGLSprite vtable slot [X]. **Effective top-left of the stored pixels = (x + spr[0x20], y + spr[0x24])**, and their size is `(spr[0x10], spr[0x14])`. All return 7 if the sprite or canvas has no JGL object. All except Sprite_draw return 0x18 for lazy sprites. The `ct` argument is a `PCX_Color_Table*` (NULL = the sprite's own palette); its `+4` (`JGL_Color_Table*`) is what gets passed down, and this is how civ colours and button palettes are applied [X].

| Wrapper | Signature (`this` = Sprite*) | JGLSprite slot | Callers / use |
|---|---|---|---|
| **`Sprite_draw`** 0x5F8210 | `(PCX_Image* canvas, int x, int y, PCX_Color_Table* ct)`, `ret 0x10` | +0x44 (jgl 0x10008180) | 441 callers; normal colour-keyed (index 0xFF) blit [X key; I exact 254 handling] |
| `Sprite_draw_on_map` 0x5F8060 | `(Map_Renderer*, x, y, ct, ?, scale_num, scale_den)`, `ret 0x1C` | +0x44 with scale | map only (section 4) |
| 0x5F81A0 | `(canvas, x, y, ct)`, `ret 0x10` | +0x5C | 26, city screen 0x408xxx-0x412xxx, 0x4F8997, 0x523Cxx [I: variant blit mode] |
| 0x5F84A0 | `(canvas, x, y, int a4, ct)`, `ret 0x14` | +0x74 | 16 (city screen, 0x52Axxx, 0x5A39xx) [I] |
| 0x5F83E0 | scaled wrapper of 0x5F84A0, `ret 0x20` | | 2 |
| 0x5F8620 | `(canvas, x, y, void* blend_lut, ct)`, `ret 0x14` | +0x7C | per pixel: if src != 0xFF then `dst = lut[(src16<<16)|dst16]` (16-bit path 0x1000A740) [X]. Translucent overlays |
| **0x5F8560** | scaled wrapper of 0x5F8620 `(…, num_x, num_y, den)`, `ret 0x20` | | 5 (0x4C47xx fog, 0x4E55D0 city HUD) |
| 0x5F8760 | `(canvas, x, y, int a4, ct)`, `ret 0x14` | +0x84 | 25, city screen 0x411xxx-0x414xxx, 0x424C05, 0x5A855B [I] |
| 0x5F86A0 | scaled wrapper of 0x5F8760 | | 1 (0x4C4F05) |
| **`Sprite_draw_with_color_table`** 0x5F87E0 | `(canvas, PCX_Image* canvas2, x, y, int a5, ct)`, `ret 0x18` | +0x8C | 10 (units/leaders) |
| 0x5F8870 | scaled two-surface variant, `ret 0x24` | +0x88 | 7 |
| 0x5F89A0 | `(canvas, x, y, ct?, a5, a6)`, `ret 0x18` | +0x94 | 18 (0x4F7Fxx-0x4F88xx, 0x5896xx, 0x5CFxxx) [I] |
| **0x5F8310** (alpha-mask blit) | `(Sprite* alpha_mask, PCX_Image* background, PCX_Image* dest, x, y, ct)`, `ret 0x18` | +0x50 | **no crop offset added**. Buttons with an alpha image: `Images[state].0x5F8310(Button.Images[3], tmp_bg, button_canvas, (W-w)/2, (H-h)/2, 0)` [X: 0x60A252]. Also Main_GUI 0x5536FD..0x558209 |
| 0x5F8380 | `(PCX_Image* a1, PCX_Image* a2, x, y, ct)`, `ret 0x14` | +0x58 | no crop offset; Main_GUI 0x54F2xx/0x552Bxx [I: two-surface mode] |

JGLSprite vtable (jgl RVA 0x68440), useful as a catch-all hook (every exe wrapper ends here, with x/y already offset): +0x44 0x10008180, +0x50 0x10009AA0, +0x58 0x10009B20, +0x5C 0x10008050, +0x74 0x10008600, +0x7C 0x10008EE0, +0x84 0x10008FC0, +0x88 0x100090E0, +0x8C 0x100090A0, +0x94 0x10009220 [X]. The first stack arg is always the destination `JGL_Image*`, and `JGL_Image+0x80` gives back its `PCX_Image*` [X]. Each slot dispatches on (sprite bpp, dest bpp) to specialised loops. Only +0x7C was decoded; the exact pixel maths of the other modes is [?].

**PCX_Image -> PCX_Image copies** (whole backgrounds, the minimap, parent backgrounds, temporaries) [X]:

| Function | Signature (`this` = source PCX_Image) | JGL_Image slot |
|---|---|---|
| **`PCX_Image_draw_onto`** 0x5FC9F0 | `(PCX_Image* dst, x, y)`, `ret 0xC` | +0x84 (jgl 0x10002410): wraps the source in a temporary JGLSprite and runs the same blit as Sprite_draw, so it is colour-keyed |
| copy rect 0x5FCB30 | `(dst, src_x, src_y, dst_x, dst_y, w, h)`, `ret 0x1C` | -> 0x5FCC10 |
| copy/stretch 0x5FCC10 | `(dst, sx, sy, sw, sh, dx, dy, dw, dh)`, `ret 0x24` | +0x40 `Copy(dst, RECT* src, RECT* dst)` (opaque) |
| copy same rect 0x5FCB60 | `(dst, x, y, w, h)`, `ret 0x14` | +0x40 with src = dst rect |
| 0x5FCBD0 | `(dst, RECT*)` | +0x40 same rect |
| 0x5FCCB0 | `(dst, RECT* src, RECT* dst)` | +0x40 |
| 0x5FCA30 / 0x5FCA70 / 0x5FCAB0 / 0x5FCAF0 | `(dst, x, y, a4)` | +0x88 / +0x90 / +0xA4 / +0xA0: **not implemented** in this jgl (`mov eax,0x18; ret 0x10`, 0x1003BE40), so they draw nothing [X] |

### 16.5 Forms, canvases and composition to the screen

Form offsets, counted from the start of `Base_Form` (including the vtable; header `Base_Form_Data` offset + 4):

| Off | Field | Use |
|---|---|---|
| +0x9C | Status1 | 0x20 coordinates relative to parent; 0x8000 see below; 0x10/0x01 has frame decorations (m80); 0x80000 erase background by copying the parent canvas; 0x1000000 transparent composite; 0x40000000 second canvas at +0x568 [X arithmetic, I names] |
| +0xA0 | Status2 | bit0 visible (`0x609AC0` = visible and all parents visible); bit1 framed/scrollable [X bit0] |
| +0xA4/+0xA8/+0xAC | composite params | only used when Status1 & 0x1000000 [X] |
| +0xB0 | surface owner | the form whose canvas is composited for this entry (normally `this`) [X use, I meaning] |
| +0x130 | Parent | [X] |
| +0x1AC..+0x1B8 | Left, Top, Right, Bottom | canvas size = (Right-Left, Bottom-Top) [X] |
| +0x1BC/+0x1C0 | client offset x/y (rect +0x1BC..+0x1C8) | [X] |
| +0x1DC | RECT clip, in **screen** coords | computed by 0x607AC0 (IntersectRect down the parent chain) [X] |
| +0x224 / +0x228 / +0x22C | child `Base_Form*[]`, capacity, count | [X] |
| **+0x274** | **Canvas** (`PCX_Image`), +0x278 its `JGL_Image*` | [X]. MSF canvas = 0x9F8934; Main_GUI (HUD, `Main_Screen_Form+0x5450` = **0x9FDB10**) canvas = 0x9FDD84 |

Every form and control (buttons included: `Button` vtable 0x6656C0, draw 0x60A010) has **its own canvas**. Controls draw into their own canvas, not into the parent's [X]. Relevant Base_Form vtable slots (shared implementations) [X]: +0x0C `m03` 0x602340 creates the canvas (`PCX_Image_create(w, h, …)` into +0x274); +0x58 `m22_Draw` (per class: draws into the canvas); +0x124 `m73` 0x6026B0 = `m22_Draw` + `0x602540` (invalidate the form's screen rect via graphics vt+0x30); **+0x13C `m79` 0x602710 = composite the canvas onto the back buffer**; +0x140 `m80` 0x608390 = draw frame/scroll images directly onto the back buffer; +0x16C 0x6028D0 = re-create the canvas on resize.

**Local -> screen** [X]: `0x604A00 Base_Form_local_to_screen(this, int* x, int* y)`, `ret 8`:
```
x += Left + client_x; y += Top + client_y                     // +0x1AC/+0x1BC, +0x1B0/+0x1C0
if (Status1 & 0x20) && Parent:
    Parent->local_to_screen(x, y)                              // recursion
    if (Status1 & 0x8000): x -= Parent.Left; y -= Parent.Top
```
`0x604A90` is the inverse. The screen position of canvas pixel (0, 0) is `local_to_screen(0, 0)`.

**Composite of one form, `m79` 0x602710 `(this, RECT* dirty)`** [X]:
```
set scale 1/1/1; dst = 0xCACFF0 (back buffer)      // 0xCAD30C override: never written in .text, ignore
dst.set_clip(this+0x1DC)                            // 0x5FC990 -> JGL vt+0x34
(ox, oy) = local_to_screen(0, 0)
if Status1 & 0x1000000:  canvas.PCX_Image_draw_onto(dst, ox, oy)   // colour-keyed; A4/A8/AC variants are no-ops
else:                    canvas.copy_rect(dst, 0, 0, ox, oy, Right-Left, Bottom-Top)   // opaque, 0x5FCB30
dst.reset_clip()                                    // JGL vt+0xC4
```
So **a draw at (x, y) on canvas C (form F) lands at screen `(x + ox, y + oy)`**, clipped to `F+0x1DC` and to the paint rect. Exception: if C is not a form canvas, it is a temporary or offscreen image whose content reaches a canvas through a later PCX->PCX copy (16.4).

**Paint pass** [X]:
1. Windows sends WM_PAINT to the jgl window. jgl (0x1003CC7E) calls `BeginPaint`, then `controller->vt[1](&ps.rcPaint)`. The controller is 0xCACD30 (vtable 0x67138C, set at 0x603220), and `vt[1]` = **0x62C3F0 `paint(RECT* r)`** = `compose(r, 0)` (0x609210) followed by `present(r)` (0x606680).
2. **`compose` 0x609210 `cdecl (RECT* r, Base_Form* from)`**: walks the draw list `Base_Form* 0xCAC2F8[count 0xCAD3C0]` **from the last index down to 0**. For each visible entry `e` it calls `e->m80(r, -1)` if `e.Status2 & 2`, then `e->[+0xB0]->m79(r)`. **The last index is the bottom layer and index 0 is the top layer.** Afterwards, if a cursor sprite is set (0xCAD2D0), it saves the background under the cursor (0xCACD38) from the back buffer.
3. The draw list is rebuilt by 0x607DE0 from the root list `0xCACB10[count 0xCAD3C4]` (roots in order; root 0 ends up topmost). Per root, 0x607CC0 appends in post-order: non-relative children (recursively), then relative (0x20) children if `this->B0 == this`, then the form itself. A parent is therefore drawn before (under) its children. If a modal form is set (0xCAD310), the list restarts at it [X list logic, I "modal"].
4. **`present` 0x606680 `(RECT* r)`**: draws the tooltip (if text 0xCAD2F4 is set: outline 0x600940 with colour 0x72FF38, `fill_area(0xCACAFC, 0xCAD308)`, font 0xCAD2F8, text split on `^` drawn by 0x5FEFC0/0x5FF0D0, all straight into 0xCACFF0). It then draws the mouse cursor `Sprite_draw(0xCAD2D0, 0xCACFF0, *0xCAD2D4, *0xCAD2D8, 0)` and calls graphics vt+0xA4(r), which BitBlts to the window.

Forms redraw into their canvas only when invalidated (m73 -> m22), not every frame. **Canvases are retained surfaces**: the tracer must keep a per-canvas display list, reset it when the canvas is re-created (0x5FC5F0 / 0x5FC690 / read_file into it) or fully covered (fill of the whole canvas, opaque copy of the full rect, background sprite at (0,0) of canvas size). "Erase background" (0x602120): if Status1 & 0x80000, `parent_canvas.copy_rect(own_canvas, Left+cx, Top+cy, 0, 0, W, H)`; otherwise `fill(color)` (0x5FFF30). Translucent and transparent controls are built from this copy of the parent's pixels.

### 16.6 Other UI drawing

| What | Function | Signature (`this` = PCX_Image) | JGL_Image slot |
|---|---|---|---|
| fill whole image | 0x5FFF30 | `(color)` | +0x44(NULL, color) [X] |
| `PCX_Image_fill_area` | 0x5FFF50 | `(RECT*, color)` | +0x44 [X] |
| fill x,y,w,h | 0x5FFEF0 | `(x, y, w, h, color)` -> 0x5FFF50 [X] |
| line | 0x5FFFB0 | `(x1, y1, x2, y2, color)` | +0x64(x1, y1, x2, y2, color, 1) [X] |
| horizontal line | 0x5FFFE0 | `(x1, x2, y, color)` | +0x64(x1, y, x2, y, color, 1) [X] |
| vertical line | 0x600010 | `(x, y1, y2, color)` | +0x64 [X] |
| outline rect | 0x6008D0 | `(left, top, right, bottom, color, ?)` built from the h/v lines [X] |
| outline RECT* | 0x600940 | `(RECT*, color, ?)` -> 0x6008D0 with right-1/bottom-1 [X] |
| special frame | 0x600980 | 5 args, uses 0x600040/0x6000A0 (exe-side 8/16-bit loops 0x62B720/0x62B9D0, 0x62B860/0x62BBA0) [I: dotted/xor selection lines] |
| rect frame (debug) | 0x5FCCF0 | `(l, t, r, b, color)` | +0x5C(&rect, color) [I] |
| set palette | 0x5FFDF0 | `(PCX_Color_Table*)` | +0xEC (canvases get the global 0xCAAE2C) [X] |
| text | `PCX_Image_draw_text` 0x5FEE50 `(str, x, y, len)`; `do_draw_centered_text` 0x5FEFC0 `(str, x, y, width, len)`; `draw_centered_text` 0x5FD630 `(font, str, x, y, width, len)`; 0x5FF0D0 `(str, RECT*, len)`; state: `set_font` 0x5FD200, `set_text_effects` 0x5FD240 `(color, shadow, dx, dy)` [H names, X addresses]. These end in jgl `TextOutA` on the canvas's DC [I: DC owned by the JGL_Image, +0x4BC]. To attribute TextOutA calls to a canvas, record ECX on entry of these functions |
| per-pixel | JGL_Image vt+0x1C / +0x14 (pixel pointer) | used by the minimap builder [I] |

**Minimap**: `Navigator_Data` lives at **Main_GUI+0x10720 = 0xA0E230** [X: passed as `this` to 0x578050/0x5786F0/0x578E00]. Its PCX_Images (+0x18, +0x2D0, +0x588, +0x858 [H]) are filled per pixel from world state, then copied into the Main_GUI canvas with 0x5FCBD0/0x5FCB60 (calls at 0x5784EC, 0x578625, 0x578733, 0x57886C) [X calls, I content]. The renderer has to generate the minimap itself. Only the frame and buttons come from art files (Main_GUI `Images[252]` at +0x140D8, file names `PCX_Files[25]` at +0x2F0C [H]).

**Not found**: no gradient fill primitive in the exe [?]. Fog/translucency in the UI goes through the LUT blend (+0x7C).

### 16.7 Hook plan

All hooks are thiscall. Read args at entry and results at return.

1. **Image origin**
   - `0x5FC700` read_file: entry `img = ECX`, `path = strdup([esp+4])`, `flags = [esp+0x14]`. On return `EAX == 0`: `img_src[img] = {file: path, w: JGL(img+4).vt[0xD8](), h: vt[0xDC]()}`.
   - `0x5FC5F0` create, `0x5FC690` resize: `img_src[img] = {generated, w, h, bpp=[esp+0xC]}`.
   - `0x5FC4D0` clear: `img_src.remove(img)`.
2. **Sprite origin**
   - `0x5F7EC0` slice: entry `spr = ECX`, `img, x, y, w, h, type, crop = [esp+4..0x1C]`. On return, if `spr+4 != 0`: `spr_src[spr] = {src: img_src[img] (or surface id img), rect: (x + spr[0x20], y + spr[0x24], spr[0x10], spr[0x14]), cell: (x, y, spr[0x18], spr[0x1C])}`. The odd-width decrement means `spr[0x18]` can be `w-1`.
   - `0x5F7F30` (same, 10 args).
   - `0x5F8A20` copy: `spr_src[ECX] = spr_src[[esp+4]]`.
   - `0x5F8AA0` / `0x5F8B10`: generated.
   - `0x5F7E20`: `{file: [esp+4], whole image}`.
   - `0x5F7DB0`: remove.
   - Alternatively key by `JGLSprite*` (`spr+4`) and hook at JGL level.
3. **Every UI blit**: hook the wrappers in 16.4 (0x5F8210, 0x5F81A0, 0x5F84A0, 0x5F8620, 0x5F8760, 0x5F87E0, 0x5F89A0, 0x5F8310, 0x5F8380, plus the scaled 0x5F8060/0x5F83E0/0x5F8560/0x5F86A0/0x5F8870). Record `{op, spr, spr_src[spr], dst_canvas, x + spr[0x20], y + spr[0x24], extra args, ct}`. Alternatively hook the ten JGLSprite slots in jgl (vtable RVA 0x68440, read-only page: patch the vtable entries or detour the functions). That catches everything with the final x/y; map `JGLSprite* -> Sprite*` from step 2 and `dst JGL_Image* -> PCX_Image*` via `+0x80`. Also hook the PCX->PCX copies (0x5FC9F0, 0x5FCB30/0x5FCC10, 0x5FCB60, 0x5FCBD0, 0x5FCCB0), the primitives and text in 16.6.
4. **Canvas -> screen**
   - Hook `0x602710` (m79) entry: `form = ECX`, `canvas = form+0x274`, `(ox, oy)` = call `0x604A00(form, &x0, &y0)` with `x0 = y0 = 0` (pure, safe to call from the hook), `clip = *(RECT*)(form+0x1DC)`, `size = (form[0x1B4]-form[0x1AC], form[0x1B8]-form[0x1B0])`, `mode = form[0x9C] & 0x1000000 ? keyed : opaque`, plus `form vtable = *(void**)form` to identify the class.
   - Bracket paint passes with `0x62C3F0` (arg = dirty RECT* in screen coords): composite events in call order are back-to-front.
   - The cursor/tooltip pass is `0x606680`.
   - Draws whose destination is 0xCACFF0 are already in screen coordinates (m80 frame pieces, tooltip, cursor).
5. **Reconstruction**: keep a retained display list per canvas (16.5 reset rules), resolve temporaries through the copy events, and on each paint pass emit `for each m79 event: canvas list translated by (ox, oy), clipped to clip ∩ dirty`. For a full-screen mirror, hook `0x609210` and re-run the composition over all forms.

Open: the exact pixel semantics of JGLSprite slots +0x50/+0x58/+0x5C/+0x74/+0x84/+0x88/+0x8C/+0x94 (need tracing of jgl's inner loops, e.g. 0x1000A1D0 for +0x44 8->16 bit); the role of `+0xB0` when it is not `this`; the frame images drawn by m80 (form+0x11C..+0x128, [I] PCX_Image* edge/scroll pieces).

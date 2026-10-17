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
| **[C]** | Read from C3X's source (`injected_code.c`, section 18), not compared with the game's output yet. |
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
- **Two zoom levels only**: `bic.is_zoomed_out == 0` -> lattice step 64 x 32 (box 128x64); `!= 0` -> step **32 x 16** (box 64x32) [X: `(0x80 or 0x40)>>1` per axis]. Every sprite draw receives a scale argument 1 or 2 (`Sprite_draw_on_map(spr, renderer, x, y, 1, 1, zoom?2:1, 0)`) and all per-layer pixel offsets are halved in zoomed-out mode (e.g. mountain -24 -> -12) [X]; so zoomed-out = the same art sampled at every second pixel, from each sprite's own *box* (below and §3.1).
- **Zoomed-out resampling, verified [X][D]** (replaces the [I] above; the first version of this paragraph took the sampling from the cell's corner, which is wrong, see §3.1). `Sprite_draw_on_map` (0x5F8060) receives `(canvas, x, y, num_x = 1, num_y = 1, den = zoomed ? 2 : 1, color_table = 0)`. It saves the JGL global scale (JGL object at `[0xCAB28C]`, vtable +0xA0 get, +0x9C set; jgl.dll stores it in its globals 0x1006C0FC/0x1006C100/0x1006C104 = num_x/num_y/den, set by 0x1003C0E0), sets `num/den`, adds the sprite's margins scaled (`x += Sprite+0x20 * num_x / den`, `y += Sprite+0x24 * num_y / den`, C division, i.e. rounded toward zero; the margins are the sprite's crop offset, §3.1, not 0), blits through the image's vtable +0x44, and restores the old scale. In jgl.dll (`Conquests/jgl.dll`, 2004-03-17) the blit dispatcher (0x10008180) picks a routine by source/destination depth and by image flag `+0x18 & 1`; the 8-bit indexed source to 16-bit canvas routine is 0x1000ADA0 (0x10034400 when the flag is set, for span-compiled images, which the map's sprites are not; 0x100141E0 is the same routine with the blend table, 0x10015C90 the colour-table draw of the borders: all of them step as below). The scaled path (taken when `|num_x| != den` or `|num_y| != den`, 0x1000B24B) is **nearest neighbour, no filtering**, and works on the *sprite's image*, the cell cropped to its box (§3.1):
  - destination rectangle = `(x', y')` to `(x' + ftol(W * num/den), y' + ftol(H * num/den))` with `ftol` truncating (the float scale 0.5 times `W`, `H` of the **image**, 0x1000B24B..0x1000B2B2; the two extra `ftol(scale)` terms added to `x'`, `y'` are 0), i.e. **`floor(W/2) x floor(H/2)`**; it is clipped by the canvas' clip rectangle (`IntersectRect`);
  - source step per destination pixel = `(den << 16) / num` in 16.16 fixed point = exactly 2.0 in both axes (0x1000B33B..0x1000B367); the source start is `(clip_dx * step, clip_dy * step)` where `clip_d*` is how far the clip moved the destination start (0 when the sprite is not clipped; 0x1000B388..0x1000B3C6);
  - so **destination pixel (i, j) = pixel (2i, 2j) of the sprite's image**, wherever the sprite lands on the canvas: a clipped sprite still samples even image coordinates. In *cell* coordinates that is `(margin_x + 2i, margin_y + 2j)` drawn at `(x + margin_x / 2 + i, y + margin_y / 2 + j)` (§3.1), so sprites whose margin is odd sample the odd columns/rows of their cell;
  - image indices >= 254 (`cmp byte [esi], 0xFE; jae skip`) are not written (both 254 and 255 transparent); others go through the 16-bit colour table. (0x10015C90 skips only 255 and always starts at the image's first pixel: a border sprite cut by the top or left edge of the canvas shows its own first rows and columns there instead of the ones that belong to the visible part [X].)
  The shadow/blend blit 0x5F8560 (airfield/radar/outpost/resource shadows) and the fog blit 0x5F86A0 take the same scale arguments and the same margin scaling.
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

### 3.1 How a sprite is stored and drawn at half size [X][D]

*Slicing.* Every sprite of the map renderer's arrays (`Std_Terrain_Images` ... `Fog_Of_War_Images`, cities, resources, borders) and of C3X's art (district slices, natural wonders: `injected_code.c`, `Sprite_slice_pcx(..., 1, 1)`) is made by `Sprite_slice_pcx` (0x5F7EC0 -> 0x62B170) as `(sprite, pcx, x, y, w, h, 1, 1)`: all 67 call sites of the map renderer's loaders (0x4C5AE6..0x4C7A27, each one a loop over an array) pass 1, 1 as the last two arguments [X] (the interface's own slices at 0x54xxxx pass 0 as the last one). Two things happen before the cell becomes a sprite:

1. 0x5F7EC0 rounds `w` **down to an even number** if it is odd and above 1 (0x5F7EFF..0x5F7F08). The 49-wide resource cells are therefore 48-wide sprites (traces: `Resources 48 49`, `ResourcesShadows 48 49`); the description's cells are 48 wide with 2 pixels between them.
2. 0x62B170 **crops the cell to the box of its pixels that are not palette index 255**: it counts the leading rows and the trailing rows that hold only 255 (`Sprite+0x24` = the top margin, `+0x14` = the rows kept), then the smallest run of leading and of trailing 255 over the rows kept (`+0x20` = the left margin, `+0x10` = the width kept: 0x62B3D9..0x62B491), and copies that rectangle (0x62B4D4..) into the sprite's own 8-bit image (`Sprite+4`). The cell's size stays in `+0x18/+0x1C` (the traces' sizes), the margins in `+0x20/+0x24` (C3X's header calls them `Width2/Height2`). **Only 255 counts as blank**: 254, the sheet filler that every blit skips as well, stays inside the box, so cells whose corners are 254 are not cropped (roads, rivers, forests, fog, ...). A cell of nothing but 255 makes no sprite (error 9). The one file whose pixels the game changes first is `Territory.pcx`: its loader (0x4C6A9A..0x4C6B0C) calls `0x5FFDB0(pcx, 0, 0xFF)` and `0x5FFDB0(pcx, 1, 0xFF)` (jgl 0x10002070 -> 0x10004FD0 replace one index by another over the image) before it slices, so 0 and 1 (the diamond's filler) are blank there and the boxes shrink to the quadrants that hold the border art.

*Drawing at zoom 1.* `Sprite_draw_on_map` draws the cropped image at `(x + margin_x, y + margin_y)` (0x5F811E..0x5F815D with `num = den = 1`): the same pixels as the whole cell at `(x, y)`. The recorded draw positions of the tracer are the `(x, y)` before the margins.

*Drawing at half size.* The margins are scaled like every offset of the call, `x' = x + margin_x * num / den`, `y' = y + margin_y * num / den` with C division and `den = 2` (**rounded down**), and the cropped image is blitted with scale (1, 1, 2): destination rectangle `floor(box_w / 2) x floor(box_h / 2)` at `(x', y')`, destination pixel `(i, j)` = pixel `(2i, 2j)` of the cropped image (§3, resampling). In cell coordinates:

```
cell pixel (margin_x + 2i, margin_y + 2j)  is drawn at  (x + floor(margin_x / 2) + i, y + floor(margin_y / 2) + j)
for i < floor(box_w / 2), j < floor(box_h / 2)
```

So the **sampling phase is `margin mod 2` in each axis** (the same for every position on the screen: it depends on the sprite, not on where it is drawn), the sprite starts at its margin divided by two, and the last column (row) of a box of odd width (height) is dropped. Examples: all 729 cells of the nine terrain files have margin (1, 0) and box 126 x 64 (the landmark files an odd left and an even top margin): the sprite is drawn at `x + 0`, columns 1, 3, ..., 125 of every second row, 63 x 32 pixels. The polar caps and the territory cells (margins (1, 8), (1, 3), (61, 8), ..., quadrant boxes) have odd left margins; 83 of the 144 mountain and volcano cells have an odd top margin, 46 of 64 hill cells, 61 of 80 city cells and 28 of 44 resource cells an odd margin in one axis or both. Roads, rails, rivers, flood plains, irrigation, forests, marsh, jungle, fog and the like are not cropped (254 fills their corners). In all, 1732 of the 2823 non-empty cells of the description's map sheets have an odd margin (1596 in x, 194 in y). Taking the cell's own corner as the origin (every second pixel from the cell's top-left) is right only where both margins are even, i.e. for fewer than half of the cells; it shifts the terrain by one pixel in x. The destination is clipped by the canvas and the source start moves by twice the clip offset, so the phase never depends on the screen position either (exception: the border blit, §3 above).

*Where it comes from [D].* In the zoomed-out trace `N03-zoomout-nodistricts` (camera (32, 20), 2205 terrain draws, 1920x1080 screenshot) the terrain pixels that no other traced draw covers match the game exactly with source columns `2i + 1` and rows `2j` at the traced position for each of the 48 draws that are not under fog or other art, and with no other phase or one-pixel shift. Of the 1332 terrain draws with enough uncovered pixels, 24 match the screenshot to 90% or better under the box rule (8 completely) and none under the cell-corner rule (the others lie under fog, labels, units or other art). The phases measured earlier on the districts' art (`A03-zoomout-phase.txt`: the 128x88 natural wonder cell (2, 0) and the ski resort sample odd rows, the others even rows) are exactly the parities of those cells' top margins (5 and 13; 24, 24 and 0 for the others). `civ3-tool pixeldiff` on the trace: 45.7% of the frame identical before, 98.0% after (terrain: 40.5% -> 99.8%), see §15.

*In our renderer.* The art is held once, at full size, for both zooms. Compiling the description finds each sprite's box (its margins and size, from the load `blank_loader` describes) and stores it with the sprite; the game's view zoomed out is a view with `downscale = 2`, whose shaders place each sprite at `anchor / 2 + margin / 2` (C division) and fetch texel `(margin_x + 2i, margin_y + 2j)` of the full-size cell for destination pixel `(i, j)`, `i < floor(box_w / 2)`, `j < floor(box_h / 2)`: the rule above. The lattice steps, the layers' offsets and the world sprites' positions are halved by the same view (rounded toward zero), `scaling` keeps the anchors of the sheets the game does not scale (forests, districts) and the size of the HUD, and the team disc's second cell (`scaled_origin`) is a twin the view draws instead. Toggling the zoom changes only the view.

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
  - **Top and bottom map edge (verified) [X][D].** `m21` (0x4C3230) wraps every lattice cell and skips it when the wrapped x is outside `[0, Width)` or the wrapped y outside `[0, Height)` (0x4C33BE..0x4C33E0), before it calls `m19` (0x4C3768, with the *wrapped* coordinates). The loop range starts at `TileY_Min = -1`, but on a map without Y wrap every cell of row -1 is skipped, and so is row `Height`. So the vertex row `vy = -1` (the top corners of tile row 0) and the row `vy = Height` get **no terrain, river or flood plain**. Vertex row 0 is drawn normally (terrain from tile `(vx, 1)`). The ice cap of vertex `(vx, 0)` lands at `(px-64, py-32)`, which is exactly the box of the skipped vertex `(vx-1, -1)`; at the bottom the cap of `(vx, Height-1)` lands at `(px-64, py+32)` = the box of `(vx-1, Height)`. With `Map.Flags & 4` the caps fill the missing top/bottom corners; without it nothing is drawn there. The cap index has no special meaning: one LCG step from `Seed + a*21031` changes slowly with `a`, so neighbouring vertices give runs (sp3, top row from the left: 1 2 2 3 4 4 5 6 6 7 0 0 1 1 2 3 3). Check (`/tmp/top.py`): the 32 row -1 vertices with a tile below them inside the draw rectangle (sp3 16, t4 16) get no terrain from the game; no trace has a terrain draw outside `0 <= vy < Height` on a map without Y wrap; polar caps 132/132 on index and position (all non-zoomed traces with a seed). Whether the fog pass (0x4C4F10) also skips row -1 was not checked (fog draws are unnamed in the traces).
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
- **Correction (verified) [X][D]: the edge hash uses the wrapped neighbour.** In m10 (0x5F4B87..0x5F4CF0), m11 irrigation (0x5F610C..0x5F6243) and m17 borders (0x5F5204..0x5F5285) the neighbour of diagonal k = 0..3 is `(x + 2*(k&1) - 1, y + 2*(k>>1) - 1)` (NW, NE, SW, SE), passed through `Map_wrap_horiz/vert` and skipped if off the map. The edge midpoint is then `mx = (x + nx) / 2`, `my = (y + ny) / 2` (C division, toward zero) with **(nx, ny) the wrapped neighbour** and (x, y) the tile as `m19` passes it (already wrapped):
  ```
  for k in 0..4:
      (nx, ny) = wrap(x + 2*(k&1) - 1, y + 2*(k>>1) - 1); skip if off the map
      if neighbour effective type (Tile vt+0xC8) not in {6, 5, 10}: continue
      mx = (x + nx) / 2 ; my = (y + ny) / 2            // cdq; sub; sar 1
      keep = k in {0,3} ? (3mx - my) % 5 != 0 && (mx - 2my) % 5 != 0
                        : (3mx - my) % 7 != 0 && (mx + 2my) % 5 != 0 && (7mx + my) % 11 != 0   // idiv, signed remainder
      if keep: mask |= 1 << k
  ```
  Away from the wrap seams this is the formula above; on a seam it is not. Example cr1 (60x60, wraps in x and y): tile (32, 0), NE neighbour (33, -1) wraps to (33, 59), so `mx = 32`, `my = 29`, `(mx + 2my) % 5 = 90 % 5 = 0`: the NE link is dropped and the game draws `Mountains_Images/13`; with the unwrapped neighbour (`my = 0`) the link is kept (15). The same applies to irrigation masks and to the border `hilly` bit. Check (`/tmp/mtn.py`): 6330/6330 mountain/hill/volcano masks in all traces with a seed (unwrapped rule: 6329, the cr1 case is the only seam occurrence); irrigation 2459/2459 under both rules (no seam case in the traces).

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
- **Correction (verified) [X][D]: variant and pass, exact** (`m15` 0x5F5300, sprite via slot 51 0x4C3D10 = `Delta_Rivers_Images[mask*2 + variant]`). `m15` gets the wrapped vertex `(vx, vy)` from `m19`. N = `(vx, vy-1)`, W = `(vx-1, vy)`, E = `(vx+1, vy)`, S = `(vx, vy+1)`, each wrapped again and skipped if off the map. "Real" = Tile vt+0xC4 (`SquareType` bits 8-11), "effective" = vt+0xC8 (bits 12-15), `River_Code` = vt+0x94 (Tile+0x04).
  ```
  a       = ((Map.Width(+0x168) >> 1) * (vy - 1) + ((u16)vx >> 1)) & 0xFFFF   // vy wrapped first, vy-1 not re-wrapped
  state   = Map.Seed(+0x1EC) + a * 0x845FED ; 4 advances
  variant = rand_int(2)                                    // always consumed (0x5F538D)
  mask, fp as in the table above; fp bit 1 never comes from W (0x5F56C3 ORs into W's flood-plain flag instead)
  coast   = real(W) == 11 || real(E) == 11 || real(S) == 11  // N is not tested (0x5F5874, 0x5F5B10, 0x5F5DDE)
  if fp != 0:  (flags & 8: Flood_Plains_Images[fp]) ; variant = 0
  if popcount(mask) == 1: variant = coast ? 0 : 1          // 0x5DF830, 0x5F5E4F
  special = any of N, W, E, S has: CityID != -1, Tile_has_colony, airfield, radar, outpost, fortress,
            barricade, goody hut, barbarian camp (Overlays(civ 0)), or ruins (Tile+0x24 != 0)
         || effective(S) in {6, 10}
         || effective(W) in {6, 10} && W.River_Code & 0x02
         || effective(E) in {6, 10} && E.River_Code & 0x80
  if mask != 0 (or Terrain_Overlays bit 27 of N / 26 of W / 25 of E / 24 of S):
      draw at (px, py) in pass 2 (flags & 4) if special, else in pass 4 (flags & 2)
  ```
  So the odd (`mtnRivers`) art has nothing to do with mountains: it is the second random variant or the spring art away from the coast. Three differences from the text above: (1) the spring coast test ignores N, so a spring whose only coast neighbour is the tile above uses `mtnRivers`; (2) the seed row is the *wrapped* `vy` minus one, which matters on Y-wrapping maps for the row drawn at `vy = -1` on screen (it is `vy = Height-1`, cr1 at (448,-4)); (3) ruins and colonies also make a river "special". Check (`/tmp/river.py`, `/tmp/river2.py`, `/tmp/rpass.py`): index 4452/4452 river draws in all traces with a seed; 781 of them are springs, 7 of those have coast only in N and all 7 use the odd art (bc1 1, bc3 2, bd1 1, bd3 1, cr1 1, ww5 1). Pass 4299/4299 in the 59 traces that record `Tile_BuildingID`; the condition without ruins/colonies gets 4295 (the 4 misses are sp5 rivers next to ruins). The old t4/t5 traces (no ruins/building fields, an older recorder) list all rivers before the pass-3 objects and were left out of the pass check.

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
- **Correction (verified) [X][D]: road index and stub suppression, exact** (`m16` 0x5F66B0 -> slot 52 `Roads_Images[i]`). `civ` = the viewer (m19 arg 1).
  ```
  road(t, c) = Tile vt+0x64 (0x5D9F20):
      if t has a city (vt+0x114 owner) or a colony (vt+0x118 owner) with owner o != -1
         and !Leader_has_tech(leaders[o], WorkerJobs[3].RequireID): false
         // BIC+0x3E1C WorkerJobs (stride 0x74), +0x1A4 = job 3 Build_Road .RequireID; 0x5613F0: -1 = known
      else Get_Overlays(c) (vt+0xA8, m42) & 0x01
  rail(t, c) = Tile vt+0x5C (0x5DA000): the same with WorkerJobs[4].RequireID (+0x218, Build_Railroad), bit 0x02
  if !road(T, civ): draw nothing
  mask = 0
  for k in 1..8:                                           // neighbor_index_to_diff 0x5E6D80, wrapped, off-map skipped
      n = neighbour k
      if road(n, civ) && !(rail(n, civ) && rail(T, civ)): mask |= 1 << (k-1)
  if mask == 0 && (CityID(T) != -1 || rail(T, 0)): draw nothing   // civ 0: the ACTUAL Tile.Overlays
  draw Roads_Images[mask] at (px, py)
  ```
  The stub test reads the tile's actual railroad bit, not the viewer's memory. On a fogged tile where the viewer still remembers road + railroad but the railroad is gone from `Tile.Overlays`, every road link is suppressed as a rail-rail link, the stub test finds no actual railroad, and the game draws `Roads_Images/0` under the remembered `Railroads_Images[...]`. In sp1/sp2 these tiles now hold only pollution (`Overlays` 0x40, remembered byte 0x0B or 0x07); in cr1 (7,59) `Overlays` = 0x05, remembered 0x07. Check (`/tmp/roads.py`, predicted multiset of (index, x, y) over every tile of the draw rectangle against the trace): 6631/6631 road draws, 0 extra, in the 59 traces with a seed and `Tile_BuildingID`; 23 of them are stubs, 19 of which are this case (cr1 1, sp1 3, sp2 5, t0 10). The rule "no stub when the viewer sees a railroad" gets 6612 and misses exactly those 19. The traces carry no tech data, so the city/colony tech gate is untested (it can only matter on city/colony tiles).

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
8. **HUD**: `Unit_draw_hud(canvas2, x - 32, y - 32, stack_flag)` (0x5BA460; -16/-16 zoomed out, with the pieces at their full size: the health bar is 2 px wide there as well, the LED 6x6 [D: `N03-zoomout-nodistricts`, pixel exact]). `stack_flag` = 1 when called from the animator and the unit is neither moving (`U+0x5D`) nor always-on-top.

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
- **Team-colour disc** (only with `P_SHOW_TEAM_COLOR_DISC`, not on city tiles; drawn before the cursor and the unit): sprite 0xB3F2BC = `Art\Cursor.pcx` cell (259, 155, 128x64), a 1-px green (index 247) ellipse ring. Zoomed out it is sprite 0xB3F290, cell (259, 90, 128x64) (0x54B048..0x54B088), a **filled** ellipse (848 pixels of index 247 in a 48x23 box against the ring's 308 in 64x28), both cut with the box rule (§3.1). It is drawn with 0x5F83E0 `(sprite, canvas, x, y, colour, num_x, num_y, den, 0)` at `(x - 64, y - 32)` with scale (1, 1, 1) (the tile box; zoomed out scale (1, 1, 2) at `(x - (W >> 2), y - (H >> 2))` = `(x - 32, y - 16)`, 0x5CBF57..0x5CBFAD) in a single colour = **`ntpNN` palette entry 66** of the owner's `Color_Table_ID` (`Leader` 0x55A350 -> 0x539E20 reads entry 0x42 and converts it to 555/565). [D: ring pixels at (896, 476) - (64, 32) are (231,231,0) = ntp03[66] (230,230,0) in RGB555; no ring on city tiles. Zoomed out, the disc of `N03-zoomout-nodistricts` matches the screenshot pixel for pixel with this cell and position.]
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

#### City labels: the plates, texts and frame [X][D]

`draw_city_hud` draws, per city and before its icons, the label: a framed box of two plates (name above, production below) with the size to its left and, for the capital, a box to its right. All of it goes to the map renderer's image (§16.8), in these operations (the mirror records them; `civ3_bridge::city_labels` reproduces them; `civ3-tool labels` compares the two, see "Reproduced" below).

**Once per call** [X 0x4E5640..0x4E56F4]: returns at once if the form at **0x9AFD58** is visible (0x609AC0: its `Status2` bit 0 and its parents'; [I: a popup that hides the map's labels]). Saves the image's clip rectangle (JGL_Image vt+0xC8), resets the clip to the whole image (vt+0xC4), `OpenGLRenderer_initialize(0xA525F4, canvas)`, opacity 0xFF, colour 0x80000000 (black), line width 1. At the end it restores the clip (0x5FC990), sets the text colour to white with a black shadow (1, 1) and the font MSF+0x4EB8. The fonts are `get_font(size, style)` (0x54CB30 -> 0x628070, a table of 0x20-byte records by size, then style): the label text is MSF+0x4EB0 = `get_font(10, 0)`, the size number `get_font(16, 0)` (bold with `FSF_BOLD` = 1: `get_font(16, 1)`), both Lucida Sans, `lfHeight` -10 / -16, weight 0 / 700 (the traced `font "Lucida Sans"/-10/0/0/0`).

**Which cities** [X]: list index 0..=`LastIndex` (0xA52E38), the order the labels are drawn in; the filter of 0x4E5748..0x4E57C2 above, then **`0x4E6AB0(x, y, 0, 0)` or `x == 0`** (a city on column 0 is always drawn). `0x4E6AB0` (`Main_Screen_Form_is_tile_on_screen`): false unless `x + y` is even; the view's tile range `[TileX_Min, TileX_Max]` x `[TileY_Min, TileY_Max]` (MSF+0x4E70..0x4E7C) is wrapped like the map (0x426C80/0x426CC0: `+Width` if negative, `-Width` if >= it, only for a wrapping axis, `Map.Flags` bit 0/1); where the range's end is before its start the tile and the start move down by one map width; the tile must lie **strictly inside** (min < t < max) on both axes. The `x == 0` exception means a city at the left seam is drawn although it may be off the view. `0x4E3BD0(x, y)` (`tile_to_screen_coords`): the tile's offset from `TileX_Min` (wrapped as above, then `-Width` if > Width/2, same for y), `sx = (w/2)*(dx - 1) - camera_x % (w/2)`, `sy = (h/2)*(dy - 1) - camera_y % (h/2)` with the tile `(w, h)` = (128, 64), (64, 32) zoomed out, `%` as in C. For a tile that does not wrap that is `64x - camera_x`, `32y - camera_y`.

**Geometry** [X][D]: `sy' = sy + 10`; `top = sy' + ((h/4)*7)/2` (66 below the tile's box top, 38 zoomed out); `cx = sx + w/2`. The label is **not scaled** when zoomed out: it hangs from the half-size tile with the same sizes. `name` is the city's `CityName` (body+0x1C4, 20 bytes, Windows-1252); `adj` = `Leader_get_civ_adjective` (0x55A1C0: `Leader+0x1184` if not empty, else `Races[race].AdjectiveName` = Race+0x74).

```
name_line = name                      + (" (" + adj + ")" if prefs & P_COLOR_BLIND_HELP 0x200000)
own       = city.civ == Player_CivID or debug-see-all           // bVar1
if own:  name_line += " :  " + turns_to_grow                    // "--" for 9999 (0x668F70)
         production = order_name + " :  " + turns_to_build      // only for Order_Type 1 (improvement, BIC+0xBA4 stride 0x110, name +0x44) or 2 (unit type, stride 0x138, name +0x08); else ""
W  = max(width(name_line), width(production)) if own else width(name_line)     // 0x5FE440
W  = (W + 12) rounded up to even
hw = W / 2;  icons = city_hud_icons(city);  ext = 24 if icons non-empty else 0   // City_has_hud_icon 0x4B9C40
L = cx - hw - 24;  R = cx + hw + ext
```

The operations, in order (screen = canvas coordinates; `shade(rect, colour, p)` is `JGL_Image` vt+0x48 `shade_rect`: opacity `a = (256 - (p*256/100)) * 255 / 256` = 127 for p = 50, 191 for p = 25 (the mirror's `Fill` with that alpha); `line` is `OpenGLRenderer_draw_line` (0x627760, black, 1 px: one fill of the line's rectangle, §10.9); `text(s, rect)` is `0x5FF0D0`):

| # | Operation | What it is |
|---|---|---|
| 1-4 | `line` (L, top)-(R, top); (L, top)-(L, top+21); (R, top)-(R, top+21); (L, top+21)-(R, top+21) | the black frame, 22 px high |
| 5 | `shade` (cx-hw, top, cx+hw, top+10), black, 50 % | name plate |
| 6 | `text(name_line, that rect)`, white (248,248,248), -10 | the name |
| 7 | `shade` (cx-hw, top+10, cx+hw, top+11), civ colour, 25 % | a coloured line between the plates |
| 8 | `shade` (cx-hw, top+11, cx+hw, top+21), (74,41,41) (`0x80004945` in 565, `0x800024A5` in 555), 25 % | production plate |
| 9 | `shade` (cx-hw-24, top, cx-hw, top+21), civ colour, 25 % | size box |
| 10 | own and `City.Common_Policy & 0x10`: `outline` of the production plate; then (own) `text(production, plate)`, white, -10 | production line |
| 11 | with `P_SHOW_CITY_POP_DROPSHADOW` 0x2000000: `text(size, size box)` black in the 16 px bold font | drop shadow |
| 12 | own and `Common_Policy & 1`: `outline` of the size box; then `text(size, size box)` in the 16 px font, colour below | the size |
| 13 | with icons: `shade` (cx+hw, top, cx+hw+24, top+21), civ colour, 25 %; then the HUD icons | right box |
| 14 | harbour, airport, barracks icons (the table above) | |

`outline(l, t, r, b)` is `0x6008D0(l-1, t-1, r-1, b, white, white)`: four `hline`/`vline` fills (0x5FFFE0, 0x600010, ends included) in white (`0x80007FFF`, as a fill (255,255,255)): `[l, t-1, r-l, 1]`, `[l-1, b, r-l, 1]`, `[l-1, t-1, 1, b-t+1]`, `[r-1, t, 1, b-t+1]`.

The **civ colour** is entry 68 (not 66) of the civ's colour table `Art/Units/Palettes/ntp<Leader.Color_Table_ID>.pcx` (`Leader_get_label_color`, 0x55A3B0 -> 0x539F80 -> `JGL_Color_Table` vt+0x20(0x44)), quantised to 15 bits. **Text** is drawn by `0x5FF0D0(str, RECT*, len)`: `x = ((r - l) - width)/2 + l`, `y = ((b - t) - H)/2 + t + H` with `H` = `font[+0x10]` (`0x5FE760`) and C division, where `y` is the baseline the game gives `TextOutA` (the mirror's `TextRun.y`); the measured `H` is 7 for the 10 px font and 14 for the 16 px font [D: the baselines of every trace, `top+8`, `top+19`, `top+17`]. Nothing is drawn for an empty string. The **colour of the size** is white, except for an own city that does not grow (`turns_to_grow` = 9999): red (`0x80007C00`, (248,0,0)) if `FoodIncome` (body+0x234) < 0, else yellow (`0x80007FE0`, (248,248,0)). With 565 surfaces the words differ (white `0x8000FFFF`, 0x4945, ...); the traces are 555. `Common_Policy` is body+0x18 (the city governor's policy word, `City+0x34`, read by 0x4C1DC0): bit 0 outlines the size box, bit 4 the production plate (own cities only); in the traces every own city has bit 0, bit 4 only two labels of `S1-16` [D: both outlines seen there; 15 for every own city of the loaded saves].

**Text measure** [X][D]: `0x5FE440` -> 0x5FE470 -> jgl's font object vt+0x10 (jgl RVA 0x3C600) = `GetTextExtentPoint32A` (the game's HFONT selected into the memory DC jgl made with `CreateCompatibleDC(NULL)`, whose bitmap is the default one monochrome pixel), width `cx`. The text itself is drawn (`0x5FD980` -> `TextOutA`, y = the rectangle's top + `H`) on the 16-bit canvas. On Windows both give the same advances (measured on the Windows VM for 10 px, 16 px and bold 16 px: `GetTextExtentPoint32A` of each character equals the sheet's advance, `hdmx` of LSANS.TTF), so the sheets' advances give the plate widths. **Under Wine they differ** (Wine's GDI hints other widths: 'y' 5 against 7, 'T' 6 against 7, ...), and then the traces' plates follow Wine's table: the sheets' `extent` (gdi-text `Glyph::extent`, measured in the process that made the trace: `tools/gen-civ3-fonts --wine`) reproduce them. All traces here were made under Wine.

**Turns** [X][D] (the rules, ported in `civ3_sys::city_label`):
- `turns_to_grow` (0x4E5977..0x4E5A0F): 9999 if `FoodIncome < 1`; else with `f = Leader_get_food_cost_factor` (0x566090: 10 for a human player, else `BIC.DifficultyLevels[p_game_difficulty].Cost_Factor` (+0x68, stride 0x7C), halved with `TR_ACCELERATED_PRODUCTION` 0x200 of `p_toggleable_rules`, at least 1) and `c = City_get_size_class` (0x4275C0: 2 above `MaximumSize_City`, 1 above `MaximumSize_Town`, else 0): `box = (2c + 2) * f`, `q = (box - StoredFood) / FoodIncome` (C division) plus one if `FoodIncome * q < box - StoredFood`; `q == 0` -> `City_requires_improvement_to_grow` (0x4B1E50) `== -1` ? 1 : 9999; `q < 1` -> 1; else `q`.
- `City_requires_improvement_to_grow`: at `size >= MaximumSize_Town` without an active improvement with `ITF_Allows_City_Level_2` (0x800) that is not obsolete (0x4ACBE0, 0x5613F0), and not on a river or next to a lake (0x5F3910: `Map` vt+0x54 `is_near_lake(x, y, 9)` or `Tile` vt+0x60 `Check_River`): the first improvement with that flag; then the same for `size >= MaximumSize_City` and `ITF_Allows_City_Level_3` (0x1000) (nothing if the city has one); else -1. [I: lake adjacency is not looked for; C3X's `patch_City_requires_improvement_to_grow` returns 0 (so "--") with districts and neighbourhoods at their cap, not handled.]
- `turns_to_build` = `City_get_turns_to_build_2(&order, 1)` (0x4BFDB0 -> 0x4BFDD0; C3X adds only a guard for a production of 0): `income = ProductionIncome` (body+0x238), 9999 if < 1; `cost` = improvement `Leader_get_improvement_cost` (0x569F90: `f * Improvement.Cost` (+0x94), halved for a non-wonder improvement whose `Characteristics` match a bonus of the owner's race (`Race+0x948` bit `RB_*`: militaristic 0 / `ITC_Militaristic` 2, religious 4 / 0x100, agricultural 6 / 0x400, seafaring 7 / 0x800, scientific 3 / 0x20), times `clamp(Leader.Cities_Count * 6 / WorldSizes[Map.World.World_Size].OptimalCityCount, 3, 10)` for `ITF_Center_of_Empire` (the palace)) or unit `Leader_get_unit_cost` (0x56A1C0: `f * UnitType.Cost (+0x54) / 10`), at least 1; for an improvement `income += sum of Citizen_Type.Construction (+0x7C, BIC+0xBA8) of the city's citizens with status 0` (specialists; not below 0, 9999 if 0); `cost -= max(0, StoredProduction (body+0x28) + (improvement ? body+0x39C : 0))`; `turns = max(1, ceil(cost / income))`.
- `City_has_improvement(i, 1)` (0x4ACBE0): `GovernmentID` of the improvement is -1 or the owner's; the owner's free buildings (`Leader.Auto_Improvements`, key `i`, or `(continent + 1) * ImprovementsCount + i`); else the city's bit.

**HUD icons** [X][D] (`City_draw_hud_icon`, 0x4B9CF0; sprites 0xB3E680 + 0x2C * code, cells (1, 1, 18, 18), (20, 1, 18, 18), (39, 1, 18, 18) of `Art\Cities\city icons.pcx`, drawn one over the other at `(cx + hw + 2, top + 2)`): 0 a grey shield, 1 the capital's star, 2 a coin. For a city of the viewer: 0 if it has a small wonder or wonder with `SmallWonderFlags & ITSW_Allows_Spy_Missions` (0x80; `Leader_count_wonders_with_small_flag`, 0x55A9C0; the city of small wonder `i` is `Leader.Small_Wonders[i]` (+0x15E8), of a wonder `Game_get_wonder_city_id` = `[p_game + 0x4F8][i]`) while the owner is not in anarchy (`Leader.Espionage_2`'s vt+0x14, 0x5A5CD0); 1 if `city.ID == Leader.CapitalID`; and then 2 if the owner has a capital, knows a tech with `ATF_Enables_Diplomats` (0x1) and is not in anarchy (`Espionage_1` vt+0x14, 0x504460). For a foreign capital: 0 if `viewer.field_D70[civ]`, then 1, then 2 if `viewer.field_D50[civ]` and not `viewer.At_War[civ]`. No icons, no right box. `City_has_hud_icon` (0x4B9C40) is true exactly when there are icons. [D: 1 and 2 (the coin only for a capital) in the traces; 0 never seen.] C3X (`ai_multi_city_start` > 1, off by default) also gives AI cities with a second palace the star: not handled.

**What the mirror shows of it** (for comparing): fills are cut to the image (`canvas_clip`), an opaque fill hides earlier operations entirely within it, text is kept only if it starts inside the image, images are cut to it, and `Sprite_draw` adds the sprite's crop offset (the mirror lists the trimmed cell and where that lands). A label that is only partly in the image is cut accordingly.

**Not covered**: the colony labels (0x4E4990; a tile building that is not an airfield, radar or outpost: its owner and resource names in two lines on boxes of the same make, and the resource's icon through `Sprite_draw_on_map` with a blend table), C3X's natural-wonder and named-tile texts that `patch_Main_Screen_Form_draw_city_hud` adds after the cities (`draw_map_tile_text`), the C3X options above, the debug flag that shows all cities in multiplayer (`0x49A070` clears bits 2..3 of `p_debug_mode_bits` there), 565 surfaces.

**Reproduced** [D]: `civ3-tool labels TRACE... [-v]` builds the labels of a dump's world with `civ3_bridge::city_labels::labels`, moves each to where the game puts its tile (`tile_to_screen`), applies the rules of the previous paragraph and compares them operation by operation (in order, as a longest common subsequence, so that cut labels and other drawings of the canvas do not disturb it) with the operations the mirror listed for the map canvas. A dump's texts come from its city lines (`city ... <order type> <order id> <policy> <food income> <grow> <build> <icons> <veteran icons><TAB><production name><TAB><name>`, and `civ_adjective`, `debug_mode_bits` lines), read from the game's memory by `civ3_sys::Civ3::cities`; older dumps, which have none, get theirs from the mirror's own text first (so their texts are not independent, but everything else is: positions, widths, colours, outlines, icons, clipping, order). The font sheets are those of the environment the traces were made in (`CIV3R_FONT_CACHE`; all of these were made under Wine, so Wine's, from `tools/gen-civ3-fonts --wine 10 16`; with the Windows sheets only the single-city dumps agree, whose widths coincide). Results over all 131 dumps of `/mnt/datasets/winvm/civ3r-runs` that have map operations, 91 of them with labels: **1408 of 1408 labels exact, 18 230 of 18 230 operations**:
- 71 dumps of earlier runs (`verify-viewer`, `verify-e1717d4`, `verify-districts`, `verify-44561a2`, among them 7 zoomed out, the WW2 Pacific, Greek/Roman and other scenarios, wrapping maps, labels cut by every edge, the city screen opened and closed): 1037 labels, 13 640 operations. 129 further labels are cut by the image so that no text of them is left to take the name from: not checked.
- 20 dumps of the live game with everything read from its memory (saves `2025 Grece Dec 10` and `2024 Rom  C XXIV`: 83 and 52 cities of 6 and 4 civs, a grid of 11 positions over the wrapping 80x80 map including the seam, and 6 zoomed out; the Preferences dialog's "Show City Pop. Dropshadow" turned on and "Color Blind Help" off): 371 labels, 4590 operations, nothing the game drew that was not expected. This checks the names, orders, turns (growth and production, among them 9999, the red and yellow sizes, aqueduct limits), policy outlines, icons and the preference paths against the game's own text.

What the game drew on that canvas and nobody expected (341 operations in the old dumps): the pieces of the 129 cut labels, the icons of cities whose label is entirely off the image, the settler's radius lines (§10.9), and the natural-wonder names C3X adds ("Mount Fuji", ...).

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

**Smooth lines** [X 0x627710; D Windows VM]: `OpenGLRenderer_initialize` turns on `GL_LINE_SMOOTH` (hint `GL_NICEST`) and blending (`SRC_ALPHA, ONE_MINUS_SRC_ALPHA`). A line along a row or column at integer coordinates lies on a pixel edge: a 1-pixel line at y halves rows y - 1 and y (x from the start to, not including, the end), a 1-pixel line at x columns x - 1 and x. The city labels' black frame (opacity 0xFF) therefore halves two rows or columns per edge, where overlapping edges darken twice. Under Wine C3X draws lines with GDI+ instead (`draw_lines_using_gdi_plus = wine`), which gives one opaque row at y (measured). `civ3_bridge::ui::line_fills` gives both.

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

### 10.13 Units as tile layers (our renderer)

What 10.2 says about the draw list, read for a tile renderer: a tile shows **one unit** (`find_visible_unit`: the strongest defender, not the top of the stack, or the selected, animating or attacking unit), plus its stack lines; the only second unit on a tile is `u2`, the unit under a moving (`U+0x5D`) or always-on-top unit, appended just before it. Always-on-top units go to the end of the whole list. The list runs over the rows top to bottom, each **right to left**, and `Unit_draw` draws each unit's disc, cursor, body (shadow first; an army: banner, then member) and HUD before the next unit.

So `civ3.toml` has a units pass after the fog of war (pass 6, `descending`: its rows are swept right to left) with two *slots* of layers, `unit_*` and `unit_over_*`: disc, (our) ring, cursor, banner, shadow (scale 16), body, HUD bar, fortified frame, LED, stack lines, in that order. `crate::units::place` puts each listed unit on its tile's next slot. A unit's cells carry the offset of where it is from where an idle unit stands (`pixel_loc - (64x + 64, 32y + 32)`; an army's banner +40 and -20 while attacking east), so a unit on its way or fighting stays in its tile's place in the order; the renderer adds the offset (halved, rounded down, zoomed out: `pixel_loc / 2` rounds down too) and widens the window of drawn cells by how far offsets reach. Cell values select an animation of one direction block (`UnitAnim@<file>/<block>`, an army's own art `UnitAnim@<file>#army/<block>`, `UnitShadowAnim@<file>/<block>`) and, in their upper bits, the colours: the art is indexed (one byte a pixel, once in the atlas for every civ; body and shadow share it), its indices 0..63 (an army's own art 0..127) drawn through the named palette the cell picks: `ntp@NN` for colour table NN, `UnitArmy@<table>a<first>x<members>` for an army's (10.11), `disc@NN` for the disc (`ntp@NN` with entry 66 copied to 247), as 10.5 composes them. The colour civ is 10.2 step 2's: civ 0 for a unit of hidden nationality another real civ looks at (`UnitType.UnitAbilities` bit 0x11), unless the debug mode shows all; the disc follows it too [I]. The cell's phase the frame (the game's own, `ticks_per_frame = 0`). The HUD is generated art (`crate::units::hud`): one sprite per bar state (black bar and segments), per bar height for the frame and the LED, per stack size, at the HUD's origin and drawn at full size zoomed out (the layers' offset (32, 0) is halved, the sprites' anchors are not), pixel for pixel what 10.6 draws.

What the layers cannot draw in the list's order stays a free sprite after the units pass, in list order, together with every unit after it: always-on-top units (unless their tile comes later in the order), a third unit on a tile. Such a free sprite picks the named palette its cell would (`disc@NN`, `ntp@NN`, an army's row, the viewer's for the cursor); without one its art shows the animation file's own colours (a flying bomber's body in the FLC's entries 0..63, its disc in index 247's green). Inverting rings (`CIV3R_UNIT_RINGS`, a blend a layer cannot do) are free sprites under the units. [D] Every stored trace (31 units in `hd-ww2`, the army traces, zoomed in and out) renders byte for byte as with free sprites, and so do variants with moving units, a fight, a moving unit over the one under it, a selected unit and an always-on-top unit.

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
2. **Shadow/transparency alpha**: the exact blend for indices 240-254 (units), 248-251 (shadow sheets) and the use of 254 in terrain sheets are inferred from palettes and C7 (JGL.dll's 8-to-16-bit blits were read for their scaled paths, §3 and §3.1, not for the blend tables).
3. Initial values of the viewport rectangle globals (0x9CB3FC..0x9CB408, UI-layout dependent), the exact camera clamping rule for non-wrapping maps and `Map.Flags` bit 2.
4. Waterfall sprite selection, polar ice-cap pixel placement, railroad sprite indices 256-271, pollution neighbour bit order, `tnt` file-row/terrain mapping for grass vs bonus grass, goody-hut index selection, fortress/camp era rows, airfield/radar/outpost placement offsets.
5. City: the "walled" size class 3 trigger, ruins/colony details, city name banner placement and size-number font.
6. LM terrain: how the 7 `landmark_terrain` cells map to terrain types.
7. Zoomed-out drawing of what the map renderer does not slice (units, the team disc, the unit HUD): positions are halved, the resampling of the scaled colour blit 0x5F8870 is not checked. For sprites cut by `Sprite_slice_pcx` it is settled (§3.1).
8. The meaning of `Terrain_Overlays` bits 17 (set on 1480 of 3200 tiles in `MK 10.SAV`), 19, 24, 27, 28 beyond what is listed.
9. BIC_get_asset_path root list at 0x9C4144 (content built at runtime).

## 15. How the findings were reproduced (so they can be rechecked)

- PCX sizes: `perl` PCX header read (xmax/ymax). Sprite-sheet grids: the loop bounds in the renderer initialiser (0x4C5A30), listed in section 12.
- Struct offsets: `gcc -m32 -fsyntax-only`-style `offsetof` dump of `Civ3Conquests.h` with a stub `windows.h`.
- SAV/BIQ: PKWare DCL decompression (port of zlib `blast.c`; C7 uses the same, `Blast` project), then TILE chunks (4 per tile).
- Image-index formula validation: 27,780 interior tiles from `Rom D 4.SAV`, `MK 10.SAV` and 12 saves in `Conquests/Saves`: `loc = d(N) + 3 d(NW) + 9 d(NE) + 27 d(T)` matched for sheets 0-4 and 6 with zero mismatches.
- River bit mirroring: 982/982 mirrored pairs in `MK 10.SAV`.
- Zoomed-out sprite rule (§3, §3.1): jgl.dll and the exe read with capstone (the blit dispatcher 0x10008180 and its routines 0x1000ADA0, 0x100141E0, 0x10015C90, 0x10034400; `Sprite_slice_pcx` 0x5F7EC0 -> 0x62B170; the territory loader 0x4C6A80..0x4C6B0C; the unit disc 0x5CBF57..0x5CBFAD), then checked against the game: `civ3-tool pixeldiff TRACE GAME.png [DIFF.png]` on the zoomed-out traces and screenshots of the C3X R28 Pacific / Middle Ages runs (`/mnt/datasets/winvm/civ3r-runs/verify-districts/`, `CIV3_INSTALL` = the R28 tree). Share of the 1920x1080 frame identical, before -> after (whole frame; terrain = the pixels none of our unit, label or interface sprites covers):

  | trace and screenshot | whole frame | terrain |
  |---|---|---|
  | `N03-zoomout-nodistricts` (districts off) | 45.7% -> 98.0% | 40.5% -> 99.8% |
  | `A03-zoomout` (districts on; the branch does not draw districts) | 44.8% -> 96.0% | 39.6% -> 97.9% |
  | `L3-zoomout` | 45.4% -> 97.6% | 40.2% -> 99.4% |
  | `M1-zoomout` (Middle Ages) | 93.2% -> 99.1% | 94.1% -> 99.96% |
  | `M2-zoomout` (Middle Ages) | 93.2% -> 98.8% | 94.0% -> 99.6% |

  The zoom-1 frames (`A02-frameA`, `N02-frameA-nodistricts`, `L2-frameA`, `L3-frameA`, `M2-frame`) are unchanged to the pixel. What is left in `N03-zoomout-nodistricts` (41,217 pixels): the city labels' text and bars (ClearType in ours, plain anti-aliasing in the game: about 27,000), unit bodies (about 8,600 around the 31 units; the screenshot shows another animation frame than the trace), the minimap and bottom-right interface (2,800), and about 2,700 elsewhere: border sprites cut by the canvas' top or left edge (0x10015C90 starts at the sprite's first pixel, §3) and road/forest overlap order. `M2-zoomout` lacks the game's cities of that scenario (at zoom 1 as well: unrelated to the zoom).

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

**What goes straight onto the back buffer is clipped to the rectangle being updated** [X; D, Wine]. `m80` sets the back buffer's clip to its `RECT*` argument (the paint's rectangle), or for a form relative to its parent to `IntersectRect(parent's client rectangle on the screen, rect)` (0x6084AF), or to the whole buffer without one; `present` sets it to its `r` if there is one (0x6066A1). A paint of a small rectangle therefore draws a popup's frame and the tooltip clipped to it: with the pointer on a popup's choices the paints are of the choice list's rectangle (803,185-1117,237 for the "really quit?" popup at 1920x1080), with the pointer on an order button of the button's (32x32). The tooltip appears in a present outside a paint, after a compose of only its rectangle, and is drawn again by every present after (the map's animation presents all of the screen about every 65 ms), with no paint when it goes. The mirror records draws onto the back buffer without that clip (`ui::content_clip`) and what a present draws as the top layer, replaced by every present (`ui::Layer::Overlay`): which part of the back buffer reaches the window is the presents' to say, below.

**Partial presents** [X, measured in Wine]: `present` is also called outside a paint, after a compose of only a rectangle (0x602430 -> 0x609350 -> `present(r)`, and 0x609A5D, 0x606A3B, ...). The cultural advisor (F5, vtable 0x66BBC0) does this while it redraws: every few seconds, for about 0.4 s, it clears its canvas, draws its background and graph, composes the whole screen onto the back buffer and presents only the graph's rectangle (screen 528,246-828,391 at 1920x1080), then draws its text and a paint presents all of it. The back buffer then briefly holds the advisor without its text, but the window only ever gets the graph's rectangle of it. A mirror has to show what was presented where it was presented, not the back buffer.

**Forms drawn over themselves** [D, Wine and the Windows VM]: the advisors' common background form (canvas 0x9AFFCC, vtable 0x6688B8) draws its art (e.g. `culture.pcx` 1024x768 at (0, 0)), the small head and the six tabs again every few seconds, with keyed blits and without clearing its canvas first; the advisor copies that canvas into its own. A retained display list that only drops what an opaque operation covers keeps every copy (about a dozen draws, one of them full-screen, per 5.4 s on F5). Drawing the same picture or opaque fill again at the same place shows nothing the earlier one did not, so the mirror drops the earlier one (translucent fills, masked pictures and text add up as in the game). The F5 "head bursts" are the speech bubble: its text fades out in five redraws 70-90 ms apart (colours 48, 96, 152, 200, then 0 for the next advice), each a graph present and a paint.

**Text on the colour key** [D, Windows VM]: the domestic advisor (F1) clears its canvas to the key (0x7C1F) and draws its text on it, its panels lying on the form below. Windows blends the text's ClearType edges with the key (magenta), and the keyed composite shows every edge pixel that is not exactly the key in 15 bits: magenta and violet fringes that make the text look bold. Under Wine the game's text is unsmoothed, so there are none.

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

**Minimap**: `Navigator_Data` lives at **Main_GUI+0x10720 = 0xA0E230** [X: passed as `this` to 0x578050/0x5786F0/0x578E00]. Its PCX_Images (+0x18, +0x2D0, +0x588, +0x858 [H]) are filled per pixel from world state, then copied into the Main_GUI canvas [X calls, I content]. (Correction, see section 17: the copies at 0x5784EC, 0x578625, 0x578733, 0x57886C are image-to-image copies inside `Navigator_Data`; the copy onto the Main_GUI canvas is 0x5FCB30 at 0x57AED0.) The renderer has to generate the minimap itself. Only the frame and buttons come from art files (Main_GUI `Images[252]` at +0x140D8, file names `PCX_Files[25]` at +0x2F0C [H]).

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

### 16.8 City view

**Summary.** The terrain in the middle of the city screen is the main map. There is no second renderer and no extra canvas. Opening the city screen re-centres the normal map camera on the city and switches the existing `Map_Renderer` into a "focus city" mode. In that mode the fog pass leaves only the city's own workable tiles lit. The city form canvas (0x740C34) is transparent in the middle, so the map shows through it [X; D: ui4/ui5 traces and screenshots].

**There is one `Map_Renderer`, and it is a `PCX_Image`** [X]. Its vtable 0x66A508 is written in three places: the constructor 0x4C29A0 (sole caller 0x5D892D, inside the `Map` constructor), the destructor 0x4C7Bxx and the `Map` destructor 0x5DC2C0. So the only instance is `Map+0x234` = **0x9C7560**. The renderer derives from `PCX_Image`: +0x00 is the vtable and +0x04 is the `JGL_Image*` (the header's `field_4[173]` = 0x2B4 bytes, so the `PCX_Image` part is 0x2B8 bytes, as in 16.2). Proof:
- `Map_Renderer_init` **0x4C30A0** `(this, x, y, w, h, a5)`, `ret 0x14`, is called once (0x4C8E4B). It runs `PCX_Image_create(this, w, h, 16, 1, 0, 0)` on `this`, then stores the **destination RECT `(x, y, x+w, y+h)` at renderer+0x3E9C**. These are the globals 0x9CB3FC/0x9CB400/0x9CB404/0x9CB408, i.e. the "viewport rectangle" of section 3. Finally it clears **renderer+0x3E94 (focus city)**.
- Every layer method gets its destination as a stack argument. `m71` passes **the renderer itself**: original `m71` = **0x4C3100** (vtable slot +0x11C; the installed exe has C3X's 0xCF2FC8 there). 0x4C3100 calls 0x5F4480(civ, canvas = `this`, a3), which calls `m20`(civ, -1, -1, canvas, a3, -1, -1), and from there `m21`/`m19`/the layers. For example, the LM-terrain layer at 0x4C392D loads that argument, sets it as the clip image (`0x5FC990`) and pushes it as the first argument of `Sprite_draw_on_map`.
- So **`Sprite_draw_on_map` 0x5F8060 `(Sprite* this, PCX_Image* dest, x, y, num_x, num_y, den, ct)`**: the first stack argument is the destination image (it reads `[arg1+4]` as a `JGL_Image*`, and the lazy-file path calls `PCX_Image_draw_onto(arg1, x, y)`). C3X types it as `Map_Renderer*` only because every map caller passes the renderer. The `colored` `Territory_Images` draws in ui5 show `canvas = canvas2 = 0x9c7560` [D].

**Path of a map pixel** [X]:
```
m71 0x4C3100 (this = 0x9C7560, civ, dest, a3)          // the Animator calls it with dest = 0x9F8934 (MSF canvas), 0x4EF07B
  if byte [0x74238D] (= City_Form 0x7409C0 + 0x19CD, "city screen open"):
      PCX_Image_fill_area(this, &this->rect_3E9C, 0x80000000)   // black, on the renderer image
  0x5F4480(civ, this, a3)                              // all terrain passes (section 4) -> Sprite_draw_on_map(spr, 0x9C7560, x, y, ...)
  if !m5 (debug bit 0 clear):                          // m5 = vt+0x14 = 0x4C3190
      MSF 0x4E5640(this)                               // city name labels etc., drawn into the renderer image
      MSF 0x4E4990(this)                               // further map labels [I]
  this.0x5FCCB0(dest, JGL vt+0xD4() /*whole image rect*/, &this->rect_3E9C)   // opaque copy onto the MSF canvas
Fog pass 0x4C4F10 (this = renderer, civ, dest = 0x9F8934, RECT* = 0)   // 0x4EF09E; draws directly on the MSF canvas
```
- `draw` coordinates in the traces are **renderer-image pixels**. They land on the MSF canvas at `+ (rect.left, rect.top)` of renderer+0x3E9C. The copy uses JGL `Copy(dst, src_rect, dst_rect)`. 0x4C30A0 makes the image exactly `w x h` and the rect `(x, y, x+w, y+h)`, so the copy is 1:1 with no scaling [X sizes; I: no other writer of +0x3E9C exists in .text; the only `[reg+0x3e9c]` stores are 0x4C30C4..0x4C30DE]. The MSF canvas is at screen (0, 0) (`ui_form 0x66b3d8 0x9f8934 0 0 1920 1080`), so screen = renderer pixel + rect origin. In ui4/ui5 the origin is (0, 0) [D: draws span x = -64..2024].
- Fog tiles (`Fog_Of_War_Images`, +0x41F8) are drawn by 0x4C4E60 with **0x5F86A0** `(spr, dest, x, y, [0x9C7304], 1, 1, den, 0)` (push order at 0x4C4ED2..0x4C4F05). That is not `Sprite_draw_on_map`, so `trace.rs` does not record fog, and the fog lands on the MSF canvas, not on the renderer.

**What opening the city screen changes.** `City_Form_open` is 0x421720; C3X redirects its entry to 0xCE9BCD, and the body continues at 0x421725 [X]:
1. Saves the zoom (`0x7409BB = [0x9C7300]`) and **forces normal zoom** (`[0x9C7300] = 0`, 0x4218A5). `City_Form_close` (0x421BA0) restores it (0x421BC3).
2. **Focus city**: `[0x9CB3F4] = City*` (0x4218AB). This is renderer+0x3E94. Close sets it to 0 (0x421C7D).
3. Sets `City_Form+0x19CD = 1` (0x4218B6). The camera setter 0x4DF7C0 then **skips its clamping** (test at 0x4DF846), and `m71` fills the renderer image black first.
4. **Re-centres the camera**: `MSF 0x4DFAA0(city.X, city.Y + 2, 0, 1, 0)` (0x4218D3; City+0x24/+0x26 = Body X/Y). The view centre is the tile one tile south of the city, so the city sits 64 px above the screen centre. ui5 shows this: camera (1536, 68), the city sprite at (877, 429), i.e. city (38, 16), and the screen centre (960, 540) = tile (38, 18) [D].
5. Erases the city form canvas with the JGL colour key (`0x602120(color = JGL_Image vt+0xC0())`, 0x421858) and hides the HUD (`Main_GUI 0x556460(0)`).

**Focus-city fog** (in the fog pass 0x4C4F10; the only readers of renderer+0x3E94 are 0x4C5260 and 0x4C53B3) [X]. For each of the four tiles k around a vertex, the pass first builds the usual masks (6.8): `explored |= bit k` if `Tile.Body.Fog_Of_War` has the viewer's bit (or `civ <= 0`). **If a focus city C is set, the normal "visible" test is replaced:**
```
n = Map.vt[+0x70](C.X, C.Y, tx, ty, 21)       // index of (tx, ty) among C's 21 neighbours, -1 if outside the fat cross
if n != -1 and City_tile_in_view 0x4BB560(C, n, 0):  visible |= bit k; explored |= bit k
```
`0x4BB560(city, n, check_units)`, `ret 8`: `n == 0` -> true; `n >= 21` -> false; tile off the map -> false; **tile's territory owner (`Tile vt+0x98`) != city's civ -> false**; otherwise true (`check_units = 0` here, so the enemy-unit test after 0x4BB644 is skipped). Then 0x4C4E60 builds the digits `2 = visible, 1 = explored, 0 = unexplored` exactly as in 6.8 and skips index 80. So:
- Tiles of the city's fat cross that its civ owns (and the city tile) are lit.
- Every other explored tile is fogged (cell 40, about 40 % black).
- Unexplored tiles are black.
- The soft "vignette" is just the partial FogOfWar cells along the boundary. ui4-game.png shows exactly this: the lit fat cross, dimmed explored land around it, black beyond [D].

**What the city form draws over it** [X calls; D: ui4/ui5 `ui_image` lines]. All of this goes into the city form canvas 0x740C34 (`p_city_form` 0x7409C0 + 0x274), which is at screen (0, 0), size 1920x1080:
- `Art/City Screen/background.pcx` (1024x768) at (448, 156). Its middle band is colour 255 across the full width (decoded: every sampled pixel of rows 150..470 is 255; row 510 is not), so the map shows through. The canvas is composited colour-keyed [I: required by the screenshot; the Status1 bit was not traced].
- The worked-tile icons: food/shield/commerce cells of `Art/City Screen/CityIcons.pcx` (`City_Icons_Images`, enum `City_Icons_Small`), e.g. `ui_image 915 470 ... 474 10 16 15 CityIcons.pcx`.
- The white outline of the workable area: the function around 0x4210CA walks the 21 neighbours with `0x5F3E80` / `0x4BB560` and draws white (`0x8000FFFF` / 555 `0x80007FFF`) edge lines at `MSF 0x4E3BD0` tile positions. The mirror already reproduces this (ui5-ours.png).

These are ordinary UI blits. The mirror already has them. **What the mirror lacks is the map under the city form.**

**Hook plan (city view).** Pick one of:
- **(A) Record the game's blits.** The existing `Sprite_draw_on_map` hook already records the city view; it is the normal map redraw (ui5 `draw` lines). Changes needed:
  1. Treat arg1 as the **destination image**, and resolve sprites against the renderer from `Draw_Tiles`' `this` (always 0x9C7560), not against arg1. Today `trace.rs` passes arg1 to `identify_canonical`. That works only because the two are equal for map layers.
  2. Hook `m71` 0x4C3100 (or the C3X slot as now). Record `dest = [esp+8]` and `rect = *(RECT*)(0x9C7560+0x3E9C)`, and translate every recorded draw by `(rect.left, rect.top)` onto `dest`. Also record the black pre-fill when `byte[0x74238D] != 0`.
  3. Hook **0x5F86A0** (fog: `Fog_Of_War_Images[idx]` at `(x, y)` on `dest` 0x9F8934), or keep computing fog yourself (B).
  4. Then emit `UnitArt`/map sprites as "named sprite S at (x, y) on canvas 0x9F8934".
- **(B) Keep rendering the map ourselves** under the city form. Read the same camera (`MSF+0x4E98/0x4E9C`; the game has already moved it). Assume normal zoom. Fill black. Apply the focus-city fog rule above whenever `[0x9CB3F4] != 0` (focus City*: territory owner == `City.CivID` (City+0x28 byte) and neighbour index < 21 via the 21-tile table `0x5E6D80(n, &dx, &dy)` [I: table semantics]). Draw city labels as usual. The ui model must keep the map underlay when the city form (vtable 0x6658B0) covers it: its canvas is keyed, not opaque.

Detect the mode with `byte [0x74238D]` (City_Form+0x19CD) or `[0x9CB3F4] != 0`.

The bridge applies the focus-city rule to the fog layer while the city screen is open (`rules::fog`): the city screen's view then shows the land around the city dimmed as the game does (the Windows VM check had 74 % of the city view identical without it). The other views show the map as the game's own renderer has it then.

### 16.9 Unit pictures in panels

**Unit info box (HUD, bottom right)** [X]: `Main_GUI` method **0x554230 `(Main_GUI* this = 0x9FDB10, Unit* u)`**, `ret 4`. It is called from the box painter at 0x5549D0 with the current unit (`MSF.Current_Unit`, 0x9FD434).

**Sprites cut with a civ's colours** [D]: the citizens' heads (`Art/SmallHeads/popHeads.pcx`, F1's city list, the city screen) and the units' icons (`Art/Units/units_32.pcx`, F3) are drawn with `Sprite_draw` without a colour table and have no colour table of their own (Sprite+0x0C), yet show in the player's colours: the palette they were cut with, the `JGLSprite`'s (+0x10, a `JGL_Color_Table*`), has the civ's colours in its first 64 entries (Rome's `ntp01`, measured). The mirror gives them that civ's palette.
- **Picture rect** (Main_GUI-canvas coordinates): with `RECT box = Main_GUI+0x13FE8` (L, T, R, B):
  ```
  left = L;  top = T + (B - T)/2 + 11;  right = R - 27;  bottom = B - 2*K - 27,   K = *(int*)([Main_GUI+0x10618] + 0x10)  [I: a font/line height]
  ```
  (C division truncates toward zero.)
- **If the unit's animation is loaded** (`byte Unit+0x38D` = `FLC_Animation+0x111`, the flag 0x403120 also tests): `Unit_draw_picture 0x5CC710(u, canvas = Main_GUI+0x274 = 0x9FDD84, &rect)`.
- **Otherwise**: the 32x32 icon. `Sprite_draw(&units32[i], 0x9FDD84, x, y, 0)` with `units32 = *(Sprite**)0xB42184` and `i = 0x54C660(Current_Unit.UnitTypeID, Player_CivID, 0)`, centred the same way as below. `units32` is sliced by 0x54A33x from `Art\units\units_32.PCX` (string 0x72BFC4): cell i = `(1 + 33*(i % cols), 1 + 33*(i / cols), 32, 32)` with `cols = W/33` and `rows = H/33`. These are ordinary UI sprites, and the slice hooks of 16.7 already name them. The city screen's garrison icons are the same sprites (ui5: `ui_image 743 887 ... 34 1 32 32 Art\units\units_32.PCX`) [D].

**`Unit_draw_picture` 0x5CC710 `(Unit* this, PCX_Image* canvas, RECT* r)`**, `ret 8` [X]. `Unit+0x27C` is the unit's `FLC_Animation` (Body+0x260):
```
img   = Unit+0x2D4                                   // FLC_Animation.Frame_1 (FLC_Frame_Image)
info  = *(Animation_Info**)(Unit+0x360)              // FLC_Animation+0xE4
flic  = info->Animations[1]                          // Animation_Info+0x13C (Flic_Anim_Info**), index 1 = DEFAULT
FLC_Frame_Image_load 0x5F7A90(img, flic, 0)          // (re)binds the unit's own Frame_1 to the DEFAULT animation
FLC_seek 0x5F7D00(img, slot = [0x73DD88], frame = 1, 0)   // [0x73DD88] = direction->block table 0x73DD7C [3] = 2 (SE), see 10.4
spr   = img + 8                                      // 0x5F7CF0
save = Unit+0x2A0; Unit+0x2A0 = 1                    // FLC_Animation summary.current_anim_type := DEFAULT
ct    = FLC_Animation_civ_color_table 0x403040(Unit+0x27C, Player_CivID [0x9FD47C])   // 10.5: DEFAULT flic's table +0x78, entries 0..63 := ntp[leaders[viewer].Color_Table_ID]
Unit+0x2A0 = save
if Unit_has_ability 0x5BC5C0(u, 0x12 = UTA_Army):   // entries 64..127 := owner's ntp entries 0..15, repeated 4 times
    tmp = ntp[leaders[u.CivID (Unit+0x34)].Color_Table_ID].get(0, 16); replicate x4; ct.set(64, 64, tmp)
x = r.left + (r.right  - r.left)/2 - spr[0x20] - spr[0x10]/2
y = r.top  + (r.bottom - r.top )/2 - spr[0x24] - spr[0x14]/2
Sprite_draw_with_color_table 0x5F87E0(spr, canvas, canvas, x, y, [0x9C7304], ct)
```
The wrapper adds `spr[0x20]/[0x24]` back, so the **FLC frame bitmap (size `spr[0x10] x spr[0x14]` = FLC header @8 x @10) is centred in `r`**. Its top-left is `(r.left + (r.right-r.left)/2 - spr[0x10]/2, r.top + (r.bottom-r.top)/2 - spr[0x14]/2)`. The colour comes from the **viewing player** (not the owner), except for the army overlay. Side effect: the unit's live map `Frame_1` is left on DEFAULT/SE/frame 1 until the animator seeks again (0x402620) [X calls; I visible effect].

**City screen production picture** [X calls, I purpose]: `City_Form` method **0x424A50**, `ret 0x10`. It draws the `FLC_Animation` embedded at **City_Form+0x96C8** (`struct_188`; its `Frame_1` is at +0x9720):
- `0x402620(anim, 3, 0)` (direction SE, frame 0 [I]).
- `ct = 0x403040(anim, city.CivID)` (City+0x28 byte, the owner).
- `0x5F87E0(spr, canvas, canvas, x, y, [0x9C7304], ct)` with x centred horizontally in the target image (canvas = the method's 2nd argument).
- The animation is loaded in `City_Form_open` by `0x4062D0(path 0xCAB298, City_Form+0x96C8, civ, -1, -1, 0)` when `City+0x50 == 2` (0x4217CD; [I] = "producing a unit"), with the path from `BIC 0x5987B0`.

The same identification works for every FLC sprite drawn in a panel. Other `0x5F87E0` callers are map-only: 0x403120 (FLC_Animation draw, from the animator), 0x4C3C42 (borders), 0x4E46A0 (selection cursor, `FLC_Animation` at 0xA281B4), 0x4F0522 and 0x5CC04B/0x5CC571 (`Unit_draw`, armies). FLC files are also loaded at 0x4C8297/0x4C82D0 (renderer) and 0x5892F9 (a dialog [?]).

**From a `Sprite*` to (FLC file, colour table, chunk)** [X layout; I chunk numbering, which matches units.rs/10.4]:

| Where | Field |
|---|---|
| `*(u32*)(spr - 8) == 0x665450` | `FLC_Frame_Image` vtable (ctor 0x5F7A60). This is the test for "this Sprite is an FLC frame". `Flic_Anim_Info` begins with its own `FLC_Frame_Image`, but its vtable is overwritten with 0x66A634 (ctor 0x600BB0), so that one does not match |
| `*(Flic_Anim_Info**)(spr - 4)` | the animation (`FLC_Frame_Image+0x04`, set by 0x5F7A90) |
| `*(i32*)(spr + 0x34)` | `Current_Frame_ID` = frame within the block (`img+0x3C`) |
| `*(i32*)(spr + 0x38)` | **block** (`img+0x40`, -1 before the first seek) |
| `flic+0x58` -> FLIC header | `@0x60` u16 = number of blocks, `@0x62` u16 = frames per block (fpd), `@0x70` u32 = present-direction mask (0 = all) |
| `flic+0x48` | `Direction_Frames`: start chunk of each block |
| `flic+0x78` | the FLC's own `PCX_Color_Table*` (what 0x403040 rewrites and returns) |

Seek 0x601490 (`Flic_Anim_Info*` in ECX; args slot, frame, img, a4) [X]:
- `block = slot` if `frame == -1`; otherwise, if mask != 0, `block = popcount(mask & ((1<<slot)-1))` (it fails if bit `slot` is clear); otherwise `block = slot % @0x60`.
- `frame %= @0x62`.
- It decodes from `Direction_Frames[block]` and steps forward until `Current_Frame_ID == frame`.
- So **`chunk = block*(fpd+1) + frame`**, the index units.rs uses.
- For the HUD: `chunk = 2*(fpd+1) + 1` when the file has all 8 directions.

**File path**: hook **`Flic_Anim_Info_load` 0x600FA0** (ECX = `Flic_Anim_Info*`, `[esp+4]` = `char*` path, already resolved by `BIC_get_asset_path`). Keep `flic -> path`; return 0 = success, and 0x600C00 clears it. Alternative without a hook: `flic == Unit.Animation_Info->Animations[k]` gives the animation type k (1 for the HUD). Then `UnitArt::file(type_key, era_key(owner), k)` names the same file.

**Hook plan (unit pictures).**
1. **Share the existing inline hook on 0x5F87E0** with `trace.rs`; do not patch it twice. It is `(spr, canvas, canvas2, x, y, flags, ct)`. For a destination that is a UI canvas (not 0x9F8934 / 0x9FD80C):
   - if `*(spr-8) == 0x665450`, emit `Blit { source: UnitArt@<path>#<table>/<chunk>, canvas, x: x + spr[0x20], y: y + spr[0x24] }`;
   - otherwise fall through to the 16.7 sprite origin (`units_32`).
2. **Colour table**: hook `0x403040` (thiscall, 1 arg `civ`). At return, remember `ret_ct -> civ` (the table is the same object for every draw of that animation, rewritten each time, so take the latest mapping). Then `#<table>` = `leaders[civ].Color_Table_ID` (`0xA52E58 + civ*0x20E4 + 0x10F8`).
   - HUD: `civ = Player_CivID`.
   - City production: `civ = city owner`.
   - When the return address is 0x5CC85D and the unit has `UTA_Army`, add the army overlay (entries 64..127 = owner's ntp 0..15 x4). This needs a recolor variant beside the existing `Recolor { army }` (map armies, 10.11, use different entries).
3. **Shadows**: only `Frame_1` is blitted (slot +0x8C, flags `[0x9C7304]`). Palette indices 248..254 darken the destination as on the map (10.5) [I: same jgl path]. Compile the frame with `UnitArt` (not `UnitShadow`).
4. The units_32 fallback and the garrison icons need nothing new.

### 16.10 Text colours

Why the button letters (minimap "G", unit box tabs "D"/"E"/"I", city-view "Wealth") are traced as `color 0 0 255` but show black: they are not blue. Their COLORREF is **`DIBINDEX(0)` = 0x10FF0000**. `inproc::gdi::dc_text_color` drops the top byte, which turns it into (0,0,255). Nothing recolours text afterwards. There is no mask surface and no LUT pass: every pass is one `TextOutA` straight into the canvas's 16-bit DIB section, with its own `SetTextColor` [X].

**Text state in `PCX_Image`** [X: setters 0x5FD240/0x5FD270/0x5FD2A0/0x5FD2D0 = styles 0..3]. `set_text_effects(color, shadow, dx, dy)` for style *s* writes `+0x6C+4s` = colour, `+0x7C+4s` = shadow colour, `+0x8C+4s` = shadow dx, `+0x9C+4s` = shadow dy. Fonts are at `+0x5C+4s` (`set_font` 0x5FD200 sets style 0). Plain strings (`+0x40 == 0`) use style 0. With markup, the style is `+0x30`, falling back to 0 when that style's colour is -1.

**Drawing, 0x5FD980** (reached from 0x5FE7B0 and the other text entry points) [X]:
1. **Palette choice.** The canvas format comes from JGL_Image vt+0xE4. On a **16-bit** canvas it takes `pal = [[0xCAAE34]+4]` and sets flag *f*. On other canvases `pal` stays 0. `[0xCAAE34]` is set to the static `PCX_Color_Table` 0xB428F8 at 0x56C3D4. Nothing ever loads that table: its only references are the ctor/dtor 0x56BEB0/0x56BED0 and this read, and the ctor 0x5FBE80 sets +4 = 0. So **`pal` is always NULL** in practice, and the canvas palette that Button draw sets (0x5FFDF0 with 0xCAAE2C) has no effect on text.
2. **Shadow pass**, if `shadow != -1`, and not `shadow == 0xFF` when *f* is set: set colour `shadow` (decoded as below), then `TextOutA(x + dx, y + dy)` (JGL_Image vt+0xB8).
3. **Main pass**: set colour `+0x6C[s]`, then `TextOutA(x, y)`.
4. **Colour decoding** (same for both passes):
   - **`c < 0`** (bit 31 set): packed high-colour RGB. If `[0xCAB28C]`->vt+0xB4() == 1 it is 565: `r = (c>>8)&0xF8, g = (c>>3)&0xFC, b = (c&0x1F)<<3`. Otherwise it is 555: `r = (c>>7)&0xF8, g = (c>>2)&0xF8, b = (c&0x1F)<<3`. The result goes to jgl vt+0xB4 (0x10001D70) = `SetTextColor(RGB(r,g,b))`. Almost all interface text uses this path: e.g. 0x80007FFF gives (248,248,248), and 0x80000000 gives black.
   - **`c >= 0`**: a palette index. If `pal` is set, it is looked up through `pal`->vt+0x20, which never happens on 16-bit canvases (step 1). If `PCX_Image+0x20 & 4`, no colour is set at all and the DC keeps its previous colour. Otherwise it goes to jgl vt+0xB0 (0x10001DB0) = **`SetTextColor(0x10FF0000 | (c & 0xFFFF))`**, i.e. `DIBINDEX(c)`. jgl's vtable 0x10068238: +0xB0 = 0x10001DB0, +0xB4 = 0x10001D70, +0xB8 = 0x10001DE0 (TextOutA). jgl never calls `SetTextColor` any other way.
5. `DIBINDEX` on a 16-bpp DIB section, which has no colour table: Wine (`make_rgb_colorref`/`get_pixel_color`) writes pixel value **0**, whatever the index. For index 0, Windows gives the same result. In mk5-game.png the "D"/"E"/"I"/"G" glyph pixels are exactly #000000, with no second shade and no shadow [X]. For indices other than 0, Windows may write the raw index as the pixel value [?]. No such index appears in the traces (all 118 `0 0 255` runs have index byte 0).

**Where the buttons get index 0** [X]. In `Button` (vtable 0x6656C0), the canvas is at `Button+0x274`. The create path (around 0x60B05B) sets the text styles from globals. Style 0 is `(0x72FFB4 = 0, 0x72FFC0 = -1, 0x72FFCC = 1, 0x72FFD8 = 1)`: colour **index 0**, no shadow. Styles 1 and 2 have colour -1, so they fall back to style 0. The font comes from 0xCAD460. Button draw 0x60A010 creates the 16-bit canvas (0x60A13E). Then, if the label `+0x5F4` is not empty (0x60A858):
- If the per-state override `Button+0x63C + 4*state` (state = `+0x634`) is >= 0, it calls `Button_set_text_color` 0x60B0E0 `(color, -1, 1, 1)`. That function sets palette 0xCAAE2C on the canvas, then calls 0x5FD240. The ctors 0x609E70/0x609F50 set the overrides to -1, and nothing in .text writes them later.
- The owner's `+0x130`->vt+0x14 can take over the drawing.
- The label is drawn with 0x5FF720 `(str, x, y, w)` when it fits, else with 0x5FEEF0 `(str, RECT*, len)` plus "..." (0x684EEC).

Some script/dialog forms (0x611F7E, 0x612334, 0x612793) call 0x60B0E0 with a byte index (`form+0x2050`). These also go through `DIBINDEX`, so they are black on 16-bit canvases.

**Other colours in the traces.** 248/248/248, 0/0/0, 64/64/64, 0/112/184 and 24/224/240 cannot be DIBINDEX values, which always decode as (index, 0, 255). They are plain RGB from the packed path, so they are already right.

**Recipe for the mirror.**
1. Simplest fix, in the `TextOutA` hook: keep the full COLORREF from `GetTextColor`. If `c >> 16 == 0x10FF` (DIBINDEX) and the DC's bitmap is 16-bit (every UI canvas), draw the text black (0,0,0), i.e. pixel 0. Otherwise use `c & 0xFFFFFF` as RGB. Do not decode the top byte as part of the colour (`inproc::gdi::dc_text_color`). PALETTEINDEX (0x01xxxxxx) never occurs.
2. No shadow or effect handling is needed beyond what is traced. A shadow is a separate, earlier `TextOutA` at `(x+dx, y+dy)` with its own colour, so the hook already records it. Buttons have none.
3. Semantic alternative, if the colour is wanted before `TextOutA`: at 0x5FD980 entry (ECX = `PCX_Image*`), read `s` (0, or `+0x30` with the -1 fallback), `+0x6C[s]`, `+0x7C[s]`, `+0x8C[s]`, `+0x9C[s]` and the canvas depth, then apply the decoding in step 4 above. On a 16-bit canvas, `c >= 0` means black, and `c < 0` means RGB555/565.

### 16.11 Preferences screen

The Preferences screen is a form class of its own with a single, static instance; the title menu and the game's menu open the same object [X, static disassembly only]:

| What | PCG | Notes |
|---|---|---|
| vtable | **0x66E628** | `Base_Form` derived: +0x0C 0x602340 (create canvas), **+0x58 m22 = 0x587B50** (draws it), +0x13C m79 0x602710, +0x140 m80 0x608390 (the shared ones). The embedded canvas at +0x274 gets vtable 0x66E614 |
| instance | **0xC72888** | constructed by 0x5877C0 (its only caller 0x587795 is a static initialiser: `mov ecx, 0xC72888`) |
| open | 0x588370 `(this)` | callers 0x4E05E8 and 0x4E1908 (`mov ecx, 0xC72888`). Creates a canvas of the screen's size (`0x9C72F8` x `0x9C72FC`), loads `art\interface\x_pref background.pcx` (string at 0x72D400, through `BIC_get_asset_path` 0x5982A0; found in `civ3PTW/Art/interface`, 1024x768) into a sprite at `this+0x574` (0x5F7FB0) |
| draw | m22 0x587B50 | `Sprite_draw(this+0x574, canvas, (screen_w - bg_w) / 2, (screen_h - bg_h) / 2)`: the background **centred** on the screen-sized canvas. Then the title (font size 0x18) centred at background x+512, y+30 (0x5FF1E0), section headings (size 0x14) at x+100, y+115 and y+317 (0x5FEE50) |

The background (1024x768) has three dark panels with the checkboxes baked in (left column at x 110, then 405 and 665; rows 22 pixels apart from y 148), a combo field at (720, 225)-(870, 247), three slider bars at (259, 551), (259, 582), (259, 613), each 408x23 (a dark one-pixel border, inside (78, 78, 62)) (the slider pieces in `SCROLL.PCX` are listed there as "PREFERENCE SCREEN: 269, 552 / 583 / 614"), and the OK button in the bottom-right corner (about 953..1015 x 713..755). The light inner frame spans about (49, 79)-(973, 690); the band above it (y 0..77) holds only the title.

**C3X's info button** [C: `patch_Parameters_Form_m68_Show_Dialog`, `activate_mod_info_button`, R28]. C3X adds a `Button` (id 0x222003) to the screen each time it opens: 102x17 at background (891, 31), i.e. screen `((W − 1024) / 2 + 891, (H − 768) / 2 + 31)`, its three states cut from `art\civilopedia\descbox.pcx` at `(1 + 103·n, 1)`, label `C3X Info`. Pressed, it opens the generic popup (vtable 0x666408) with text key `C3X_INFO` (version, configuration files loaded), modal over the screen, its frame drawn onto the screen like every popup's. While a popup is the modal form over the screen, the bridge's panel leaves the input to the game, and our tabs are drawn under that popup and its frame.

Which screen is open: a composed form with vtable 0x66E628 (`ui::Frame::forms`), and where: the draw of `x_pref background.pcx` among its draws (its screen position minus its source offset is the background's top-left), else the form's canvas centred. The bridge draws its own tabs ("Resolution", "Layout") over this screen (`civ3_bridge::prefs`).

### 16.12 Screens and HUD buttons the viewer places

The viewer places some of the game's screens in one of its views and keeps the corner buttons in the view the pointer is in (`scene::Interface`). The bridge identifies them in the composited forms (`ui::Frame::forms`: vtable, canvas, origin, size), from the static code of the PCGames build (`Civ3Conquests-Unmodded.exe`, `.rdata` 0x665000/0x1A400) [X] and C3X's `civ_prog_objects.csv` (GOG, Steam, PCG columns) [H]. GOG's vtables equal PCGames' (the csv gives the same vptrs for every `.rdata` entry); GOG's globals from 0x7409C0 to 0xCAA2F0 are PCGames' + 0x40 [H]. No GOG or Steam executable was at hand: their values rest on the csv deltas. `civ3_sys::ui_hooks::screen_forms` and `hud_buttons` hold them.

**Function keys** [X]. `Main_Screen_Form` m82 (vptr 0x66B520) is 0x4E0670; it handles Tab and passes the rest to 0x6087D0. The F-keys are in `handle_key_down` 0x4E06B0: at 0x4E0AEE `sub vk,0x70` and a jump table at 0x4E2248 (F1-F11).
- F1-F6 (0x4E0B56-0x4E0BDD): `mov ecx,0x9AFD58; push kind; call 0x49D100` (`Advisor_GUI_open`), kind 0-5: domestic, trade, military, foreign, cultural, science. Shift+F1 calls 0x4E3740 instead. The `Advisor_GUI` (vtable 0x6688B8, canvas 0x9AFFCC) is the advisors' common background; each advisor is a form of its own class, composited with it.
- F7: `mov ecx,0xCA7C90; call 0x5D07C0` (Wonders of the World; 0x5D0440 loads `art\Advisors\wonders_background.pcx`, `WondersBox.pcx`, `WONDER_WIN`). Not vtable 0x66FE08: that is the wonder-completion splash (`art\wonder splash\wonderBackground.pcx`).
- F8: `mov ecx,0xB30208; call 0x545F10` (Histograph; `art\histograph-top5_final.pcx`).
- F9: `mov ecx,0xB6DDF8; push 0xB71248 + player·0xB0; call 0x57C600` (palace view; `text\PalaceView.txt`, `art\PalaceView\`, `PALACE_INSTRUCTIONS`), skipped while byte [0x9C5B02] is set.
- F10: `mov ecx,0xC90E40; push 0; call 0x5A3610` (spaceship). F11: `mov ecx,0xA32B90; call 0x4FD0E0` (demographics).

Each global screen is built by a static initializer (`mov ecx,obj; call ctor`); the constructor writes the vtable to `[esi]` and the canvas' own vtable to `[esi+0x274]`:

| Screen (settings name) | vtable (PCG = GOG) | object PCG | Steam vtable | evidence |
|---|---|---|---|---|
| Domestic advisor (F1) | 0x66C270 | (in `Advisor_GUI`) | 0x68935C | csv `Advisor_Base_Form_domestic_m95` vptr − 0x17C (slot 95) |
| Trade (F2) | 0x66F750 | | 0x68C80C | csv, as F1 |
| Military (F3) | 0x66DF50 | | 0x68B03C | csv |
| Foreign (F4) | 0x66C930 | | 0x689A20 | csv |
| Cultural (F5) | 0x66BBC0 | | 0x688CB8 | csv; §16.5 |
| Science (F6) | 0x66F420 | | 0x68C4DC | csv |
| Wonders (F7) | 0x66FF98 | 0xCA7C90 | ? | ctor 0x5D0210 (vtable write 0x5D0289) |
| Histograph (F8) | 0x66D380 | 0xB30208 | ? | ctor 0x541CA0 (0x541D2B) |
| City screen (City) | 0x6658B0 | 0x7409C0 | 0x682914 | csv `City_Form_m82` vptr − 0x148; traces |
| Palace view (Palace) | 0x66E0F8 | 0xB6DDF8 | ? | ctor 0x57B020 (0x57B0BA) |
| Espionage (Spy) | 0x66C5E0 | 0xA422B8 | 0x6896D0 | csv `Espionage_Form_m22_Draw` vptr − 0x58 |
| Game menu (Menu) | 0x66DD80 | 0xB429F0 | ? | ctor 0x56C6E0 (0x56C720); traces: 364×266 |
| Civilopedia (Help) | 0x66B0A8 | 0x9E85B0 | 0x6881A8 | csv `Civilopedia_Form_m22_Draw` vptr − 0x58 |

The game menu is what the HUD's menu button opens: `Main_GUI_handle_button_press` 0x5577A0 (jump table 0x557DD0), button id 0x31 at 0x557A94: `mov ecx,0xB429F0; call 0x56C760`, a popup-derived global with text key `GMENU_MAIN` (`script.txt`) run modally (0x611460); its m22 is the generic popups' 0x613AE0. Ctrl+M opens the same (0x4E174E). Escape on the map does not open it: 0x4E08F3 calls 0x4DFEF0, which builds a popup on the stack (vtable 0x666408, the generic one) with key `REALLYQUIT`, so it cannot be told from other popups.

**HUD buttons** [X; offsets from `Civ3Conquests.h` compiled with `gcc -m32`]. `Main_GUI` is at 0x9FDB10 (GOG 0x9FDB50, Steam 0xA20350). Every button is a `Button` (vtable 0x6656C0, 0x6D4 bytes) with its own canvas at +0x274:

| Field | Offset | Canvas PCG | Button id | Position at 1920×1080 [D] |
|---|---|---|---|---|
| `Advisors_Btn` | 0x845C | 0xA061E0 | 0x33 | (94, 12) 34×30 |
| `Menu_Btn` | 0x8B30 | 0xA068B4 | 0x31 | (21, 12) 36×30 |
| `Geography_Btn` | 0x9204 | 0xA06F88 | 0x2A | (12, 1007) 15×15 |
| `Civilopedia_Btn` | 0x98D8 | 0xA0765C | 0x32 | (57, 12) 36×30 |
| `Unit_Command_Buttons[42]` | 0x16CE4 | 0xA14A68, every 0x6DC | | 32×32, two rows above the bottom middle |

The top-left corner's three buttons are menu, Civilopedia and advisors. The unit's orders are `Command_Button`s (0x6DC bytes: a `Button` and its command); the constructor builds the array at 0x558AA9 (`push 0x2A; push 0x6DC; lea ecx,[esi+0x16CE4]`, element ctor 0x558BC0, which only calls the `Button` ctor 0x609E70: no vtable of their own). Only the orders the unit has are composited (10 in the traces, x 836-1092, y 966-1062 at 1920×1080).

**Popups.** An open popup is the modal form (0xCAD384, `civ3_sys::modal`) when it is none of the screens above; its draws are those composited from it on (its children and what the game draws over it afterwards, e.g. the cursor), and, just before it, its frame: the border pieces and title band (`popupborders.pcx`, the caption text) its m80 draws straight onto the back buffer, on no canvas, unclipped (`ui_canvas_draws` gaps in a trace). The bridge hands a popup over the map's own screen to the viewer as a part of its own; one over another screen (an advisor, the Preferences screen with C3X's info box) stays with that screen.

**The city screen's production box** is its largest button of the city screen's own `Button` subclass (vtable 0x665F20; 114×95 at the screen's lower right, 1353,672 at 1920×1080): the bridge names it as the screen's main control (`scene::Interface::focus_control`), where the viewer puts the pointer when the screen opens while the mouse is captured.

**A screen as a part.** A named screen smaller than the window (the advisors and the other 1024×768 screens at higher resolutions; not the city screen) is handed to the viewer the same way, with what is drawn over it (`scene::Interface::screen_view`), so the viewer can move it out from under its zoom strip or box where there is room below.
### 16.11 Controls that take the mouse

What a pointer aid (the viewer's button snapping) needs: the screen rectangles of everything the mouse acts on. All [X] from `Civ3Conquests-Unmodded.exe` unless tagged; read by `civ3_sys::controls`.

**Hit test `0x609030`** `Base_Form* cdecl (int* x, int* y)`, screen coordinates in, the hit form's own coordinates out; `[0xCAD348]` = 1 when the point is on the form's frame. A mouse capture (`[0xCAD370]`, `[0xCAD374]`, `[0xCAD360]`, `[0xCAD364]`, set by a press) takes everything while it lasts. Otherwise, with a modal form `[0xCAD384]` only its tree, then the companion `[0xCAD380]`, are tested (a miss still goes to the modal form); without one, the roots `0xCACB10[count 0xCAD3C4]` from index 0 (topmost), first hit wins. The recursive test `0x608C70`:
- the form and every parent visible (Status2 bit 0, `0x609AC0`), else nothing;
- children first (the child array, from index 0); absolute children (Status1 without 0x20) are not clipped by the parent;
- framed forms (Status2 & 2): the outer rectangle `+0x1BC..+0x1C8` in parent coordinates, the frame pieces tested by pixel;
- the form's rectangle `+0x1AC..+0x1B8` (half-open, `0x60ECE0`);
- Status1 & 2: the form itself never takes the mouse (its children do);
- keyed forms (Status1 & 0x1000000, not & 0x100): a colour-key pixel of the canvas lets the click through.

No enabled flag is read on the mouse path: a disabled control is hidden, or its owner ignores the event [?]. Wheel `0x62C6D0` goes to the hit form's `vt+0x64(delta, x, y)`.

**Classes** (no RTTI; names from the C3X header [H] or matched by structure):

| Class | Vtable | What is clicked |
|---|---|---|
| `Button` [H] | 0x6656C0; city screen 0x665F20; base 0x671550 | the whole control. Pressed +0x614, hovered +0x5E4, forced state +0x634, id +0x5E8 |
| `Main_GUI` buttons [H] | Main_GUI 0x9FDB10 (vt 0x66B570): Advisors +0x845C, Menu +0x8B30, Geography +0x9204, Civilopedia +0x98D8, navigator [8] +0x9FAC, right panel [7] +0xD64C (stride 0x6D4), unit commands [42] +0x16CE4 (`Command_Button`, stride 0x6DC) | the buttons; the minimap (+0x13FF8) and unit box (+0x13FE8) are tested in code, inclusive RECTs |
| `Scroll_Bar` [H] | 0x6660A8 | its arrow `Button`s (+0x610, +0xCE4); pos +0x58C, min/max +0x580/+0x584 |
| list boxes | generic 0x6718E8; city 0x665D88, 0x665BF0, 0x665A40, 0x6665D8; combo dropdown 0x671FC8 | entries (below) |
| `CheckBox` [H] | form part (CheckBox+0x20) 0x671C70 | boxes: `idx = (x/(W+gap))·rows + y/(H+gap)`; item list at CheckBox+0x598 |
| `Context_Menu` [H] | 0x671AC0, 0x66B240 | rows: items +0x578 (28 bytes, +0xC bit 0 shown, bit 2 disabled), count +0x2184, height +0x218C; row = `(y − 2) / max(height, font line)` over shown items |
| popups [H] | 0x666240 family | OK +0x5DC, Cancel +0xCB0 (`Button`s); a choice list at +0x1468 |
| Advisor_GUI | 0x9AFD58 | six tabs tested in code (0x49D6D0): x 4..=0x44, y 242+62k..=304+62k |
| any form | — | text links (Civilopedia): 32-byte entries at +0x378, a RECT at +4, count +0x380 |

Not found: a text edit control (nothing handles WM_CHAR but cheat codes and scroll keys). Unnamed classes with an item list (0x666778, 0x666908, 0x671730, 0x671E10) are probably radio or tab groups [?]. Class 0x666908 is a control of one of several kinds by a type field [X, static]: its draw thunk (`sub ecx,[ecx-4]`, `jmp 0x619570`) dispatches on the type at `[ecx-8]` (ecx = F − [F−4]) through a jump table to `add ecx,imm; jmp`: type 2 a list box (`ecx − 0x1A8` = G + 0x64, draw 0x6181E0), type 8 a choice grid (`ecx − 0x8C`, draw 0x6156F0; object B = ecx − 0x8C − 0xE0, form part B + vb[1], item list L = B + vb[2] with vb = [B], count at L+0xD0; item k, drawn by 0x615080, at column k / rows: x = col·(W+gap), y = row·H + gap/2, W×H), type 16 rows of text, a popup's choices such as the quit confirmation's (`ecx − 0x184`, draw 0x61D7C0; B = ecx − 0x184 − 0x28, the same parts; each row drawn by 0x61CC20, as high as its lines (item node +8 data, its +8 the lines) times the font's line, at least H, one below the other) [X static; D the quit popup: form 0x666908, 314x52, typed draw 0x61D7C0].

**List boxes** use virtual inheritance: the form part F (in the form tree), the list object G, the item list L. A list's draw slot `F.vt[+0x58]` is a thunk `sub ecx,[ecx-4]` (`2B 49 FC`), maybe `sub ecx,k` (`83 E9 k`), `jmp 0x6181E0`; `G = F − [F−4] − k − 0x64`, `vb = [G+4]`, `F = G + 4 + vb[1]`, `L = G + 4 + vb[2]` (generic `0x60, 0x5D8`; combo `0x64, 0x5DC`; city `0x70, 0x5E8`). G: +0x2C first entry shown, +0x30 entries shown. L: +0x3C y0, +0x44 x0, +0x48 gap, +0x50 layout (1: one column of entries of several lines), +0x58 columns, +0x5C rows, +0x68 W, +0x6C row height H, +0xC8 first node (+8 data whose +0xC is its lines, +0xC next), +0xD0 count, +0xF0 selected, +0xF4 hovered. Entry `k` shown (0x617470, F's pixels): grid `col = k / rows`, `row = k % rows`, `x = x0 + col·(W+gap)`, width `W − 2·x0`, `y = y0 + row·(H+gap)`, height H; layout 1 stacks `(H+gap)·lines − gap` high. The wheel (0x618E40, only with F.Status1 & 8) moves the first entry by one per message, up for a positive delta, down while it is below `count − 1` (not stopped at the last page).

**Mouse cursor**: `Art\CURSOR.PCX` (scenario first), 23 cells of 32×32 at `(1 + 33k, 1)` into `Sprite[k]` at 0xA52200 + 0x2C·k (0x54AF90); hotspot from the 0xFB pixels in the row above and the column left of each cell (arrow cells (1, 1)), kept in `int[23]` 0xA521A0 (x) and 0xA52140 (y). The map's plain arrow is the system cursor. [X]
- Each form has a cursor: a sprite at `Base_Form+0x208` with its hotspot at +0x20C/+0x210 (set by 0x604350 `(Sprite*, hx, hy)`), else a system pointer (+0x218 an `HCURSOR`; 0x6043A0 `(IDC_*)` loads one). As the mouse moves, 0x606A70 takes the cursor of the form under it: a sprite becomes the screen's cursor (`Sprite*` 0xCAD2D0, hotspot 0xCAD2DC/0xCAD2E0, drawn at 0xCAD2D4/0xCAD2D8 = pointer − hotspot) and the system pointer is hidden (`ShowCursor(FALSE)` until the count is negative); otherwise 0xCAD2D0 = 0, the system pointer shows and `SetCursor(LoadCursorA(NULL, id))` sets it: the form's, else `IDC_ARROW`; on a form's frame the resize arrows (`IDC_SIZENWSE`, `IDC_SIZENESW`, `IDC_SIZEWE`, `IDC_SIZENS`).
- `present` (0x606680) draws the sprite cursor onto the back buffer last (`Sprite_draw(0xCAD2D0, 0xCACFF0, x, y)`); the area under it is saved in 0xCACD38 and put back (0x6069A0) before it moves.
- The map form (`Main_Screen_Form`) picks `Sprite[k]` by the action the player is choosing a tile for (e.g. 0x4DC5C0, 0x4D9520): k = 0 is the system arrow; after G (Go To) it is k = 1, a blank cell [D].
- While it works, the game shows the system's busy pointer: `GetCursor`, `SetCursor(LoadCursorA(NULL, IDC_WAIT))`, then the saved cursor back (0x4F5FB0, 0x5584A0, 0x56990C, 0x5918E8, 0x59A480, 0x5D1620, ...).
- What the viewer does with it: `civ3_sys::cursor` reads 0xCAD2D0 (a cell `k` or not), the bridge's hook on the executable's `SetCursor` import tells the system pointer; `scene::Cursor` carries the sprite set `Viewer@cursors/<k>` of `civ3.toml` (the sheet finds the hotspots from the marks, `anchor_marks`), the busy and resize pointers as the system's. The game's own sprite cursor is left out of the mirrored interface (it would lag behind the pointer).

---

## 17. Minimap

The minimap is generated by the exe from world state. No art file is involved apart from the HUD frame drawn over it. The object is `Navigator_Data` at **Main_GUI+0x10720 = 0xA0E230** [X]. The prototype `tools/minimap-proto.py` implements this section and reproduces the screenshots exactly (17.9).

All colour arithmetic below uses the **RGB555** path. The game queries the surface format (graphics vt+0xB4; 1 = 565, otherwise 555). The screenshots are 555, and every constant has a 565 twin that I list once. Channel helpers (555) [X]: `R(c) = (c >> 7) & 0xF8` (0x550880), `G(c) = (c >> 2) & 0xF8` (0x57AF60), `B(c) = (c << 3) & 0xFF`, `rgb(r,g,b) = (r&0xF8)<<7 | (g&0xF8)<<2 | b>>3` (0x48D210). Colours are written as `0x8000xxxx`. Bit 31 is a JGL flag; only the low 16 bits reach the pixels. Screen value of a 5-bit channel: `(c<<3)|(c>>2)` (6.7a).

### 17.1 Object layout [X offsets; H names]

| Off | Field | Use |
|---|---|---|
| +0x00 | `Main_PCX` | destination canvas. Set to 0x9FDD84 (Main_GUI canvas) by init |
| +0x04 | `PCX_0` | the "source" image: +0x2D0 (political, default) or +0x18 (terrain). The toggle 0x579E70 switches it |
| +0x08..+0x14 | `Rect` | left/top/right/bottom of the minimap on `Main_PCX` (right/bottom are not used for drawing) |
| +0x18 | `PCX_1` | **terrain image**: bilinear terrain colours (17.4) |
| +0x2D0 | `PCX_2` | **political image**: darkened terrain, owned land tinted, city squares |
| +0x588 | `PCX_3` | darkened terrain (PCX_1 averaged with grey) |
| +0x840 | `u16 (x,y)[w*h]` | pixel coordinate list; the cell lists point into it |
| +0x844 | `Navigator_Cell[Width*Height]` | one 0x18-byte record per **lattice** cell (both parities), index `Width*y + x`: +0 x, +2 y, +4..+0x10 vector of pointers to the pixels whose nearest tile is this cell, **+0x14 byte "not yet revealed"** (1 at init) |
| +0x848 | `per_col` | `w / Map.Width` (integer) |
| +0x84C | `ratio` | `Map.Width / Map.Height` (integer) |
| +0x850/+0x854 | | w, h before the aspect correction |
| +0x858 | `PCX_4` | **displayed image**: black, with revealed cells copied from `PCX_0` |
| +0xB10/+0xB14 | `w`, `h` | image size (all four images are `w x h`, 16 bpp, created by 0x578430) |
| +0xB18 | flags | bit0 "process the newly-visible list on the next draw" (set by init and after every main-map draw, cleared by the draw) |

Functions [X]: init 0x578050 `(w, h, bool tint)` `ret 0xC`; create images 0x578430; reset 0x5786F0 (= `Navigator_Data_reset` in the C3X csv); reveal pass 0x578970 `(bool all)`; territory update 0x578E00 `(x, y, r)`; tint one cell 0x578BA0 `(PCX_Image* dst, Navigator_Cell*, colour)`; tint all 0x579D90; city square 0x579090 `(cell)` / 0x5797B0 `(x, y)`; erase city 0x5799B0; re-tint/un-tint one cell 0x579290/0x579510; pixel->tile 0x57A5D0 (cdecl); tile->pixel 0x57A6D0 (cdecl); build terrain image 0x57A760 (cdecl `(PCX_Image*, w, h)`); **draw 0x57ACE0** `(PCX_Image* dst, RECT* r, flags)` `ret 0xC`; viewport rectangle 0x579F20; terrain/political toggle 0x579E70; click -> centre the map 0x57AF00 `(px, py)` (calls MSF 0x4DFAA0 with the tile from 0x57A5D0).

### 17.2 Size and position [X formulas; D values for 1920x1080]

The HUD box is a RECT at **Main_GUI+0x13FF8** (canvas coordinates). It is set by the layout and is runtime-only. Main_GUI first erases it (0x602250) and derives the inner rect **Main_GUI+0x13F38** = box shrunk by 26 px horizontally and 12 px vertically (0x553730(rect, 0x1A, 0xC)). The draw (0x553470) fills that inner rect black. Init with `w, h < 0` (the Main_GUI call 0x557613 passes `-1, -1, 1`) computes:

```
w = (box.R - 26) - (box.L + 26)                 // = box width - 52
h = (box.B - 12 + 1) - (box.T + 12)             // = box height - 23
ratio = Map.Width / Map.Height                  // lattice dims (Width = 2 * tiles per row), C int division
if ratio > 1:  h = h / ratio;  dy = (h_orig - h) / 2     // letterbox
if ratio < 1:  w = w / (Map.Height / Map.Width); dx = (w_orig - w) / 2
if w odd: w += 1                                // the builders walk pixels linearly (stride = 2w)
Rect.left = box.L + 26 + dx;  Rect.top = box.T + 12 + dy
per_col = w / Map.Width
```
When `ratio != 1`, Main_GUI also draws two white separator lines in the letterbox (0x5535A4..0x5536BB): for `ratio > 1`, horizontal lines at `top + d - 1` and `bottom - d` with `d = (h_orig - h)/2`; for `ratio < 1`, vertical lines at `left + d - 1` and `right - d`. Both span the inner rect and are white 0x7FFF [X; not exercised by the traces, every traced map has ratio 1].

At **1920x1080 [D]** (fitted over 28 screenshots; also the only geometry with exact matches): `w x h = 230 x 108` (ratio 1), minimap pixel (0,0) at **screen (31, 961)**. So the HUD box is screen (5, 949)-(287, 1080). I did not find statically how the box, or the Main_GUI form origin, depends on the resolution [?]. Read Main_GUI+0x13F38 and `local_to_screen` (16.5) at runtime.

`PCX_4` is copied **opaque** (0x5FCB30, `copy_rect(dst, 0, 0, Rect.left, Rect.top, w, h)`, at 0x57AED0) onto the Main_GUI canvas. Then the viewport rectangle is drawn (17.7). Afterwards Main_GUI draws the HUD frame over it: alpha-mask blit 0x5F8310 of sprite Main_GUI+0x14AD0 with mask +0x14AFC, background = the map canvas 0x9F8934 (0x5536FD) [X]. At 1920x1080 the frame covers the minimap's **last 3 rows and last 2 columns** plus a few corner pixels (921 pixels with the standard frame, 899 with the WW2 frame) [D].

### 17.3 Pixel <-> tile mapping (0x57A5D0, 0x57A6D0) [X]

The minimap is the main map's lattice **scaled, not rotated**: lattice x spans `w` pixels, lattice y spans `h` pixels over `Height - 1` rows. Pixel -> tile rounds to the nearest tile in diamond distance:

```
D = 2*w*h
s1 = Width*px*h - (Height-1)*py*w + w*h ;  fx = s1 % D ; if s1 < 0: s1 -= D ; A = s1 / D     // C division
s2 = Width*px*h + (Height-1)*py*w + w*h ;  fy = s2 % D ;                     B = s2 / D
x = A + B ; y = B - A ; if x >= Width: x -= Width ;  if fx < 0: fx += D
// returns 1 (invalid) if x or y is outside the map
```
i.e. with `X' = px*Width/w`, `Y' = py*(Height-1)/h`: `u = floor((X'-Y'+1)/2)`, `v = floor((X'+Y'+1)/2)`, tile `(u+v, v-u)`. `(fx, fy)` are the position inside the rotated cell (0..D). Every minimap pixel is appended to the `Navigator_Cell` of its tile (row-major, 0x578430). Tile -> pixel (0x57A6D0, also inlined in the city and rectangle code): `px = x*w/Width`, `py = y*h/(Height-1)` after wrapping x/y.

### 17.4 Terrain image `PCX_1` (0x57A760) [X]

For each pixel (row-major; the out pointer only advances on valid pixels):
```
(x, y, fx, fy) = pixel_to_tile(px, py)
if fx < D/2: fx += D/2; x -= 1; y += 1   else fx -= D/2
if fy < D/2: fy += D/2; x -= 1; y -= 1   else fy -= D/2
for i in 0..3:                                   // corners (x,y) (x+1,y-1) (x+1,y+1) (x+2,y)
    xx = wrap_h(x + (i+1)/2);  yy = wrap_v(y + i/2 - (i&1))    // 0x426C80/0x426CC0
    c  = off-map ? (Map.Flags & 3 ? white 0x7FFF : black 0) : TERRAIN[tile.m50()]
    wx = (i&1) ? fx : D - fx ;  wy = (i>>1) ? fy : D - fy
    r += R(c)*wx/D*wy/D ; g += G(c)*wx/D*wy/D ; b += B(c)*wx/D*wy/D      // unsigned, truncating each step
pixel = rgb(min(r,255), min(g,255), min(b,255))
```
So it is a bilinear blend of the four surrounding tile colours, keyed by the **effective terrain** (`Tile::m50_Get_Square_BaseType` = `SquareType` bits 12..15). On wrapping maps the off-map samples at the top and bottom edge are **white**. On non-wrapping maps they are black [X; D: the wrap maps cr1/sp*/nat match exactly].

Terrain colours (switch at 0x57A90D, jump table 0x57ACA4) [X; D]. "Darkened" = the `PCX_3` value, i.e. what an explored, unowned, single-terrain area shows:

| id terrain | 565 | **555** | RGB (screen) | darkened RGB |
|---|---|---|---|---|
| 0 desert | 0xF7B7 | 0x7BD7 | (247,247,189) | (148,148,123) |
| 1 plains | 0xD56A | 0x6AAA | (214,173,82) | (132,115,66) |
| 2 grassland | 0x8CA6 | 0x4646 | (140,148,49) | (99,99,49) |
| 3 tundra | 0xDF5D | 0x6FBD | (222,239,239) | (140,148,148) |
| 4 flood plain | 0x61A2 | 0x30C2 | (99,49,16) | (74,49,33) |
| 5 hills | 0x3326 | 0x1986 | (49,99,49) | (49,74,49) |
| 6 mountains | 0xCE59 | 0x6739 | (206,206,206) | (132,132,132) |
| 7 forest | 0x4BC6 | 0x25E6 | (74,123,49) | (66,90,49) |
| 8 jungle | 0x1AE0 | 0x0D60 | (24,90,0) | (41,74,24) |
| 9 swamp | 0x63CB | 0x31EB | (99,123,90) | (74,90,74) |
| 10 volcano | 0x6801 | 0x3401 | (107,0,8) | (82,24,33) |
| 11 coast | 0x2515 | 0x1295 | (33,165,173) | (41,107,115) |
| 12 sea | 0x1C11 | 0x0E11 | (24,132,140) | (41,90,99) |
| 13 ocean | 0x026B | 0x012B | (0,74,90) | (24,66,74) |
| other | | 0 | black | |

### 17.5 Darkened and political images [X]

- `PCX_3` = `PCX_1` averaged per channel with grey `0x1CE7` (565 `0x39E7`, 8-bit (56,56,56)): `rgb((R(c)+56)>>1, (G(c)+56)>>1, (B(c)+56)>>1)` (0x578430, 0x5786F0).
- `PCX_2` starts as a copy of `PCX_3`. **Territory** (0x579D90 at init; 0x578E00 on change): for every tile with `0 < owner < 32` (`Tile::m38` = `Territory_OwnerID`, Tile+0x05, **no visibility test**), every pixel of its cell **that is not water** (`Tile::m35_Check_Is_Water`: coast/sea/ocean keep the darkened colour) becomes `rgb((3*R(k) + R(p)) >> 2, (3*G(k) + G(p)) >> 2, (3*B(k) + B(p)) >> 2)`, where `p` is the **bright** `PCX_1` pixel and `k` the owner colour. In 0x578E00 an unowned tile's cell is restored from `PCX_3`.
- Owner colour `k` = `Leader 0x55A350` -> `Race 0x539E20(Color_Table_ID)` = **entry 66 (0x42) of `Art/Units/Palettes/ntpNN.pcx`**, `NN = leaders[owner].Color_Table_ID` (+0x10F8), converted to 555 (the same colour as the team disc and the unit-path line, section 10) [X; D]. The colour-table array is 0x73DA04. Scenario `Art/Units/Palettes` override it through the asset search path [I]. Values (palette entry 66 -> screen RGB):

| NN | screen RGB | NN | screen RGB | NN | screen RGB | NN | screen RGB |
|---|---|---|---|---|---|---|---|
| 0 | (231,231,231) | 8 | (123,0,156) | 16 | (57,57,57) | 24 | (198,214,148) |
| 1 | (231,8,8) | 9 | (148,107,0) | 17 | (198,0,255) | 25 | (115,255,173) |
| 2 | (255,140,0) | 10 | (123,222,0) | 18 | (206,181,0) | 26 | (189,206,66) |
| 3 | (231,231,0) | 11 | (148,8,8) | 19 | (123,0,255) | 27 | (173,132,82) |
| 4 | (0,115,0) | 12 | (148,148,148) | 20 | (173,222,239) | 28 | (107,82,8) |
| 5 | (24,222,222) | 13 | (0,33,148) | 21 | (115,132,49) | 29 | (74,90,41) |
| 6 | (49,99,255) | 14 | (8,132,148) | 22 | (255,198,0) | 30 | (255,198,255) |
| 7 | (255,74,239) | 15 | (247,198,255) | 23 | (255,107,0) | 31 | (198,165,239) |

- **Cities** (0x579090, called for each revealed cell that has `city_at(x,y)` (0x56D230, any owner), and by the city create/capture handlers 0x58B6AE.. via 0x5797B0): `s = clamp(per_col >> 2, 1, 4)`; centre `(cx, cy) = (x*w/Width + (per_col>>2), y*h/(Height-1) + (per_col>>4))`; every pixel `(cx-s..cx+s, cy-s..cy+s)` with `0 < px < w` and `0 < py < h` (**strictly > 0**) of `PCX_2` becomes **white 0x7FFF**, and that `(2s+1)^2` square is copied from `PCX_2` to `PCX_4` (0x5FCB60) when `PCX_0 == PCX_2`. For every traced map (`w = 230`, Width 60..256) this gives a **3x3 white square** centred on the city's pixel. City squares are not owner-coloured and do not depend on size or capital status. City removal: 0x5799B0 restores the square from `PCX_1` averaged with grey.
- **Units are not drawn** on the minimap. Nothing in 0x578050..0x57AF90 reads units [X].

### 17.6 Revealing: which tiles are shown [X]

`PCX_4` starts filled with `0x80000000` = black (0x5FFF30). Reveal pass 0x578970 (from init/reset, and with `all = 0` from 0x4FA600/0x56E511): for each tile `(x, y)` (`x+y` even, row-major) whose **`Tile.Body.Fog_Of_War` (Tile+0x58) has bit `leaders[Player_CivID].ID`** (the explored mask, `Civ3Tile::explored`; read through C3X's `Map_get_tile_to_check_visibility` replacement at 0x5789D7, which handles shared hot-seat vision): if `all` or the cell's "not revealed" byte is set, copy the cell's pixels from `PCX_0` to `PCX_4`, clear the byte, and draw the city square if there is a city. **There is no fog distinction**: explored = full colour, unexplored = black. Current visibility (`FOWStatus`/`Visibility`/V3) is not read. The territory tint uses the live `Territory_OwnerID` of every explored tile, not a remembered owner.

Debug: if `p_debug_mode_bits & 8` (0xA52640, "reveal map"; cleared to `& ~0xC` when 0x47B5B0 returns true) or draw flag 0x10, the draw blits `PCX_0` instead of `PCX_4` (whole map, no black).

### 17.7 Viewport rectangle (0x579F20) [X; D]

Drawn on `Main_PCX` after the blit, every time the minimap is drawn, unless draw flag bit 0 is set. Colour **white** 0x7FFF (565 0xFFFF). One pixel wide. The canvas clip is reset while drawing and restored afterwards. Inputs are the **MSF tile range** `TileX_Min/Max`, `TileY_Min/Max` (0x9FD530/34/38/3C, section 3; one tile larger than the screen on each side):
```
xa = TileX_Min, ya = TileY_Min, xb = TileX_Max, yb = TileY_Max
non-wrapping axis: clamp to [0, Width-1] / [0, Height-1];  wrapping axis: wrap (twice, one step each)
if !in_map(xa,ya) or !in_map(xb,yb): draw nothing
x0 = xa*w/Width ; y0 = ya*h/(Height-1) ; x1 = xb*w/Width ; y1 = yb*h/(Height-1)
L = Rect.left, T = Rect.top ; lines have inclusive endpoints (hline 0x5FFFE0, vline 0x600010)
x0<=x1, y0<=y1 : outline (L+x0, T+y0) .. (L+x1-1, T+y1-1)         (0x600940, right/bottom exclusive)
x0> x1, y0<=y1 : split at the x wrap: vline(L+x1-1, T+y0..T+y1-1), hline(L..L+x1-1, T+y0), hline(L..L+x1-1, T+y1-1),
                 vline(L+x0, T+y0..T+y1-1), hline(L+x0..L+w-1, T+y0), hline(L+x0..L+w-1, T+y1-1)
x0<=x1, y0> y1 : hline(L+x0..L+x1-1, T+y1-1), vline(L+x0, T..T+y1-1), vline(L+x1-1, T..T+y1-1),
                 hline(L+x0..L+x1-1, T+y0), vline(L+x0, T+y0..T+h-1), vline(L+x1-1, T+y0..T+h-1)
both wrapped   : the four corner pieces: hlines (L..L+x1-1) and (L+x0..L+w-1) at T+y0 and T+y1-1,
                 vlines at L+x0 and L+x1-1 over (T..T+y1-1) and (T+y0..T+h-1)
```
[D: bc1 `TileX 27..59, TileY 0..35`, 140x140 map -> screen x 75..126, y 961..987, exactly as computed.] No blinking: nothing in the minimap code reads a timer or a frame counter [X].

### 17.8 When it is (re)built [X]

The images are **retained**. They are not regenerated per frame.
- **Init** 0x578050 (game start/load: Main_GUI 0x557613, 0x494F16, 0x495053, 0x495104, 0x4F6322; other forms with their own `Navigator_Data`: 0x58C8F1 (426x202), 0x593212..): sizes, cell lists, all images, tint (if arg3), reveal all explored.
- **Reset** 0x5786F0 (0x47901E player switch, 0x494F20, 0x49510E, 0x4F5B7A, 0x4FA60B, 0x56E51C, 0x59322A, 0x593266): rebuilds `PCX_1/3/2/4`, marks every cell unrevealed, tints, then reveals.
- **Territory/terrain change** 0x578E00(x, y, r) over lattice `x-2r..x+2r`, `y-2r..y+2r` (callers 0x4AE68E, 0x4AF30A, 0x4B0D62 (culture/borders), 0x565301, 0x5D29A2/0x5D2A11): re-tint or restore each tile's cell in `PCX_2`. For already revealed cells it copies to `PCX_4` and redraws the city square. A city square that reaches into a neighbouring cell can be partly overwritten by that cell's re-tint (history-dependent) [I].
- **City events** 0x58B6AE.. (0x5797B0 / 0x5799B0 / 0x579290 / 0x579510): add/remove squares, re-tint single cells.
- **Newly explored tiles**: after every main-map draw (0x4EEEBB, 0x4EF0A3) and in 0x494C04/0x55B2DB, `+0xB18 |= 1` and Main_GUI is redrawn (`m73` 0x6026B0). The next draw 0x57ACE0 walks the vector at 0xA26860..0xA26864 (the tiles whose visibility changed, filled by the map/fog code) and reveals each still-unrevealed cell (copy + city). Then it clears `+0xB18`.
- **Draw** 0x57ACE0 runs on every Main_GUI redraw (0x553470 -> 0x5536D5; 0x5579A0 after the toggle; 0x58E0C8 etc. for the other forms with flags 0x11). It blits `PCX_4` and draws the rectangle. So the rectangle follows the camera on every map redraw.
- Toggle 0x579E70 (Main_GUI command handler 0x557979, a HUD minimap button [I which one]) swaps `PCX_0` between `PCX_2` and `PCX_1` and recopies every **non-zero** pixel of `PCX_4` from the new source. Terrain mode therefore shows bright untinted terrain without city squares (squares already copied stay white) [X].

### 17.9 Verification [D]

`tools/minimap-proto.py TRACE GAME.png [--diff out.png]` builds `PCX_1 -> PCX_3 -> PCX_2 -> PCX_4` from a civ3-bridge trace (`tile`: `SquareType`, owner, explored; `city`; `color_tables`; `map` flags; `view` tile range; `player`), adds the rectangle and compares with the screenshot. It assumes the init state, i.e. no history effects from 17.8. Results with `w x h = 230 x 108` at screen (31, 961):
- 28 screenshots (bc1-3, bd1-4, cr1, nat, off, sel, sp1-5, lm1, live2, u1, u2, txt2, ww1-7). Maps: 60x60 wrap X (flags 0x5), 60x60 wrap X+Y (0x3), 136x120, 140x140, 160x160 wrap X, 256x256. **Every pixel not covered by the HUD frame matches exactly.** The only mismatches are the frame pixels: 921 per shot (899 with the WW2 frame). That is 96.29 % / 96.38 % of the 24,840 minimap pixels, and **100.00 %** of the uncovered ones.
- `act` has no HUD in its screenshot (the minimap is not on screen) and was excluded.

### 17.10 Open points

- How the HUD box (Main_GUI+0x13FF8) and the Main_GUI origin depend on the screen resolution. Only 1920x1080 was measured [?].
- Letterbox lines and aspect correction for non-square maps (`Width/Height != 1` after integer division, e.g. 200x100) are [X] only, not checked against data.
- History effects (city squares partly overwritten by later re-tints, owner changes on revealed cells while `PCX_0` is the terrain image) cannot be reproduced from a single snapshot. Rebuilding from scratch matches a freshly loaded game.
- Which HUD button triggers the terrain/political toggle, and whether the choice persists (it is not saved in the Navigator object) [?].

---

## 18. C3X districts [C]

C3X (release 28 preview, `d4e71c4`) adds districts, wonder districts and natural wonders: buildings on the map that belong to a tile rather than a city. It draws them by replacing `Map_Renderer::m12_Draw_Tile_Buildings` and hooking a few more layers. This section is read from its source, `injected_code.c` (line numbers below are that file's), and has not been compared with the game's output yet [C]; the open points are in 18.11. The port is `crates/civ3-bridge/src/districts.rs` and `districts/`; the choices that depend on the neighbours (bridge, canal, port, Neighborhood) are checked against C3X's own functions, cut out of its source, over every combination of their inputs by `tools/district-oracle`. Without C3X districts, or with them switched off (`enable_districts`, `enable_natural_wonders`), nothing here applies and the game draws as in 6.7b.

### 18.1 What C3X changes in the map drawing [C]

Its `Map_Renderer_m19` wrapper (27258) records the tile being drawn and its district for the hooks below.

| Hook | Effect |
|---|---|
| m12 tile buildings (40493) | no district on the tile: the game's m12. A district: if its kind is `draw_over_resources`, the tile's resources are drawn first (40515); then `draw_district_on_tile` (40276). So **a tile with a district shows none of the game's tile buildings** (ruins, airfield, mine, barricade, fortress, goody hut, barbarian camp, colony, radar, outpost, victory marker), finished or not. The one exception: with `enable_districts` off and natural wonders on, the game's m12 is called for every district but the natural wonders (40489). |
| m09 resources (40522) | with a district on the tile and not `draw_over_resources`: `draw_district_generated_resource_on_tile` (40112) draws the resources in the game's place (after pollution, before the city); drawn over resources, m09 does nothing (40576). No district: the game's. |
| m11 irrigation (40588) | not drawn on a completed district whose kind `allow_irrigation_from` (an irrigation source). |
| m52 roads and railroads (27404, 27426) | not drawn on a Bridge district (`enable_bridge_districts`), finished or not: the bridge art has them. |
| river special (40609) | the four `Tile_has_city` calls of the river drawing (m15, 6.4; call sites in `civ_prog_objects.csv` 872-875) ask `Tile_has_city || district instance`. A district of any kind and state next to a river vertex makes it "special": drawn in pass 2, under the objects. |

### 18.2 State [C]

C3X's state is at the start of its PE section `.c3xdat` (`crates/civ3-sys/src/c3x.rs`; offsets generated into `c3x_layout.rs` from `C3X.h` by `tools/gen-civ3-layout`, valid only for the release they were generated from, checked by its `mod_version` and size). `district_tile_map` (tile pointer to `struct district_instance`, C3X.h:1673) holds every district: `state` (`DS_COMPLETED`), `district_id`, `tile_x/y`, `built_by_civ_id`, the wonder state (`wonder_info`: state, city id, wonder index) and the natural wonder id. The kinds are `district_configs[]` (ids 0-10 are the special ones: Neighborhood, Wonder District, Distribution Hub, Aerodrome, Natural Wonder, Port, Central Rail Hub, Energy Grid, Bridge, Canal, Great Wall; then the ones of `*.districts_config.txt`), their buildings `district_infos[].dependent_building_ids`, the wonders `wonder_district_configs[]` and `natural_wonder_configs[]`. `get_district_instance` (3879) ignores instances whose id is out of range. A district under construction counts as completed as soon as its tile has the mine the building worker makes (`district_is_complete`, 7399; it also marks it completed).

The bridge copies this into `Civ3World` (`districts`, `district_configs`, `improvements`, `free_improvements`; `Civ3Tile::district`, `city_area`, `continent`) once per snapshot, and the tracer dump has it as `district_*`, `wonder_kind`, `natural_wonder_kind`, `improvement`, `free_improvements` and `district` lines (parsed back by `Trace::parse`). The configs are read again only when a hash of their memory changes.

### 18.3 Art files [C]

`init_district_images` (39088) slices every sheet once, when first needed (the first district drawn). A sheet is `Districts/<season>/1200/<file>`: `<season>` is `Summer` unless the seasonal cycle is on (then the season of the moment, or of the clock), `1200` the hour of the day/night art. The file name is the one of the config, in its letter case (`Neighborhood_AMER.pcx`; the files on disk are `.PCX`, Windows does not mind). `get_mod_art_path` (19111) turns it into `Art\Districts/...` and asks `BIC_get_asset_path`, so a scenario's own file wins; else it is `<mod_rel_dir>\Art\...`, **in C3X's own folder** (`C3X_R28`, relative to `Conquests`; `DistrictConfigs::mod_dir`), which is not one of the game's art roots. `DistrictDraw::image` is `Art/Districts/Summer/1200/<file>`. Day/night and seasonal art (`DayNight/...`, `build_sprite_proxies`) swaps the sprites at draw time and is not modelled.

| Sheet | Cells |
|---|---|
| a district kind, one file per variant (`img_paths`) | `img_column_count` columns, one row per era if `vary_img_by_era` (4), else one; cells `custom_width` × `custom_height` (default 128 × 64) at `(w·column, h·row)` |
| `Abandoned.pcx` | land sprite at (0, 0), maritime one at (128, 0), both 128 × 64 |
| a wonder (`Wonders.pcx` if the config names none) | 128 × 64 unless the wonder has a size: `(w·img_column, h·img_row)`; the same for the building site and, if `enable_img_alt_dir`, the two alternative cells |
| a natural wonder | 128 × 88 at `(128·img_column, 88·img_row)` |

### 18.4 Which sprite, where [C]

`draw_district_on_tile` (40276). The tile's `Territory_OwnerID` is `owner`; `era` and `culture` are the owner's `Leader.Era` and `Races[RaceID].CultureGroupID`. A sprite's cell is `[variant][era][column]`: `variant` is the culture if `vary_img_by_culture` (else 0; the Port's sheet is chosen in 18.8, the Neighborhood's is always the culture), `era` the owner's if `vary_img_by_era` (else 0).

1. **Natural wonder** (40286): needs `enable_natural_wonders` only, no completion. The 128 × 88 cell at `(0, -24)` from the cell origin (bottom-aligned to the 64-high tile).
2. Other kinds need `enable_districts` and a completed district (40303).
3. **Owner** `> 0`: as above; a kind with `align_to_coast` goes through 18.8. **No owner** (`<= 0`): the district is **abandoned** (40332) unless it is a Wonder District, Natural Wonder, Bridge or Canal: `Abandoned.pcx` land cell, or the maritime one if the tile is water, at `(x_offset, y_offset)` of the kind (not its size). Those four draw with variant 0 and era 0; if the tile's owner is negative and the builder is known, with the builder's culture and era (40343).
4. The sprite of a kind is centred and bottom-aligned: `at = (x_offset - (w - 128) / 2, y_offset - (h - 64))` from the cell origin (40352).
5. Per kind (the switch, 40361): 
   - **Wonder District** (`enable_wonder_districts`): 18.9. 
   - **Neighborhood** (`enable_neighborhood_districts`): the culture's sheet whatever `vary_img_by_culture` says; column `hash(x, y) & 3` (40420): `v = x·0x9E3779B1 + y·0x85EBCA6B; v ^= v >> 16; v *= 0x7FEB352D; v ^= v >> 15; v *= 0x846CA68B; v ^= v >> 16` in 32 bits.
   - **Distribution Hub** (`enable_distribution_hub_districts`): column 0. 
   - **Energy Grid** (`enable_energy_grid_districts`): column 0, or `i + 1` for the first of its dependent buildings (in config order) that a city serving it has (39782). 
   - **Bridge** (`enable_bridge_districts`): 18.6. **Canal** (`enable_canal_districts`): 18.7. **Great Wall** (`enable_great_wall_districts`): 18.7.
   - every other kind (also Aerodrome, Port, Central Rail Hub): `render_strategy` **by-count**: column = how many of its dependent buildings some city serving it has (40243); **by-building**: column 0, then, for each dependent building `i` that a city has, column `i + 1` over it (40257).
6. Nothing is drawn for a kind that is switched off, and no tile building either (the game's m12 is not called).

**Which cities serve a district** (`tile_coords_has_city_with_building_in_district_radius`, 39331): none if the tile has no owner; for a kind of type `tile-improvement`, the city `Tile_Body.CityAreaID` names, if the tile is in its work area; else the cities of the tile's owner standing on tiles of that owner in the work area around it (`FOR_CITIES_AROUND`: the work area is `workable_tile_count` tiles, in the order of `cultural_ni_to_diffs`, 380; skipping polluted tiles and ones with an enemy unit). A city has a building (`has_active_building`, 17459) if `City_has_improvement(.., include_auto = 1)` (the city built it, or a wonder gives it to all of its owner's cities or those on its continent: `Leader.Auto_Improvements`, keys `id` and `(continent + 1) · ImprovementsCount + id`, §8), the building's government (`GovernmentID`) is none or the owner's, and the owner does not know its `ObsoleteID` tech.

**Zoomed out** (`draw_district_on_map_or_canvas`, 39770): C3X draws the same sprites with scale 2 but **does not scale its offsets**: `at` is the same number of pixels, in the 64 × 32 cell, so tall sprites sit higher than half their zoomed-in place. `districts::draws` gives the zoomed-in numbers (to be halved like the game's own art); `districts::draws_at(.., true)` gives what C3X passes [C, to be verified].

### 18.5 Offsets summary [C]

| Kind | Size | Offset | Sprite position |
|---|---|---|---|
| default | 128 × 64 | (0, 0) | the cell origin |
| Energy Grid, Great Wall, Municipal, Ski Resort, Water Park | custom height 88 | (0, 0) | `(0, -24)`; the Great Wall ignores it: its sprites are drawn at the cell origin (40101) |
| Bridge, Canal | 176 × 112 | (0, 24) | `(-24, -24)` |
| Abandoned | 128 × 64 | the kind's | the kind's offset only |
| Natural wonder | 128 × 88 | | `(0, -24)` |

### 18.6 Bridge [C]

Column (`get_bridge_image_index`, 39797): 0 SW-NE, 1 NW-SE, 2 N-S, 3 W-E; +4 if the bridge tile has a railroad (the real overlays). With bridge districts around (completed, any of the eight neighbours): two opposite ones decide (SW+NE, NW+SE, N+S, W+E, in that order), else one (NE or SW, NW or SE, N or S, W or E). With no bridge around, land decides the same way (any land, whoever owns it); none: SW-NE.

### 18.7 Canal and Great Wall [C]

**Canal** (`get_canal_directions`, 39889; `draw_canal_district`, 40046): sprites by direction (column = `enum direction`, 1 NE .. 8 N), era row. First direction: with canals around, the first of NE, NW, SE, SW, N, E, S, W that has a canal; else the first that has a canal or water. Second: the first in that order, other than the first, that has a canal (else canal or water), is not side by side with the first (N/NE, NE/E, ... NW/N), then the one higher on the screen goes first, so that the lower one is drawn in front (NE/NW/N, then E/W, then SE/SW, then S). Nine fixes for corner cases follow (40002-40010, e.g. no canal and water at NE and SW: NE and SW). Each sprite is drawn again 18 pixels sideways and 9 up or down (not for N and S: only the 9) towards a direction whose neighbour is water (40027). Edge offsets: NE+S moves the first sprite `(+7, -2)`, N+W the second `(-12, 0)`, N+SE the second `(+4, 0)`.

**Great Wall** (`draw_great_wall_district`, 40080): variant 0, era 0. For the NW and NE neighbours that are Great Wall districts **of the same owner**, their piece (column = direction); then the pillar (column 0); then the SW and SE pieces. All at the cell origin. The kind is drawn over resources: its resource sprites come first (18.10).

### 18.8 Port: coast alignment [C]

`align_variant_and_pixel_offsets_with_coastline` (39423), for a kind with `align_to_coast` and an owner. The sheet (variant: 0 NW, 1 NE, 2 SE, 3 SW; the Port's `img_paths` are in that order) and the anchor tile (one of the eight neighbours) come from the position of the **first** city around the port in `FOR_CITIES_AROUND` order and from which neighbours are land of the port's owner. A city on a diagonal next to the port decides alone (NE: SW sheet, anchor NE; SE: NW, SE; SW: NE, SW; NW: SE, NW). A city straight above, below, east or west prefers the owner's land on the sides that face it, and when there is none moves the sprite by a fixed amount (above: 4 left and up, below: 4 right and down, west: 30 left and 24 down, east: 30 right and 24 up). A city in a quadrant prefers the owner's land on its two sides. Else, or if there is none: the first land of the owner in the order NE, SE, SW, NW, N, E, S, W; else the same for any land; else SW and NE. The sprite then moves by a table of 7 cases for (sheet, anchor), and by edge cases for the **base terrain image** of the anchor tile (its sheet 0-5 and image number, from `SquareParts`) unless the anchor is hills or mountains or the city is on a diagonal (which have 4 cases of their own). The decision tree is `districts/port.rs::coast_choice`, a transcription of the function, checked against it for 300000 random neighbourhoods.

### 18.9 Wonder districts [C]

`draw_district_on_tile`, 40362. State `WDS_COMPLETED`: the wonder `wonder_index` (an index into `wonder_district_configs`), its finished cell. Else, if the district is `WDS_UNDER_CONSTRUCTION` (reserved by a city) and that city (`wonder_info.city_id`) still builds a wonder (`Order_Type` improvement with the wonder or small wonder characteristic) whose config is found by name (`find_wonder_config_index_by_improvement_id`, 7006: the first improvement with exactly the wonder's name) and has the tile in its work area (17195): the building-site cell of that wonder. Else the plain Wonder District cell (column 0, era row). For a wonder, the position uses the wonder's own size and the district's offsets, after moving towards the river if the wonder `wonder_requires_river` (square-type mask has the river bit 14, or `buildable_on_rivers`): `get_primary_river_direction` (4460: E, W, SE, NE, SW, NW, S, N, by `River_Code` bit) moves it by the **tile step** of the direction in x (-2 to 2 pixels, not a distance) and by 0, ±18 or ±36 in y (39388). The alternative art (`enable_img_alt_dir`) is chosen (39680) if the owner is `> 0` and: for a river wonder, the river direction's step is positive but not 2; else the nearest city of the owner in the work area (by `|dx| + |dy|`, the one with the smaller `|dx|` on a tie) is **east** of the wonder (`dx > 0`), or is straight above or below and any of the NE, E, SE neighbours is the owner's land.

### 18.10 Resources [C]

`draw_district_generated_resource_on_tile` (40112) draws a district tile's resources. The tile's own (`get_visible_non_subsumed_tile_resource`, 17798): the one the viewer's techs show, unless the district is completed (`DS_COMPLETED`, not merely complete by the mine) and its kind `subsumes_tile_resource`: then none. A kind with a `generated_resource` also shows that resource when: the district is completed, the tile's owner `>= 0` and, **given the viewer `visible_to_civ_id`**, equal to it (or the viewer `< 0`), the owner knows the resource's tech (unless `MF_NO_TECH_REQ`), and the tile holds the resource the kind wants on it (`resource_prereq_on_tile`; the connection the kind's other `resource_prereqs` need from the Trade Net is not checked here) (`district_generates_resource_for_civ`, 15091). With one, the tile's own resource is drawn by the game's code 16 pixels (`tile_width / 8`) to the left, the generated icon (no shadow) 16 pixels to the right of the centre `((tile_width - 49) >> 1, (tile_height - 49) >> 1)`; without a resource of the tile's own it is centred alone. Zoomed out C3X uses `tile_width = 64` for that while the sprite's width stays 49, which leaves it above and left of the centre.

Order: for a kind drawn over resources (the Great Wall) the resources come **before** the district, at the place of m12; for the others in the game's resource place, after pollution. The game's resource layer (`rules::resource`) draws exactly the cases C3X leaves to the game; `districts::takes_over_resource` says when it must not.

### 18.11 Open points [C]

- Seasonal and day/night art, `dc_img_state` failures (a missing sheet makes C3X draw nothing for districts at all, 40282) and online games (no district art) are not modelled.
- `FOR_CITIES_AROUND` also skips tiles with an enemy unit (4798); not modelled (a city tile has none).
- A generated resource needs its kind's other `resource_prereqs` connected to a city by the Trade Net (14651); taken as met.
- Read from the exe, not from C3X [X]: `Leader.Auto_Improvements` (Leader+0x20D0, a `Hash_Table`: items +4, key count +8, `Capacity` +0xC, mask +0x10) is open addressed: `Get_Key_Index` (0x56BB40) is `key & mask`, `Init_Items` (0x56B880) fills unused items with the key -1, `Hash_Table_look_up` (0x4C2010) probes linearly until the key is found or every item was seen, and `City_has_improvement` (0x4ACBE0) asks it for the building and for the continent key, as the port reads it. The continent is `m46_Get_ContinentID` = `Tile+0x1E`; `m18_Check_Mines` and `m20_Check_Pollution` test bits 2 and 6 of `m42_Get_Overlays(civ)`; `m35_Check_Is_Water` is effective terrain 11..13; `Territory_OwnerID` is a signed `char`.
- To be compared with the game: the zoomed-out offsets (18.4, 18.10), the places the district and the resources are drawn in relative to pollution and the city (18.1, 18.10), that a bridge really has no roads or railroads of its own on top (18.1), the alignment of ports (18.8) and wonders on rivers (18.9), and the colours of the resource shadows under a district.

---

## 19. Scenario files and the Load Scenario preview

### 19.1 The Load Scenario screen [X][D]

The title menu's *Load Scenario* (and *Conquests!*) screen is a form of class 0x66EAB0 (PCG), a 1024×768 dialog in the middle of the screen. When a scenario is highlighted (also the one it remembers when it opens), its code (around 0x59F600) builds the file's path (`Conquests\<name>.biq`, relative to the game's working folder, the install's `Conquests`) and calls `BIC::load_scenario_preview` (0x59B1A0, `__thiscall`, `this` = the global `BIC` 0x9C34C8, args: path, `int *out`; `ret 8`; 0 when it loaded) from **0x59F72A**, its only caller. That function calls `load_scenario` (0x59A870) with the file: the game loads the whole scenario — rules, map tiles, cities, units, the art search folders (BIC+0xC7C, not the scenario folder BIC+0x128) — into its tables to show the players, description and map size. C3X redirects `load_scenario` and recognises this load by its return address 0x59B1B9 (`ADDR_LOAD_SCENARIO_PREVIEW_RETURN`) to skip its own setup, so a hook belongs one level up: `civ3-bridge` points the `call` at 0x59F72A at itself (`civ3_sys::menus`), calls the original and records the path.

The map in the game's tables after such a load is the previewed scenario's, not a game's: with no game running the bridge used to draw it as the map (the main screen form, 0x66B3D8, composes the title menus too, so the map looked available), as seen by player 0 who has explored nothing: everything black but the top of tile (0, 0)'s mountain, which reaches above the map's first row where no fog is drawn (WWII in the Pacific: tile (0, 0) is a mountain on grassland, `SquareType` 0x6200). The same happens with the last game's map after leaving it. A game runs while `Main_GUI`'s canvas (0x9FDB10 + 0x274) is composed: it is under every screen open over the map (advisors, the city screen, the game menu), never on the title menus or the loading screen.

### 19.2 The `.biq` format [D]

Compressed with PKWare DCL (`blast.c`; uncompressed files start with `BIC`). Then `BICX` and sections: a four-letter name, a record count, the records, each an `i32` length and that many bytes (`VER#` is the first; `FLAV` is the exception: a group count, then per group a count and records of 4 + 256 + 4 + 4·n bytes without lengths). A scenario without rules of its own has no `BLDG`…`WSIZ` sections and uses `Conquests/conquests.biq`'s; one without a map (no `WCHR`/`WMAP`/`TILE`) has the game generate one. Field offsets below are from the record's data (after its length); checked against the game's memory after starting *9 WWII in the Pacific* (all 8160 tiles: `SquareParts`, `SquareType`, rivers, resources, cities, continents and ruins equal; 69 city tiles gain the road and railroad the game adds, victory locations get numbered, colonies renumbered):

| section | fields |
|---|---|
| `WMAP` | 0 resource count n, 4·n resource ids, then (at a = 4 + 4n) a continents, a+4 height, a+8 civ distance, a+12 civ count, a+24 width, a+28 32 starting locations, a+156 seed, a+160 flags (bit 0 wrap x, 1 wrap y, 2 polar caps) |
| `TILE` (45 bytes, width·height/2 records in `Map.Tiles` order) | 0 `River_Code`, 1 border (0 none, else 33 + the owner's race), 2 resource, 6 texture location, 7 texture file (= `SquareParts`), 14 barbarian tribe, **16 city, 18 colony** (QueryCiv3 has these two swapped), 20 continent, 23 victory location (0 yes, -1 no), 25 ruins, 29 `Overlays` (u32), 34 `SquareType` bits 8-15, 37 fog of war (the editor's brush: 0x8000 where painted, nothing else seen in the game's scenarios; not what the game reveals, §19.4), 39 `Terrain_Overlays >> 16` (the game sets bit 17 itself) |
| `CITY` | 0 walls, 1 palace, 2 name[24], 26 owner kind (1 barbarians, 2 race, 3 player), 30 building count n, 34 4·n buildings, then culture, owner, size, x, y, level, border level (1 the tiles around it, 2 from culture 10, 3 from 100, 4 from 1000; §19.4) |
| `UNIT` | 0 name[32], 32 owner kind, 36 experience, 40 owner, 44 unit type, 48 AI strategy, 52 x, 56 y |
| `CLNY` | 0 owner kind, 4 owner, 8 x, 12 y, 16 kind (0 colony, 1 airfield, 2 radar, 3 outpost) |
| `LEAD` | 0 custom data, 4 human, 8 leader name[32], 48 start unit count u, 52 8·u, then (at b) gender, b+4 tech count t, 4·t, then difficulty, initial era, cash, government, race, colour (the colour table), … |
| `RACE` | 0 city name count, 24 each, leader count, 32 each, then 208 bytes of text (leader name 32, title 24, Civilopedia key 32, adjective 40, name 40, noun 40), 520 per era (two file names), then culture group, leader gender, civ gender, aggression, unique counter, shunned and favourite government, default colour, unique colour, … |
| `GOOD` | 0 name[24], 24 key[32], 56 type, 60 appearance, 64 disappearance, 68 icon, 72 prerequisite |
| `PRTO` | 4 name[32], 36 key[32], 84 defence, 88 icon, 92 attack, 108 movement |
| `ERAS` | 0 name[64], 64 key[32] |
| `BLDG` | 64 name[32], 152 bombard defence (walls), 208 government, 220 obsoleted by, 232 `ImprovementFlags` |
| `RULE` | 96 spaceship part count p, 4·p, then at c = 100 + 4p: c+144 largest town, c+148 largest city |
| `GAME` | 8 playable civ count k, 4·k, then at 116 + 4k the search folders (`;`-separated, 5200 bytes), then 4·k alliances, 13 victory settings, a byte, 5 alliance names of 256, 25 war flags, the alliance victory type, the plague (a name of 260, a byte, 7 numbers), 260 bytes of text, the flag units, a byte, the gold for capture, then at 7311 + 8k the *Reveal entire map* byte (1 in *WWII in the Pacific*, *WWII European Front* and *Napoleonic Europe*), then retain culture (files of version 12.06 end 12 bytes after it, later ones 20) |

The game numbers a scenario's civilizations only when it starts (the human player first). A preview numbers them barbarians 0, then the `LEAD` players in order, then races owning something without a player; a city, unit or border owned by a race belongs to the player playing it.

### 19.3 The preview [D]

`civ3-bridge` (`preview.rs`) reads the highlighted file with `civ3_assets::biq` and makes a `Civ3World` of it as a nation that does not exist would see it if it had every player's start (§19.4): the tiles any player has explored at turn 1, those in sight not fogged, the rest black; no player's highlights, no active unit, every resource shown, the best defender of each stack on a tile in sight (no health bars, LEDs or stack lines, whose art keeps its size in a world shown small), labels for the cities on explored tiles only. The cells of vertices among unexplored tiles only draw nothing but their fog: the fog of war covers them inside the map, not what reaches past its edges (tall terrain, the halves of the edge rows and columns), which showed as stray bits around the black. The scenario's own art folders (its folder, the file's path without extension, and its search folders) are the art's folders while it shows.

The scene asks the viewer for the screen at the left and the world beside it (`scene::Beside`), naming the part of the world to fit (`Beside::world`): the smallest box of revealed tiles (`revealed::revealed_box`; on a map that wraps, the shortest stretch round it along each axis that wraps: all columns used but the longest gap) and a tile around it (past the map's edge the black beyond). The world is shown 2^k times smaller (`View::downscale`, the power of two at which that box is nearest to filling the space either way), the viewer zooming the rest. City labels are the name only, centred below the city, laid out at full size and kept so beside the city in the smaller world. `civ3-tool preview SCENARIO.biq OUT.png` renders the same offline.

Beyond downscale 2 the renderer samples sprites on the cells' grid (`tile_render_shared::placed`): sampled from each crop box's own corner, as the game does at 2, tiles cropped to different boxes left about one pixel per tile vertex uncovered (black dots on the sea; isolated black pixels in the preview of WWII in the Pacific: 5005 before, 830 after).

Not covered: a scenario without a map of its own shows none; saved games (`.sav`, the Load Game file picker, another code path) are not previewed.

### 19.4 What a scenario reveals at the start [M]

Measured in the game after starting scenarios (every civ's `Tile.Explored` and visibility bits at turn 1, from `civ3-session trace`), compared with what the `.biq` holds (`crate::revealed`):

- With *Reveal entire map* (`GAME`, §19.2) every civ has explored every tile (*WWII in the Pacific*: 8160 of 8160 for all six civs).
- Otherwise a civ has explored, around each of its cities, the tiles within a circle one tile wider than the city's borders, and around each of its units the eight tiles next to it. Along the map's two diagonals (u = (dx + dy)/2, v = (dx − dy)/2) a tile is in the circle of radius r when u² + v² ≤ r² + r + 1 (radius 1: the 9 tiles around; 2: the 21 of the work area; 3: 45, the 7 × 7 square without its corners; 4 and 5 rounder). r is the city's `CITY` border level + 1: *Rise of Rome*'s cities of culture 50–90 (border level 2) explore radius 3, those of 100–800 (level 3) radius 4, those of 1000 (level 4) radius 5. What a civ has explored it also sees (323 of Rome's 327 tiles).
- The tiles the editor's fog-of-war brush marks (`TILE` fog of war, 0x8000) are not revealed: *Rise of Rome* marks 3679 tiles, Rome has explored 327 at turn 1 (all but 12 its cities' and units' by the rule above), Egypt 204 of which none are marked.

The rule reproduces *Rise of Rome*'s turn 1 to within 2 % (the eight civs have explored 3232 tiles between them; the rule gives 3178, 3177 of them the same; the rest are a few units seeing two tiles far, e.g. from hills), and *Age of Discovery*'s to within 4 % (587 tiles between its nine civs, the rule gives 565, all of them among those). A preview counts the units and cities of every scenario player (a `LEAD` player, or a race one plays), not the barbarians'. A scenario whose players have nothing on the map yet (*Sengoku*, *Mesoamerica*) remains black. The game generates their starting positions when it starts; the preview has no exploration data for them.

### 19.5 Switching scenarios [D]

The game loads the highlighted scenario itself (§19.1) on its own thread, in 5–30 ms (60–140 ms for the biggest maps); everything of ours happens elsewhere, and nothing waits:

- **The preview thread** (`civ3r-preview`, `preview::Previews`) gets the file from the Load Scenario hook as soon as the game has loaded it, reads it and makes the world (1–6 ms natively) and sends it, then the city labels (their text measured as the game does), then the unit art the world needs, resolved in the scenario's art folders (`units::Needed::still`). The publisher takes what is ready (its pause between rounds ends as soon as a stage is); a newer selection supersedes the one being made (the thread stops between stages and sends nothing more of it), and what comes for an older one is dropped.
- **The publisher** shows the new world at once with the art there is: terrain and cities with the art before (the base game's, or the last scenario's), units whose files it has resolved already. It reads no unit art itself meanwhile (`UnitArt::set_reading`); once the preview thread's unit art is there it takes it and the scenario's art folders, and asks for their art.
- **Whole descriptions** for art folders no longer shown stop at the next sheet they load, or before they start (`art::ArtStop`, `assets::compile_growing`): flipping through the list builds the art of the last scenario only, not of each one passed; additions queued behind one given up add nothing.
- **The viewer** uploads new art a part a frame (about 16 MiB, `TileRenderer::stage_atlas`/`upload_staged`), drawing frames with the art before meanwhile, and swaps it in when all of it is there and a scene refers to it; the old atlas goes when the GPU is done with it (it used to wait for the GPU to free the old one first).

Measured at 1920 × 1080 on the AMD iGPU (the logs of these stages and `TILE_VIEWER_FRAME_TIMES`; medians of the selections picked 3 s apart, from the game's load of the file; before and after alternated, twice, the machine shared with other work). With the native viewer the new world's first scene went out after 33–61 ms (80 before, when the publisher read the file itself) and was drawn after 103–188 ms (131–172 before); the scenario's own art was in use after 453–464 ms (one run 1081; 471–488 before). In the game's process (the in-process viewer): 25–30 ms to the scene (70–72 before), 79–93 ms to the drawing (97–135), 685–724 ms to the own art (706–726). Flipping 26 times in 3 s, 18–21 of the 25 selections got a scene of their own on the way with the native viewer and 17–20 in-process (16–21 before), and whole descriptions for scenarios passed stopped after 90–270 ms. The viewers' frame gaps and longest draws stay as they were (native 17–70 ms longest draws, in-process 120–280 ms while flipping, not yet known what in them); the art's upload is not among them: it goes up over a few frames drawn meanwhile.

### 19.6 Exploring the preview [D]

A double-click on the world beside the screen explores it up close (`tile_viewer::layout`, `Explore`): it is shown twice as big as the driver shows it, by nearest pixel, around the point clicked, and the mouse is captured as in the viewer's views that follow the world cursor (the cursor drawn, the system pointer hidden, at the world's edge the system pointer shown at the world cursor), the "Press CTRL" hint at the world's top-left. Nothing of it reaches the game: clicks, keys and the wheel go nowhere while the mouse is captured. Ctrl on its own lets the mouse go (the view stays; the dialog takes clicks again, a click on the world captures the mouse again); a double-click or Esc goes back to the world fitted (Esc does not reach the dialog then). Another world beside the screen (another scenario picked) or the screen closing ends it too.

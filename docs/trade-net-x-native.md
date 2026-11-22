# Trade Net X as native 64-bit code

Goal (user request, 2026-11): C3X's Trade Net X compiled natively for the
machine (ARM64 on the Mac, x86-64 on Windows) and loaded through Wine's
unixlib mechanism. The C++ source must not change. Anything needed to make
it build goes in separate adapter files.

Status: measurement first. Nothing is implemented yet.

## What it is

C3X loads `<mod>\Trade Net X\TradeNetX.dll` (`injected_code.c` ~4877) and
takes these `cdecl` exports by name: `set_exe_version`, `create_tnx_cache`,
`destroy_tnx_cache`, `set_up_before_building_network`,
`get_move_cost_for_sea_trade`, `flood_fill_road_network`,
`try_drawing_sea_trade_route` (and `test`, unused).

C3X calls it from:
- `Trade_Net::recompute_city_connections` (creates the cache);
- `Map::build_trade_network`;
- `Trade_Net::recompute_city_cons_and_res`;
- `Trade_Net::set_unit_path` (the road flood fill);
- the sea trade path cost, once per pathfinder edge with flags 0x9/0x1009.

The source (`TradeNetX.cpp`, 832 lines; `Civ3Conquests.hpp`, 7,435 lines;
`BUILD.bat`: MSVC `cl /LD /EHs /O2`) ships with the mod in the install. Its
license is unknown, so it is not copied into this repository: the build
reads it from the install.

## Why it needs an adapter

- **Pointers.** It reads game memory through the `Civ3Conquests.hpp`
  structs, which hold 32-bit pointers. Built for 64 bits, every pointer
  field doubles in size and the layouts no longer match. A replacement
  header must lay the structs out with 32-bit pointer wrappers. The 32-bit
  address space lies within the 64-bit one in a WoW64 process, so a wrapper
  dereferences by zero-extending.
- **Game functions.** It calls game functions through `bin_addrs`
  (per-build address tables). These are 32-bit `thiscall` code that 64-bit
  code cannot call: `Map_get_x_dist`, `Map_get_y_dist`, `Map_wrap_horiz`,
  `Map_wrap_vert`, `Leader_has_tech`, `Leader_has_tech_with_flag`,
  `Leader_count_wonders_with_flag`, `TileUnits_TileUnitID_to_UnitID`,
  `Tile_has_colony`, `Tile_has_city`, `tile_at`, and the Tile vtable's
  m35, m38, m40, m50, m69 and m70 (water, territory owner, tile unit,
  base terrain, city civ, building owner). The adapter reimplements each
  natively, reading the same memory. Each needs a differential test
  against the game's own function.
- **Its globals.** It uses `p_bic_data`, `leaders`, `p_units`,
  `p_tile_units`, `p_player_bits` and `p_null_tile`.

## Measurement before porting

C3X can measure this itself. With `measure_turn_times = true` in its
config, every interturn ends with a popup (`injected_code.c` ~8019). It
shows:
- the turn time;
- the time spent recomputing city connections, with the call count;
- the time spent flood-filling road networks.

Measure a large late-game save over a few turns, with Trade Net X on and
off, on x86-64 (Papa) and, if the user agrees, on the Mac, where the 32-bit
code runs emulated. Port only if those times are a real share of the turn.

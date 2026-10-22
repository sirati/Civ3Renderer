# PCGames 1.22 AI focus/camera evidence (read-only)

Binary: `/mnt/datasets/winvm/civ3-src/Civilization III Complete/Conquests/Civ3Conquests-Unmodded.exe` (PE i386, image base 0x400000). C3X source: same directory's `injected_code.c`, `C3X.h`, `civ_prog_objects.csv`.

## Native camera functions

### Main_Screen_Form camera setter 0x4DF7C0

* Prologue: `83 EC 10 53 55 8B 54 24 1C 56 8B F1 57`.
* `__thiscall`-style function on `Main_Screen_Form *` (`ECX`); four stack dwords; `ret 0x10` at 0x4DFA8E. The calls use `(camera_x, camera_y, param3, always_update)`; e.g. 0x4E0D68 pushes `0,1,new_y,new_x` (arg4=0, arg3=1), while 0x56CA6A pushes `1,0,camera_y,camera_x` (arg4=1).
* Stores x/y at MSF+0x4E98/+0x4E9C. Unit steps are `BIC::is_zoomed_out ? (32,16) : (64,32)`, obtained through vtable +0x28 on `0x9C732C` (BIC/map data).
* Reads viewport globals `0x9CB3FC` L, `0x9CB400` T, `0x9CB404` R, `0x9CB408` B and computes tile ranges into MSF+0x4E70..+0x4E7C:
  `TileX_Min = (L+x)/stepX - 1`, `TileX_Max = (R+x)/stepX + 1`, `TileY_Min = (T+y)/stepY - 1`, `TileY_Max = (B+y)/stepY + 1` (C signed `idiv`). Sets dirty/repaint byte MSF+0x2FAC4=1 near 0x4DFA6A.
* Horizontal/vertical wrapping/clamping occurs before range recomputation. Non-wrapping X clamps to `[0, mapWidthStep - viewportWidth]`; non-wrapping Y clamps to `[0, mapHeightStep - viewportHeight]`, with Map.Flags bits 1/2 selecting wrapped paths. City focus (`byte [0x74238D]`) skips clamp at 0x4DF846. If `always_update`/arg4 is zero and old x/y equal new x/y, the range/dirty work is skipped at 0x4DF978..0x4DFA87.
* For a deliberately parked engine camera, call the original setter as `(0,0,1,0)` (or chosen tiny viewport globals) so range fields and dirty state remain coherent; do not write only MSF camera fields. Preserve `0x9CB3FC..408` (UI map clip) and semantic renderer focus separately.

### bring_tile_into_view 0x4DFAA0

* Prologue: `53 8B D9 B9 C0 09 74 00` (`push ebx; mov ebx,ecx; mov ecx,0x7409c0`). `ret 0x14` at 0x4DFB69. `Main_Screen_Form *` in ECX; five stack args `(tile_x,tile_y,param3,always_update,param5)` in calls.
* It first tests City_Form at `0x7409C0` and a helper 0x47B5B0; if City_Form is active and the fifth arg is false, it takes the early return at 0x4DFAC0..0x4DFB68. Otherwise it computes a legal camera centered on `(tile_x,tile_y)`, preserving parity, map wrapping, viewport offsets and current zoom, then calls setter 0x4DF7C0 at 0x4DFB61. The call pushes `(param5,param4,computed_y,computed_x)`; no third input tile parameter reaches setter.
* Main AI movement focus call is inside `perform_interturn` 0x4F5FB0: at 0x4F5B5D it obtains an AI/current unit's `Body.X/Y` and at 0x4F5B63..0x4F5B70 pushes `0,0,0,Y,X` then calls 0x4DFAA0. This is the call to observe for semantic focus; it may be bypassed by the City_Form early path or by an already-on-screen optimization in a replacement hook.

## Camera-dependent unit list and the “already visible” fast paths

### Animator::build_unit_list 0x4EF4D0

* Prologue: `83 EC 14 53 8B D9 57 33 FF`.
* Clears each prior entry's `U+0x24D` and animation timer, then `Units_Count` at Animator+0x1008. It loops only `Main_Screen_Form.TileY_Min..TileY_Max` (MSF `0x9FD538/0x9FD53C`) and X descending `TileX_Max-1..TileX_Min` (MSF `0x9FD530/0x9FD534`), wraps and parity-checks, then calls `find_visible_unit` 0x4E3F50 at 0x4EF5E9.
* Per tile, skips `U+0x259`, applies `tile_visible_now` 0x4F0DE0 unless always-on-top/override, and appends selected units; moving/always-on-top units cause a second `find_visible_unit`. Thus a tiny parked native camera intentionally limits this list to the small engine range; renderer off-camera snapshots must remain independent.

### find_visible_unit 0x4E3F50

* Prologue: `83 EC 1C 53 55 56 8B 74 24 2C 33 ED`; `ret 0xC`.
* Args `(Main_Screen_Form*, tile_x, tile_y, excluded)` (three stack args after `this`). It walks TileUnits and returns early for always-on-top, moving (`U+0x5D`), special attack/death/victory animation, and current unit. These returns happen **before** the normal visibility/tile score path; a hook intended to publish AI focus must run before relying on this native routine’s “already visible/animating” result.
* Normal path checks `Unit_is_visible_to_civ` 0x5BB360 and, unless debug-see-all or ATTACK/DEATH animation, drops invisible units. It then scores defenders by class/HP/fortification; this is not a camera test except that the caller only iterates the camera tile range.

### Main_Screen_Form_is_unit_visible_to_player 0x4ED1E0

* Prologue starts `51 A1 94 74 9C 00 53 ...`; `ret 0xC`.
* C3X replaces this with `patch_Main_Screen_Form_is_unit_visible_to_player` (`injected_code.c:1639`): true iff unit owner is current `Player_CivID` or `Leader_is_tile_visible(current_player,x,y)`. It never checks the camera. Preserve this behavior to avoid a camera-visibility leak.

## AI move/combat/animation hooks

### Unit_move 0x5BCF30

* C3X CSV: `void (__fastcall *)(Unit *this,int edx,int tile_x,int tile_y)`; native entry prologue `83 EC 08 53 55 ...`, `ret 0x8` (the executable's thiscall stack has x/y; C3X wrappers pass `__` for EDX).
* At 0x5BCF41 it returns immediately when destination equals current coordinates. Otherwise updates `Body.PrevMoveX/Y`, `Body.X/Y`, resets movement fields, sets animation target pixels via `0x403A80`/`0x403AA0`, calls `Unit_update`/event paths and applies tile-unit lists. This is the reliable semantic event for every AI and human move, before any camera-dependent animator list.
* C3X wraps it at `injected_code.c:3448` (`patch_Unit_move`): `record_ai_unit_seen(this,tile_x,tile_y)` then calls original. `record_ai_unit_seen` ORs destination tile `FOWStatus|V3|Visibility|field_D0_Visibility` into `is->players_saw_ai_unit` only for AI-owned units. This is the existing source of AI-move visibility; add renderer focus publication here (before/alongside the call), but do not force visibility bits.

### Unit_bombard_tile 0x5C1120 / Unit_play_bombard_fire_animation 0x5C0F20

* `Unit_bombard_tile` prologue `53 8B 5C 24 08 56 57 8B 7C 24 14 8B F1`; args x/y are stack, `ret 0x8`. It validates target at 0x5C112F, calls bombard-fire animation 0x5C0F20 at 0x5C114D, then applies status/movement.
* C3X wrapper at `injected_code.c:3454` (`patch_Unit_bombard_tile`) sets `is->bombarding_unit`, calls `record_ai_unit_seen(this,x,y)` **before** original, clears stealth/bombarding state after. This is the reliable semantic event for AI bombard focus. It must not be gated on whether the target is in current native camera.

### Fighter combat animation visibility 0x4A1D60

* CSV signature: `bool (__fastcall *)(Fighter*, int edx, Unit *attacker, Unit *defender, bool param3)`; native prologue `51 8A 44 24 10 53 55 8B 6C 24 14 ...`; `ret 0xC`.
* If `param3` is nonzero, function starts with `bl=1` and immediately returns true at 0x4A1FBB path. Otherwise it checks debug see-all (`0xA52640` bit8), native class preferences through `Game::get_preferences` (`0x539350`, storage at `0xA52638`), attacker/defender civs, per-unit visibility and tile flags (including `Leader_is_tile_visible`), then returns false when unseen. This is the native animation-suppression gate. Do not replace it with a camera check; call/publish semantic focus only after this says the event is legal/visible to the player.

### Animator_play_one_shot_unit_animation 0x4F01B0

* CSV signature `void (__fastcall *)(Animator*,int edx,Unit*,AnimationType,bool)`; prologue `51 53 8B 5C 24 10 55 56 8B 74 24 14 57 8B E9`; `ret 0xC`.
* It resets/starts the unit animation and repeatedly invokes `Animator_update` 0x4EED00 to advance it. The update rebuilds the camera-bounded unit/effect lists, so this function can animate a semantically visible AI combat event even when the native camera is parked; renderer must copy/publish the event tile and draw it independent of this list.

### Unit_play_attack_animation 0x5C7E90

* Prologue `56 8B F1 8D 8E 7C 02 00 00`; args direction stack, `ret 0x8`.
* Starts attack animation on `Unit.Animation` and calls `Animator_play_one_shot_unit_animation` (e.g. 0x5C7F43..0x5C7F5C for FORTIFY+ATTACK/ATTACK1). This is downstream of Fighter visibility; focus publication should happen at combat/Unit_move/bombard event, not by forcing this routine for hidden units.

## Safety conclusions

1. Publish a semantic focus event from C3X `patch_Unit_move` / `patch_Unit_bombard_tile` and from the accepted combat-animation path (after 0x4A1D60 returns true), independent of whether 0x4DFAA0 or 0x4EF4D0 considers the tile on-screen.
2. Observe 0x4F5B63..0x4F5B70 (`perform_interturn`'s AI/current-unit `bring_tile_into_view`) as an additional focus source. If replacing/suppressing the native camera setter, preserve that event before the native already-visible fast path.
3. Keep native visibility logic (`patch_Main_Screen_Form_is_unit_visible_to_player` and tile flags) unchanged. Never broaden visibility just because the renderer focuses a tile; semantic focus must not OR or synthesize FOW bits.
4. If a native camera setter hook parks the game camera, invoke original 0x4DF7C0 with coherent args and preserved viewport globals; do not mutate `TileX_Min/Max` by hand or write a non-zoom-scaled camera.

## C3X-patched executable check

The installed C3X-patched `/mnt/datasets/winvm/civ3-src/Civilization III Complete/Conquests/Civ3Conquests.exe` redirects `Unit_move` 0x5BCF30 (`E9 CF 97 72 00`) and `Unit_bombard_tile` 0x5C1120 (`E9 65 55 72 00`) to C3X injected code. `Fighter_check_combat_anim_visibility` remains native at 0x4A1D60. Hooks for the first two must use an over-JMP-compatible installer (`install_at_start`/`install_over_jmp`); checking only the unmodded prologues will reject or overwrite C3X's movement/attack logic.


Final implementation also validates the native enemy classifier at `0x558F20`, called by Fighter at `0x4A1DED`. This handles foreign hidden nationality through civ0 rather than reading an owner's At_War byte directly. `Game::get_preferences` (`0x539350`) preserves its class-specific policy. Exact byte checks for both unmodded and C3X images are in `camera-native-contracts.txt`.

# Independent map cameras

The renderer owns the displayed map cameras. The original Main_Screen_Form
camera stays at world pixel `(0, 0)`, and its native tile bounds stay
`[-1, 5]` on both axes. The native strict visibility test therefore sees
only tile coordinates `0..4`. Terrain iteration may include its surrounding
boundary cells. The global native map clip, game window resolution, UI
canvases and named panels retain their original dimensions.

City_Form retains its native camera and internal map geometry while open.
Its camera setup and restoration do not become main-map focus requests.
The next map update restores the small parked native map. A and B keep
showing their independent world locations throughout.

## Focus requests

`Scene.camera_focus` carries a serial number and a center in the current
game zoom's world pixels. The native side stores the center in canonical
full-size pixels, so Z changes the published coordinates without changing
the request's serial or world location. The viewer rebases its local
camera, pan and captured-cursor coordinates at the same transition,
including positions between native tile boundaries.

A serial change tells A to follow an explicit native focus request, even
when its destination equals the previous destination. Between requests,
manual panning retains its exact position. Per-frame parking writes do not
publish focus events, and Z rebuilds preserve the last canonical focus.
An actual direct camera request for `(0, 0)` still reaches A. B, the magnifier and inset
cameras do not receive these focus events.

The PCGames `bring_tile_into_view` entry at `0x4DFAA0` has its existing
renderer-selection suppression guard. For other calls it records a scoped
pending tile, then executes the native function. The request is published
only if that function reaches the native camera setter at `0x4DF7C0`.
This preserves its native early-return policy checks and ordinary
bookkeeping. The checked setter has four callee-popped arguments,
`x, y, param_3, always_update`. Its original invalidation logic still runs;
only the resulting main-map camera and tile bounds are parked.

AI focus also observes `Unit::move` at `0x5BCF30`, bombardment at
`0x5C1120`, and the accepted native combat visibility result at `0x4A1D60`.
Movement and bombard hooks preserve C3X's existing entry jumps. Movement
checks the native friend/enemy show-moves preference for the unit's class.
It calls the game's own enemy classifier at `0x558F20`, which handles
foreign hidden nationality through barbarian relations, and its preferences
getter at `0x539350`. It focuses a currently visible, detected destination,
or the visible source when the destination is hidden. A no-op move has no
focus event. Combat uses a visible, detected endpoint after the native
visibility gate succeeds, including the player's own defender; disabling
movement-following does not suppress battle focus. Visible bombard hits
follow the battle-animation preference. Debug see-all mode retains its
native visibility behavior. These hooks never change fog or unit state to
make an event visible.

World hover, selection and orders always use the renderer-supplied native
tile helper while parking is active, including tiles that happen to lie
inside the native window coordinates. UI hit testing retains the ordinary
client-coordinate route. The renderer's main art camera has no native tile
cull rectangle. The City_Form internal view keeps its own rectangle.

## Animation and activation

The camera hooks activate only after independent world input and animation
hooks install successfully and the native HWND message endpoint is ready.
A hook or endpoint failure leaves the original game camera behavior active.
Current native sites are byte-checked for the PCGames v1.22 executable;
unsupported executable builds keep the original camera.

Viewer shutdown or failure permanently disables parking for that process.
The bridge restores original pixel drawing, then uses a WM_NULL safe point
on the game thread to restore the full native map at the last renderer A
center. A pending City_Form keeps its own camera until it closes. An
attached input endpoint cannot reactivate parking after the viewer is gone.
This also covers in-process window initialization errors and native input
protocol failures.

Visible units, city effects and tile effects update through the existing
native elapsed-time call, with duplicate objects removed. The cursor and
global extra animations still update once. Native fog, detection, shared
vision and animation preferences remain in control of what the game shows.
See [independent animations](independent-animations.md).

## Diagnostics and verification

Startup logs distinguish `native camera hooks ready` from `native camera
active: parked (0,0), map tile range -1..5`. The second line appears only
when the game-thread update has a usable renderer input endpoint. Debug
logging for `civ3_sys::native_camera` records each semantic serial and its
canonical world-pixel destination. A normal map trace should show native
camera `(0,0)` and tile bounds `-1..5`, while A and B visibly show unrelated
parts of the map. Opening City_Form temporarily changes the native camera
and its own bounds; closing it restores the parked map.

Regression tests cover repeated requests to the same tile, direct corner
requests, City_Form entry and restoration, native Z transitions,
wire round trips, disabled metadata and the viewer-loss lifecycle. Viewer
tests cover exact manual A positions, A-only request handling, B/cursor independence and normal/Z
round trips. Live QA must additionally inspect AI movement/combat focus,
fogged or hidden AI actions, off-camera G targeting and selection, native
unit/effect animation progress, minimap navigation, and named City/F1/Menu
placement, plus closing or failing the viewer and continuing in the restored
native game window. Passing the pure regressions alone does not establish those
live behaviors.

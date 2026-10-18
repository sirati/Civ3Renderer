# Map input outside the game's camera

An independent renderer camera can show tiles that the original game window does not show. Pointer motion over those tiles emits `scene::Input::WorldMove`; clicks emit `WorldButton`. Once the game advertises its semantic camera-focus stream, every actual map point uses this route, including points inside the old client rectangle. UI controls retain client-coordinate input. The bridge no longer clicks the minimap to make a world click reachable. Actual map pans still have their own `Look` messages.

For the decoded PCGames executable, a registered private window message delivers each virtual event on the game's thread, in the same OS queue as ordinary keys and UI input. A hook on `Main_Screen_Form::get_tile_coords_under_mouse` supplies the renderer's tile. Native hover and Base_Form button handlers then perform selection, city opening, context menus, go-to targeting and legal-action checks. The game remains responsible for visibility, movement and command validity. Ordinary mouse input clears the virtual tile.

The conversion uses the game's current normal or Z coordinate scale, including wrapped maps. The stored preview identity is a tile, independent of artwork scale. It never treats a normal-resolution zoom-box camera as a different input coordinate system. Native selection does not need to recenter the game camera. A scoped guard suppresses the native empty-tile recenter during virtual selection, while a newly opened city screen retains its native city focus.

A presented camera movement also refreshes the world point beneath a stationary pointer. This keeps G targeting and a held right-drag route current during edge scrolling, flings and focus changes. Released destinations stay fixed. Failed frame acquisitions and retained pictures awaiting compatible art do not retarget from an unpresented layout.

Consecutive pending moves replace one another without adding more OS messages. Preview updates coalesce in the same way. Keys, buttons, sliders and other UI events end that sequence. Camera-report metadata does not end it. Path finding runs on the game thread at most once per 33 ms for a moving target, while the renderer can continue drawing faster.

The Drag map mode uses a separate go-to preview transaction. Right dragging changes the destination and right release keeps that preview. A later left click confirms only the same tile, selected unit and loaded world. A changed unit, map, command or destination consumes the confirmation without issuing a move. Cancellation always retires the transaction; Escape reaches only its own still-active go-to command in a safe map state, never a dialog opened in the meantime.

The native camera may park only while the private-message interceptor is attached to the live game window. Direct map events require the instrumented human-turn pump and no active animation or modal dialog. The existing advisor/Civilopedia input path remains responsible for its allowed actions while those forms are open. Builds without decoded native input sites retain their existing fallback.

## Validation

Automated coverage includes input wire round trips, normal/Z and wrapped tile conversion, hover beyond the original camera in free and captured modes, pending-move coalescing and input ordering, confirmation identity checks, and cancellation after a modal or unit change. The sys layer checks every native entry point against its recovered PCGames bytes before installing hooks.

For a controlled Wine or Windows game session:

1. Split the map into A and B. Pan B to a visible friendly unit outside the original game camera. Hover and select that unit without moving A. Right-click it and double-click a city to check the native context menu and city screen.
2. Press G and move the pointer across the original camera boundary into a distant renderer-visible tile. Check the destination cursor, red path and turn count continue to update. Confirm a legal destination once. Repeat with the pointer captured and with Z enabled, including the normal-resolution zoom box.
3. Check a wrapped map seam, an unreachable destination, a hidden enemy unit and an off-map black region. Native visibility and movement restrictions must still apply; leaving the map must clear the old destination overlay.
4. In Drag mode, right-drag a destination and release. Move the pointer elsewhere; the preview must stay. Click the original destination to issue one order. Repeat but click another tile, select another unit, open a dialog, change command, lose focus or load another save before confirming. None of those cases may issue the stale order or close the new dialog.
5. Open the Preferences and advisor panels, use relocated UI controls, then return to B. Ordinary UI input must restore client-coordinate lookup, and B's next world hover must restore its own tile.

Live native checks must use rebuilt matching bridge and renderer binaries because the input protocol changed. A test result from an older renderer does not validate these hooks.

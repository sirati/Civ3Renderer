# Alt targets in the status panel

The status orb and the unit picture are not `Button` objects. PCGames
`Main_GUI`, class `0x66B570`, draws them on its canvas and handles their
coordinates in `Main_GUI_handle_click_in_status_panel`, `0x555F30`.
C3X wraps this handler to preserve end-of-turn checks.

`controls` publishes tagged targets for the orb and the active unit's
picture. The orb skips the active unit, or ends the turn when the native
turn-end flag is set. The picture centres the map on the active unit.
Neither target is published for a hidden, mouse-disabled, or disabled HUD,
when a modal root removes the HUD, or while its status is passive.

The upper status row also selects the active unit or its carried units.
The native handler at `0x556220` computes its strip from
`Main_GUI.Unit_Status_Rect`, at `+0x13FE8`, and the `units_32` cell size.
The bridge identifies the icon columns actually composed into that strip.
The game's image row starts at `L+42`, but its click columns start at
`L+27`. Targets use those native click columns. Clicking the visual centre
of a densely packed cargo icon would otherwise select a later unit.
Overlapping icons use their column stride for the target width, including
the final column, so the snapping centre does not land on another unit.
The native game still checks each unit's movement and army membership
when that icon is clicked.

The orb's normal, hover and pressed sprites are `Main_GUI+0x147E4`,
`+0x14810` and `+0x1483C`. The normal sprite also follows the native
status mode at `+0x10654` when no unit is selected. The native hover flag
is `+0x1064C`. The bridge supplies these real alternative sprites to the
local hover renderer. The unit picture and unit icons have no separate
native hover art.

The targets remain in the driver's coordinates. The viewer applies the
same final HUD affine used for the visible panel to snapping and maps
clicks back through it. It sends ordinary pointer events, preserving the
normal game handler and C3X hooks.

Regression coverage includes status transitions, disabled and hidden HUDs,
modal roots, native clip geometry, actual icon composition, overlapping
columns, and target/click mapping for all splits at pixel scales 1–6 and
frame scales 50%, 100% and 200%.

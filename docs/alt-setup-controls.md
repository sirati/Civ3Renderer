# Alt targets on the title and New Game screens

The PCGames title and New Game choices do not create child Button forms.
Walking the normal form tree therefore omitted them from Alt navigation.
The renderer now reads the controls that the native hover and click code
actually tests, then sends the ordinary pointer and button events back to
that code.

The world setup form has vtable `0x670390`; its hover and click handlers
are `0x5DEA10` and `0x5DEA80`. They read the eight-bit hit image at form
`+0x3958`. Its indices 0 through 33 cover world size, barbarian activity,
landmass and ocean coverage, climate, temperature, and world age.

The editable map seed is a separate TextBox at world form `+0x120C`,
vtable `0x667188`. It also receives an Alt target. The normal native click
sets its caret and focuses its existing keyboard input; it is not replaced
with an invented selection action.

The player setup form has vtable `0x66E2F0`; its handlers are `0x5835A0`
and `0x583610`. Its hit image is at `+0x2B60`. Those pixels cover the
civilization choices, difficulty, game rules and AI aggression. The last
valid index is the current civilization count plus difficulty count plus
26. Restrictions from scenarios remove the corresponding hit pixels, so
no target is invented for an unavailable option.

`PCX_Image+4` points to the JGL image. Its width and height come from the
rectangle at `+0x54`, its byte stride is at `+0x40`, its bit depth at
`+0x24`, and its pixel pointer at `+0x4C0`. These masks remain rendered by
the game when ordinary drawing is suppressed. One guarded block read
copies a mask. Solid runs of each valid index produce its target; choosing
a solid rectangle keeps the target center out of gaps and other options.
The unrelated world renderer never scans these masks.

The title's active flag is byte `0xB425CA`. Its eleven rows use native
screen dimensions `0x9C72F8/0x9C72FC`, position values
`0x72C2A4/0x72C2A8`, and row height `0xB3E274`. Row zero, Play Last World,
is absent when `0xA52CE8` is zero. An open modal dialog excludes the title
behind it. The bridge matches the captured translated labels to those
rows and measures their current font with the existing system text
measurement. The click boxes use the same text width plus 20 pixels as
native handlers `0x54ECA0` and `0x54ED90`. Captured TextOut coordinates are
baselines, so matching allows the label's baseline within its native row.

Direct icon grids and radio lists also bypass the generic typed-control
dispatcher. Their draw thunks jump directly to `0x6156F0` and `0x61D7C0`.
The decoder now accepts those virtual-base thunks as well as the existing
typed dispatcher. Hidden forms and forms that do not accept the mouse
remain excluded.

The regression fixtures cover every native world/player index, scenario
restrictions, row stride padding, invalid images, nonrectangular hit
regions, direct grid and radio classes, title resolution and modal state,
and a title captured before the publisher starts without any hover input.

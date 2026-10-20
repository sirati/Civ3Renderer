# Unit and map notices

Short messages such as an exhausted unit's warning belong to the map, not to
an advisor or popup. Civ III draws them through `MapMessage::draw` onto the
animator's overlay image. Copying ordinary UI forms therefore omits them.

The renderer reads the fifty `Main_Screen_Form.map_messages` records at
`+0x590`, each `0x170` bytes. In the PCGames 1.22 executable, creation is at
`0x4D78D0`, placement at `0x4D7E80`, drawing at `0x4D7E00`, and removal at
`0x4F05B0`. The text at `+0x68` has already been localized and expanded by
the game. Semantic records decode Windows-1252 into Unicode. At the font
measurement/drawing boundary, text uses the renderer's byte-valued glyph keys
so punctuation, currency signs, and accents retain their original glyphs.
The tile is at `+0x18/+0x1C`; `+0x24` moves an overlapping message
down by half a tile. The font pointer is at `+8`, the foreground at `+0x14`,
and the shadow at `+0x10`.

The alive byte is `+0x26`. The byte at `+4` only controls the initial
input pause and clears before the message disappears. Neither that pause
byte nor the engine-camera visibility byte at `+0x25` determines whether a
renderer view should show a notice. Removal clears `+0x26` and the first
text byte. The creation timestamp is at `+0x168`, and the duration in
milliseconds is at `+0x16C`.

The game's animator normally expires these records, but stale alive records
must not keep plates visible. The publisher deduplicates text and tile,
restarts a notice only when its slot or creation timestamp changes, keeps
one notice per tile, and limits the total to eight. Another message on a
tile replaces the previous one. Valid durations up to five seconds come
from the game; missing or unreasonable durations use 2.5 seconds. The
renderer's own refusal messages use the same tracker and last 2.5 seconds.
Removing a game or modal record also removes its visible notice.

Each telling gets an increasing serial and a generic scene sprite lifetime.
The viewer measures its duration with its own monotonic clock, independently
of incoming scenes. This also expires notices while the publisher cannot
read a world snapshot or stops progressing. Repeated scenes with the same
serial do not restart the timer, and an expired serial cannot reappear.
Expiry masks only that notice's world sprite range in the derived render
list. Canonical scene clips remain intact, so a new telling can show the
same art again with a lifetime-only update. Unrelated sprites and the
indices used by interface looks and overlays stay intact.

The plate and its text become world sprites above units and fog. Their font
metrics and GDI font come from the original font object. The plate keeps
40 percent of the map underneath; the text shadow is displaced by two
pixels on each axis. Their art joins normal asset preparation and readiness
checks. Every view may show the same tile's notice, subject to that view's
Clean Map Units setting. The original notice draw is excluded from the UI
mirror and recent text fallback to prevent a duplicate screen-space copy.

## Live verification

1. Open a saved game with a unit whose movement is exhausted. Keep A and B
   at independent map positions, then bring the unit into both views.
2. Click or attempt to move that unit. Check that its localized warning
   appears above the unit in both views, including its translucent plate
   and shadow. Compare the text and lifetime with the original game view.
3. Repeat with the unit in B but outside A. Its warning must remain anchored
   to its actual map tile. Pan either view while it shows.
4. Repeat in the game's normal and Z zoom levels, and at another renderer
   zoom. Check text placement against the tile in each view.
5. Wait for expiry without further input. Both copies must disappear. Open
   a menu after expiry and confirm no old warning reappears.
6. Trigger another message in the reused slot. Confirm it replaces the old
   text. Spam the same exhausted unit fifty times. Check that one plate remains
   and expires after the last click. Trigger notices on separate tiles and
   confirm they each appear once, with at most eight visible at once.
7. Hide units through B's Clean Map override only. Its notice must hide in B
   while remaining present in A.

The automated tests cover record lifetime, slot reuse, Windows-1252 text,
placement at both engine zooms, deduplication, caps, expiry without new
scenes, stale-serial replay, and world-sprite
routing and category metadata. The live sequence above needs a game run.

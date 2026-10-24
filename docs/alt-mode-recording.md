# Alt mode recording

The viewer records how the Alt mode (the snapping pointer, `crates/tile-viewer/src/snap.rs`) is used, so that its target picking can be tuned from real use. The idea behind it: a player moves the mouse more or less towards the element they end up clicking, so every landing on another element before that click is the picker getting it wrong. A recording holds only the boxes of the elements, never which button of the game they are, so the picker can be run again over it with other values.

The setting is **Record Alt mode interactions (for tuning)** on the Accessibility tab of Preferences, in the "Mouse (Alt mode)" card (`input.record_alt_mode`, on by default). The files contain geometry, timing and mouse motion only, with no game text and no names.

## Where the files go

The files sit in the game's folder, next to `civ3-bridge.log`, named `alt-mode-<Unix seconds>-<pid>-<part>.altrec`. `CIV3R_ALTREC_DIR` overrides the folder. A separate viewer process (`civ3r-viewer`) records only when that variable is set.

A file is appended to while the session runs. Past 8 MiB the writer closes it and starts the next part. Each part starts with the rate and the last target snapshot, so it can be read on its own. After every write the writer deletes the oldest `alt-mode-*.altrec` files in the folder until all of them together are at most 64 MiB. It never deletes the file being written or any other file.

The viewer's thread builds the records and hands them to a writer thread (`civ3r-altrec`) through a `handoff::Bounded` queue of 16384 records. Nothing on the viewer's thread waits for the disk. When the queue is full a record is dropped, and the writer later writes a `Dropped` record with the count. When the viewer closes (window closed, Quit, the host's Close) it asks the writer to finish and waits for it at most 100 ms.

## What is recorded

Records are made only while the Alt mode holds the pointer (on, or landing after it was turned off), plus the "gave up" click after it.

- **Rate**: the Alt mode's updates per second (`input.snap_updates_per_second`, which also sets how long a flight lasts). It is written at the start of each file and whenever it changes. No other setting is recorded.
- **Snapshot**: every target the pointer can snap to, in window pixels. A snapshot is written only when the targets differ from the last snapshot written. The other records refer to targets by their index in the last snapshot.
- **Motion**: each raw mouse motion as the Alt mode gets it (before `MOTION_GAIN`), at the rate it arrives, during flights too.
- **Pointer**: where the Alt pointer is drawn, and the target it is on, when a flight lands (rest) and when a new motion starts (origin: the first motion after a rest, or after 0.25 s without motion).
- **Click**: each mouse button in the mode, the target snapped to, and what the press did (went to the target, started or ended an Alt-mode slider edit, or was a release that went nowhere).
- **Mode**: on (with the pointer position the mode started from, and the snapshot), or off (landing, or stopping at once). The reason is recorded too: Alt, Esc, Ctrl, a menu opened, the menus closed, focus lost, an attractor clicked, a shortcut, a command waiting for a place of the world.
- **GaveUp**: for 10 s after the mode ends, the first press with the free pointer on an element whose box (`rect`) equals a box of the last snapshot. The record holds the time since the mode ended, the target's index, the pointer position and the button. A press on nothing, on the world, on a box that moved or on a new box records nothing. The watch ends at that click, after 10 s, or when the mode comes on again.
- **Dropped** and **End**: records lost to a full queue, and the recording closed.

Wheel and scroll events, pick decisions, the spring's state and the other settings are not recorded.

## File format, version 1

All numbers are little-endian. A varint is unsigned LEB128.

Header, 32 bytes:

| bytes | field |
|---|---|
| 8 | magic `C3RALTRC` |
| u16 | version, 1 |
| u16 | length of the rest of the header, 20 (a reader skips any more) |
| u64 | wall clock at the session's start, Unix microseconds |
| u64 | time base: the viewer's clock in microseconds that the first record counts from |
| u32 | part number, from 0 |

Then records, each `u8 type`, `varint payload length`, payload. Every payload starts with a varint: microseconds since the previous record of the file (for the first record, since the header's time base). The viewer's clock counts from the viewer's start. A reader skips unknown types by their length. A record cut short (the viewer killed while writing) ends the file, and the records before it stay valid.

A **coordinate** is exact. If the value is a whole number of 1/256 pixels, it is written as `varint(zigzag(value · 256) << 1)` (low bit 0). Otherwise it is the byte `0x01` followed by the `f64`. A point is two coordinates, a rectangle four (x, y, width, height). A **target reference** is `varint(index + 1)`, with 0 for none.

| type | record | payload after the time step |
|---|---|---|
| 1 | Rate | `f64` updates per second |
| 2 | Snapshot | varint number (from 1), varint count, then each target (below) |
| 3 | Motion | point: raw dx, dy |
| 4 | Pointer | `u8` 0 rest / 1 origin, point, varint snapshot, target reference |
| 5 | Click | `u8`: bits 0-1 button (0 left, 1 right, 2 middle), bit 2 down, bits 3-4 use (0 to the target, 1 slider edit begins, 2 slider edit ends, 3 swallowed release); varint snapshot; target reference |
| 6 | Mode | `u8` 0 on / 1 off landing / 2 off at once; `u8` reason (0 unknown, 1 Alt, 2 Esc, 3 Ctrl, 4 menu opened, 5 menus closed, 6 focus lost, 7 attractor, 8 shortcut, 9 command targets the world); on only: point, varint snapshot |
| 7 | GaveUp | varint µs since the mode ended, varint snapshot, varint target index, point, `u8` button |
| 8 | Dropped | varint count |
| 9 | End | (nothing) |

A target in a snapshot:

| field | |
|---|---|
| varint key | the same element keeps its key through the session (a counter, not the game's id) |
| `u8` kind | 0 button, 1 slider, 2 list entry, 3 list, 4 capture view (attractor) |
| `u8` flags | 1 spread (the whole area draws the pointer), 2 an area of its own follows, 4 an anchor follows, 8 the entry lies in a list, 16 the list has entries |
| rectangle | where it takes clicks |
| rectangle | its area, if flag 2 (else the area is the rectangle) |
| point | its anchor (where the pointer goes on it), if flag 4 |
| slider | track rectangle, varint zigzag value in millionths |
| list entry | varint index of its list in the snapshot, if flag 8 |
| list | scroll rectangle, coordinate scroll step |
| capture view | point where the pointer goes |

These are the fields `snap::pick` and the flight read. A whole session of a few seconds takes about a kilobyte.

## Reading recordings

```
civ3-tool altrec FILE|DIR... [--rate N] [--tick-ms MS] [--cone DEG] [--dead-zone PX]
                 [--idle-reset S] [--angle-weight W] [--distance-weight W]
                 [--attraction F] [--gain G] [--flight F]
```

For each session (the parts of one file name joined in order) the tool prints two things:

- What was recorded: how often the mode came on, the snapshots, motions, landings and clicks, how many landings a motion led to on another element than the one clicked next, and how many of those had the motion pointing at the clicked element (within the cone, seen from where the motion started). The landing when the mode snaps on is not counted. It also prints the "gave up" clicks and the landings on other elements before them.
- The picker run again (`altrec::replay::run`) over the recorded motion, clicks and snapshots with the given values. The defaults are the constants in `snap.rs`, and the rate is the recorded one unless `--rate` is given. The tool reports the replayed landings, the landings on other elements before each click, and at how many clicks the replayed pointer was on the element the player clicked.

The replay steps the pointer every `--tick-ms` (default 8.333 ms, 120 a second, on multiples of it) and at each recorded rest. The viewer steps once a frame, so a replay of real use matches the live run closely but not to the frame. Over a recording made with steps on the replay's grid it repeats the live landings and clicks exactly (the test `a_replay_of_a_recording_lands_and_clicks_as_the_live_run_did`).

Example, a synthetic session (three moves along a strip of buttons, three clicks) replayed with the defaults and then with a narrower cone and a larger dead zone:

```
$ civ3-tool altrec alt-mode-1761300000-3197508-000.altrec --cone 20 --dead-zone 30
picker: cone 20.0°, dead zone 30 px, idle reset 0.25 s, angle weight 8, distance weight 0.25, attraction 0.25, gain 0.7, flight 1.333 updates; step 8.333 ms
alt-mode-1761300000-3197508 (1 file, 3.1 s)
  recorded: the mode on 1 time(s), 1 target snapshots, 60 motions, 3 landings, 3 clicks (3 on a target)
    landings elsewhere before the click: 0 (0.00 a click), 0 of them with the motion pointing at the element clicked
    gave up (clicked a box of the mode with the free pointer within 10 s): 0, after 0 landings elsewhere
  replayed: 4 landings, 1 landings elsewhere before the click; on the clicked element at 2 of 3 clicks (67 %)
```

With the defaults the last line reads `replayed: 3 landings, 0 landings elsewhere before the click; on the clicked element at 3 of 3 clicks (100 %)`.

In code, `snap::pick_tuned` and `snap::Snapper::with_tuning` take the values as a `snap::Tuning`. `Tuning::default()` holds the constants, and with it they behave exactly as `pick` and the viewer's snapper.

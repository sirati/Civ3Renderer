# JGL continuation handover

Implementation source is back at baseline `c1a8d1e`. The shared-glyph HUD
source, owned line/mill bypasses, and WoW64 window fixes predate this session. Owned mode still needs an explicit `CIV3R_JGL=own`.
Normal launches remain on the original jgl path until the final switch is
qualified.

The hard requirement is unchanged: owned mode must avoid legacy drawing work.
The renderer must be the only drawing path. Reconstructing a queried canvas
pixel on the CPU still repeats graphics work, even if it shares the
renderer's glyph emitter or coverage math. No scratch GDI surface,
`TextOutA` calibration, canvas-command pixel emulator, duplicate text
rasterizer, or original mill `TransparentBlt` is acceptable.

## Closed work

| Task | Status |
| --- | --- |
| Object-semantics research and source review | All of this task has been done. |
| Scoped Group 6 data/state/DC contracts | All of this task has been done. |
| Standalone Group 7 window and pump contracts | All of this task has been done. |
| Shared renderer glyph source and owned line bypass | All of this task has been done. |
| Owned mill raster bypass and private selector/art proofs | All of this task has been done. |
| WoW64 native-window build, taskbar fit, and launcher fixes | All of this task has been done. |

The source-review record remains in
`docs/evidence/jgl-renderer-source-review/review.json`. The scoped contract
reviews do not prove complete gameplay or a no-jgl shipping session.

## Open work

1. Click detection is ours (user, 21 November 2026): we find what a
   click is on and call the game's functions; the game's own click
   detection is not used. Done for the form under the mouse: 0x609030 is
   replaced by `civ3_sys::controls::form_hit`, which reads no pixel, and
   the CPU pixel-query code that answered the old reads is deleted. The
   map already went through the renderer (`world-input.md`). Still the
   game's: the Palace View's piece map (answered from piece geometry and
   art) and the screens' region index maps (exe-decoded file data, no
   drawing involved).
2. Run the combined owned game through the loaded map and HUD. Exercise keyed
   reads, frame readers, line paths, F1/F5/F3, quit, and both mill visibility
   cases. The old HUD and F1 missing-source failures have corrections and
   recorded regressions in `main`. The 26 October live check recorded zero
   unanswered owned reads, including an enabled Granary Iron badge. Its
   F1 sweep still had 680 text differences, so it does not close full Group 6.
   See `jgl-contract-audit.md` for the source-bound evidence and exclusions.
3. Repeat the enabled-mill live check on the final combined source after
   correction of the interaction/readback architecture.
   Confirm the native queries, retained resource art, ordering, clipping, and
   zero legacy DC/blit/release calls in owned mode.
4. Integrate and qualify Group 7 gameplay: launch lock, windowed mode,
   Alt/capture, stale input, screen/turn/combat checks, and the soak run.
5. Only after those gates pass, make the no-jgl default switch and produce a
   coherent release build. Deploy that build to Neo and verify the native
   renderer window, map, input, and clean exit on ARM64.

The recovery backup at `2501e49` was recreated and verified on both hosts.
Its receipt is `evidence/jgl-source-backup-20261104/backup-receipt.json`.
The final source checkpoint and its receipt are retained separately at
`~/civ3r-source-backups/20261104-renderer-source-final` preserves the
verified `7875de3` checkpoint on all three hosts. A successor incremental
bundle and receipt at `~/civ3r-source-backups/20261104-architecture-review`
preserve the rejected experiment and the corrective main commit. Restore
the base bundle first, then fetch the successor bundle. Consult the external
receipts for exact commits, hashes and verified hosts. The configured origin is the unavailable local path
`/home/sirati/Projects/civ3-renderer-old`; no origin push is claimed.

Only unfinished Codex tasks belong in the continuation state. Read their
status before starting a replacement. Completed agents must not be listed.

Sound and the 240 FPS target are P3. Six-bit/seven-bit pixel attribution is
P4.

## Current validation and limits

The 4 November CPU pixel-query experiment is rejected for architecture,
although its tests and release builds passed. It extended source sampling,
text layout/coverage replay, and query caching instead of removing the
need for that work. Its source is preserved on
`checkpoints/cpu-keyed-readbacks-20261104` at `9f0f07a`. All five new
implementation commits were reverted from main, including the supporting
resource-version API. The four affected source files match `c1a8d1e` byte
for byte. The earlier verification record is historical experiment evidence,
not acceptance of that architecture. No experimental artifacts were deployed.

The datasets drive is unmounted in the current session. The game assets,
VM images, old runtime evidence, and previous backup directory under
`/mnt/datasets` are therefore unavailable. A noninteractive mount was denied
by the OS. Live game validation cannot resume locally until the drive is
available; source work and host tests can continue.

The latest retained WoW64 Windows play result reached the map, founded Rome,
advanced to 3900 BC and saved `wow64test.SAV`. The runner still needed to
parse the latest HUD date and click OK after selecting the quit radio
choice. These are test-runner findings, not acceptance of ordinary Quit.

The accepted source changes preserve the renderer's file, glyph, line, and
mill state rather than copying scratch pixels into the viewer. Original-jgl
comparisons remain diagnostic only. Preserve and restore game files, DLLs,
settings, and logs with hashes around every live run. Do not claim Neo
deployment or backup verification without a receipt containing the final
archive hash and remote verification.

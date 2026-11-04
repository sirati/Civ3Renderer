# JGL continuation handover

`main` is clean at `c1a8d1e`. The shared-glyph HUD fix, owned line bypass,
owned mill raster bypass, and the WoW64 window/launcher fixes are already in
the ancestry of `main`. Owned mode still needs an explicit `CIV3R_JGL=own`.
Normal launches remain on the original jgl path until the final switch is
qualified.

The hard requirement is unchanged: owned mode must avoid legacy drawing work.
The renderer is the only drawing path. Text hit answers use the same glyph
data and layout as the viewer. No scratch GDI surface, `TextOutA` calibration,
duplicate text rasterizer, or original mill `TransparentBlt` is acceptable.

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

1. Run the combined owned game through the loaded map and HUD. Exercise keyed
   reads, frame readers, line paths, F1/F5/F3, quit, and both mill visibility
   cases. The earlier run stopped at
   `owned keyed canvas 0x9fdd84: no exact model answer`; that is an
   incomplete-path diagnostic, not acceptance.
2. Qualify an enabled mill in the live game after the private mill proofs.
   Confirm the native queries, retained resource art, ordering, clipping, and
   zero legacy DC/blit/release calls in owned mode.
3. Integrate and qualify Group 7 gameplay: launch lock, windowed mode,
   Alt/capture, stale input, screen/turn/combat checks, and the soak run.
4. Only after those gates pass, make the no-jgl default switch and produce a
   coherent release build. Deploy that build to Neo and verify the native
   renderer window, map, input, and clean exit on ARM64.
5. Recreate the successor source backup after the T3 restart. The prior
   resumable Neo transfer was cancelled and its receipt is not present in the
   current filesystem. The configured `origin` path is
   `/home/sirati/Projects/civ3-renderer-old`, which is unavailable here, so a
   verified origin push cannot be claimed.

No subagent is currently unfinished. If a later gate needs delegation, start
one Codex GPT-6.1 Sol agent with an isolated scope and record its task here
only while it remains unfinished. Completed agents must not be listed.

Sound and the 240 FPS target are P3. Six-bit/seven-bit pixel attribution is
P4.

## Evidence and safety

The accepted source changes preserve the renderer's file, glyph, line, and
mill state rather than copying scratch pixels into the viewer. Original-jgl
comparisons remain diagnostic only. Preserve and restore game files, DLLs,
settings, and logs with hashes around every live run. Do not claim Neo
deployment or backup verification without a receipt containing the final
archive hash and remote verification.

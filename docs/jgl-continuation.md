# Jgl continuation handover

The current task is to finish Group 6, Group 7 and the final no-jgl switch,
then qualify a coherent build on Neo. Main still has implementation baseline
`dd992e0` plus documentation. Experimental replacement code remains unmerged,
and `CIV3R_JGL=own` remains opt-in. Normal viewer launches still load jgl.

The user's requirement is to avoid legacy drawing work. Our renderer must
be the only drawing path in owned mode. Text hits must use the same glyph
data and placement that the viewer draws. Reproducing jgl text in another
rasterizer, calibrating against `TextOutA`, or doing legacy drawing into a
discarded scratch bitmap does not meet that requirement. Original-binary
pixel comparisons remain useful research, but cannot accept that architecture.

## Closed work

| Task | Status |
| --- | --- |
| Complete object-semantics research | All of this task has been done. |
| Palace geometry and retained-source review | All of this task has been done. |
| Registry and font-resource lifetime review | All of this task has been done. |
| Scoped static navigator comparison at `83caad2` | All of this task has been done. |
| Group 7 standalone window and pump implementation and comparison | All of this task has been done. |
| Scoped image data, state/error and GDI+ original-binary comparison | All of this task has been done. |

The [object semantics](jgl-object-semantics.md) preserves the completed
research. Earlier source and navigator evidence remains in the
[replacement record](jgl-replacement.md). These scoped results do not qualify
later changes or the full replacement.

## Open work

The combined candidate `31da02c` includes accepted Group 7 code and the
corrected Group 6 data implementation. The capped global blit history and
guessed frame transparency are removed. Original-binary comparisons now
establish the tested `+0x38` forward overlap traversal, `+0x58` data
replacements, image key and defined state/errors, and GDI+ status/lifetime
sequence. Full Group 6 remains unaccepted.

The final matched build reaches the visible title, load dialog, save summary
and loaded map. Moving the pointer to `(1642,970)` then exits with
`owned keyed canvas 0x9fdd84: no exact model answer`. This forced exit is an
incomplete-path diagnostic, not accepted gameplay. HUD glyph, indexed,
transformed-art and other source semantics must supply exact supported
answers without guesses or dummy pixels. The current model rejects whole
source families and can propagate unknown state across an entire canvas.

The two frame-reader sites select four caches at `+0x11C`, `+0x120`,
`+0x124` and `+0x128`. Their writers include fills/tiles, lines, captions
and per-form drawing. Shared source semantics and reached-reader predicate
comparisons remain open. Border pointer movements alone did not establish
that either reader ran. Loaded-art instance/rectangle transformations,
remaining indexed readers, dynamic/replay/scenario buffers and unsupported
slots still need coverage in the [contract audit](jgl-contract-audit.md).
Native mill-icon art/state and enabled-feature visibility remain separate
unfinished work. Source implementation now exists at `2ac04ae`, with
sampling support at `538d23c` and retained documentation at `b753a5f`.
It is unaccepted because its row hook still calls the original producer,
including DC acquisition and `TransparentBlt`, before emitting native art.
The continuation must avoid that legacy raster work in owned mode, preserve
required state and pass-through behavior, and then qualify an enabled mill.

The later HUD candidate has independent frame/animation fixes at `05f567c`
and game-loaded civ-palette wiring mixed into `3ca7e11`. These remain subject
to review. Its separate glyph-outline and scratch-text calibration engine is
rejected. Parent preserved all subsequent per-colour work in recovery commit
`ac6bde9`; it must not become production code. The continuation removes that
engine and shares the actual renderer's glyph source and layout for hits.
The previous four sessions and 42-million-pixel text comparison are historical
diagnostics, not acceptance of the rejected implementation. Main has none of
these experimental implementations merged.

The parent also found that `hook_lines` can call the original C3X line
drawer after recording a native line, including keyed canvases. That drawer
uses GDI+. Its reached owned-mode paths need the same no-duplicate-drawing
audit. Previously accepted GDI+ status/lifetime comparisons establish their
stated contracts; they do not require retaining unnecessary raster work.

Group 7's standalone implementation is accepted at `5ca2cf6`. Integrated
launch-lock, windowed mode, Alt/capture, stale input,
screen/turn/combat checks and the 30-minute soak follow Group 6 and parent
integration. Default replacement and Neo deployment remain unaccepted.
Sound and 240 FPS remain P3; six-bit/seven-bit attribution remains P4.

## Continuing agents

Two replacement GPT-6.1 Sol tasks continue the unfinished Group 6 work. Their identifiers,
branches and reservations are in [the machine-readable state](jgl-continuation-state.json).
Read their durable state before spawning anything. Continue an existing
running task; if it has stopped with unfinished work, start a replacement
with its original brief, result and unresolved review findings.

- The HUD/frame task replaces duplicate text reproduction with the viewer's
  shared glyph source and qualifies visible and logical behavior on the candidate.
  It alone may drive the shared local game, replace test DLLs and use
  `/tmp/c3r-jgl-g23/target`. It may combine the accepted Group 7 code in an
  isolated opt-in candidate and run available combined-game checks.
- Native mill icons avoid the original mill raster path while deriving the
  city-list icons from mill/resource state and file art. This task uses its private target and fixtures, without shared
  GUI, installed-file mutation or warm-target access. Enabled-mill live
  qualification follows parent integration and reassignment of the GUI lease.

After Group 6 acceptance and parent common-source integration, spawn a
GPT-6.1 Sol continuation using [the integrated Group 7 brief](jgl-group7-integration.md).
Assign the sole shared-game reservation to it after Group 6 releases that
reservation. Its remaining work is integrated game qualification and the
final switch. No Group 7 agent is running now.

Do not restart completed work or the deferred backlog. Only stop games
started by the current task. Preserve and restore DLLs, settings and logs
with hashes. Never use scratch game pixels as viewer input or add a software
replacement for jgl canvas drawing.

## Evidence and source safety

The [latest Group 6 review](evidence/jgl-readback-review/review.json) verifies
158 retained evidence hashes and the source archive. Independent parent
replays matched 201 data-operation calls, 492 defined image state/error pairs,
8,224 GDI+ status/state pairs across 16 lifetimes, and the 40-call navigator
regression. The scope is seeded data and defined logical state/status, not
frame/HUD pixels or complete gameplay. The scratch extent and pixels differ.

The full workspace run completed with 2,116 passes, 37 ignored tests and only
the known P4 subpixel failure. Affected packages, guards and release builds
also passed. Final artifacts use production implementation `34d7add`; later
commits change tests or documentation. Original baseline files were restored,
but the original pre-run hash of `civ3-render-errors.log` is uncertified
because its baseline was captured late. No broader restoration claim is made.

The [Group 7 review](evidence/jgl-window-review/review.json) checks 26
source, artifact, log and cleanup bindings. Independent parent replays
matched all 2,840 deterministic and 438 real Win32 observations, including
nested pump re-entry and 24 sequential window lifetimes. The comparisons
deliberately load original jgl as an oracle; they do not prove a no-jgl game
session. The workspace retained the known P4 renderer failure and a separate
unchanged handoff-test scheduling failure that passed its isolated rerun.
The [window contract matrix](jgl-window-contracts.md) states the exclusions
and remaining game gates. The implementation remains on its unmerged branch
until it can be combined with accepted Group 6 code.

Origin is the missing `/home/sirati/Projects/civ3-renderer-old`. The successor
backup is `20261026-jgl-renderer-sources` under the local, Papa-laptop and Neo
backup roots. It preserves committed refs, readable documentation and the
review transcript/source evidence. Any in-progress source snapshot is an
unaccepted recovery checkpoint, not a tested build. Consult the receipt
named in the continuation state for verified transfers and exact hashes.
The preceding readback-review backup was verified locally and on Papa's
laptop; Neo SSH timed out and that checkpoint's replication was pending.
No new Neo deployment has occurred. A working, coherent Neo build remains
the delivery goal after source correction and integrated game acceptance.

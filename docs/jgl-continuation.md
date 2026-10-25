# Jgl continuation handover

The current task is to finish Group 6, Group 7 and the final no-jgl switch,
then qualify a coherent build on Neo. Main still has implementation baseline
`dd992e0` plus documentation. Experimental replacement code remains unmerged,
and `CIV3R_JGL=own` remains opt-in. Normal viewer launches still load jgl.

## Closed work

| Task | Status |
| --- | --- |
| Complete object-semantics research | All of this task has been done. |
| Palace geometry and retained-source review | All of this task has been done. |
| Registry and font-resource lifetime review | All of this task has been done. |
| Scoped static navigator comparison at `83caad2` | All of this task has been done. |
| Group 7 standalone window and pump implementation and comparison | All of this task has been done. |

The [object semantics](jgl-object-semantics.md) preserves the completed
research. Earlier source and navigator evidence remains in the
[replacement record](jgl-replacement.md). These scoped results do not qualify
later changes or the full replacement.

## Open work

Group 6 checkpoint `339bac3` corrects the owner identity, first lazy 16-bit
fill and Rust self-copy aliasing defects. It classifies replay/form buffers
earlier and installs independent hooks for both frame readers. Full Group 6
remains unaccepted.

Frame masks still need exact answers from their writers and the viewer's
immutable sources. The current fallback guesses transparency when provenance
is missing. A global 1,024-entry history records every sprite blit and can
evict persistent masks during unrelated drawing. It also omits writer clip,
scale and other composition semantics. Loaded-art transforms need consistent
retained file, viewer and hit-query behavior; basename transparency constants
do not establish the original rectangle and lifetime semantics. Original
`+0x38/+0x58` status and overlap comparisons remain open.

The scratch-DC GDI+ constructor/release/destruction test now passes under
Wine. Actual drawing/status comparison remains open. Native mill icons
still need their viewer art/state implementation. Other indexed readers,
escaped pixel pointers and unsupported slot/error cases remain in the
[contract audit](jgl-contract-audit.md).

The latest game attempt did not reach a usable loaded save. Two retained
captures are entirely black despite the harness reporting a main menu.
Startup/load must be diagnosed against the original baseline before a
screen or click comparison can count as acceptance.

Group 7's standalone implementation is accepted at `5ca2cf6`. Integrated
launch-lock, windowed mode, Alt/capture, stale input,
screen/turn/combat checks and the 30-minute soak follow Group 6 and parent
integration. Default replacement and Neo deployment remain unaccepted.
Sound and 240 FPS remain P3; six-bit/seven-bit attribution remains P4.

## Continuing agents

Two GPT-6.1 Sol tasks continue the unfinished Group 6 work. Their identifiers,
branches and reservations are in [the machine-readable state](jgl-continuation-state.json).
Read their durable state before spawning anything. Continue an existing
running task; if it has stopped with unfinished work, start a replacement
with its original brief, result and unresolved review findings.

- Group 6 corrects and qualifies the data, mask, image and DC contracts.
  It alone may drive the shared local game, replace test DLLs and use
  `/tmp/c3r-jgl-g23/target`. It may combine the accepted Group 7 code in an
  isolated opt-in candidate and run available combined-game checks.
- Native mill icons derive the city-list icons from mill/resource state and
  file art. This task uses its private target and fixtures, without shared
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

The [latest Group 6 review](evidence/jgl-group6-correction-review/review.json)
records the source corrections and remaining objections. The new Wine logs
report the 40-call, 1,369,360-byte data comparison and GDI+ constructor test
passing. Final Windows/native artifact hashes were independently checked
and copied before the warm target was reassigned. The current debug test
binaries were retained, but their historical hashes were not supplied.
Four DLL/native restoration hashes match the captured baseline output.
The workspace log stops at the known P4 renderer failure; later package and
guard completion was not established by the command transcript. Earlier
live results apply only to their earlier bound checkpoints.

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
backup is `20261025-jgl-group6-correction-review` under the local, Papa-laptop and Neo
backup roots. It preserves committed refs, readable documentation and the
review transcript/source evidence. Any in-progress source snapshot is an
unaccepted recovery checkpoint, not a tested build. Consult the receipt
named in the continuation state for verified transfers and exact hashes.
The preceding window-review backup was verified locally and on Papa's
laptop; Neo SSH timed out and that checkpoint's replication was pending.

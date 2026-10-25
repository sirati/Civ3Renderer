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

Group 6 checkpoint `907d777` was reviewed and withheld from integration.
Its data policy confuses `JGL_Image*` with its `PCX_Image*` owner, disabling
the established navigator policy. Dynamic ownership is discovered too late
for initialization and omits related replay buffers. A first 16-bit fill
does not allocate lazy storage. The indexed copy creates overlapping Rust
references for self-copies. Loaded-art transforms still need correct source
semantics and comparison with original jgl.

Both frame-mask branches need answers from their actual writers and the
same sources as the viewer. Only `0x608E0A` is hooked in this checkpoint;
`0x608E7E` remains unhandled. A last indexed blit and fallback to owned
dummy bytes do not establish either mask's behavior. Keyed hits on text and
unavailable art remain open.

The 1x1 scratch DC needs actual GDI+ initialization, retained-handle and
destruction checks. A nonzero DC does not establish those contracts, and
mill icons still lack their native viewer art/state source. Other indexed
readers, escaped pixel pointers and unsupported slot/error cases remain in
the [contract audit](jgl-contract-audit.md).

Group 7's standalone implementation is accepted at `5ca2cf6`. Integrated
launch-lock, windowed mode, Alt/capture, stale input,
screen/turn/combat checks and the 30-minute soak follow Group 6 and parent
integration. Default replacement and Neo deployment remain unaccepted.
Sound and 240 FPS remain P3; six-bit/seven-bit attribution remains P4.

## Continuing agents

One GPT-6.1 Sol task continues the Group 6 corrections. Its current identifier,
branches and reservations are in [the machine-readable state](jgl-continuation-state.json).
Read their durable state before spawning anything. Continue an existing
running task; if it has stopped with unfinished work, start a replacement
with its original brief, result and unresolved review findings.

- Group 6 corrects and qualifies the data, mask, image and DC contracts.
  It alone may drive the shared local game, replace test DLLs and use
  `/tmp/c3r-jgl-g23/target`.

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

The [Group 6 review](evidence/jgl-group6-review/review.json) binds the reviewed
source and records each objection. The completed task ran 39 safe jgl tests,
three selection tests, one navigator test, nine host jgl-ffi tests and the
lock/generic guards. It built Windows release artifacts at `1c8fe20` and
checked the final `907d777` source. Its new Windows-only data assertions and
live game behavior were not exercised. Formatting was blocked by unavailable
rustfmt. Earlier live results apply to their earlier bound checkpoints.

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
backup is `20261025-jgl-window-review` under the local, Papa-laptop and Neo
backup roots. It preserves committed refs, readable documentation and the
review transcript/source evidence. Any in-progress source snapshot is an
unaccepted recovery checkpoint, not a tested build. Consult the receipt
named in the continuation state for verified transfers and exact hashes.
The preceding Group 6 review backup was verified locally and on Papa's
laptop; Neo was offline and that checkpoint's replication was pending.

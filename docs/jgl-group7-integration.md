# Group 7 integrated qualification brief

Spawn a GPT-6.1 Sol continuation for this unfinished work once Group 6 is
accepted, the parent has integrated the common source, and Group 6 has
released its exclusive shared-game GUI/input/test-DLL reservation. Do not
restart the accepted standalone implementation work. Until then, preserve
the active Group 6 task and avoid a competing game session.

## Starting source

The standalone window/pump implementation is accepted at `5ca2cf6` on
`t3/finish-jgl-groups-switch-4`. Its implementation commit is `96598d1` and
its final fixtures are `019d24d`. The parent independently replayed 2,840
deterministic and 438 real Win32 comparisons with zero differences. The
[contract matrix](jgl-window-contracts.md) and
[parent review](evidence/jgl-window-review/review.json) define that scope.
The branch also contains earlier experimental Group 6 ancestry. Do not
merge that ancestry into main merely to obtain Group 7.

Use a new app-managed worktree based on the parent's accepted integrated
commit. Retain the source/artifact bindings. Read the
[current state](jgl-continuation-state.json), [replacement plan](jgl-replacement.md),
[object contracts](jgl-contract-audit.md), concurrency and generic-crate rules.

## Required outcome

Qualify behavior against original jgl in the actual game. Cover launch lock,
normal and windowed modes, Alt/capture/focus transitions, stale input, save
load, map, city screen, advisors, diplomacy, Civilopedia, game menu, Escape,
turn change, combat and confirmed Quit. Finish the 30-minute soak. Compare
actual calls and effects, before/after screens and populated traces. A trace
with zero draw records is not trace equivalence.

Use the established `000jgl.SAV` load workaround. Set
`input.alt_in_menus = false` for scripted screen walks; test Alt behavior
separately under controlled capture. Only stop games started by this task.
Take fresh DLL, settings, log and trace baselines and restore every changed
file with checksums. Receive the GUI reservation explicitly from the parent.

Prove that owned mode has no jgl mapping or Wine loader entry throughout
the run and that original-table counters remain zero. Preserve plain-game
pass-through without a viewer or trace. Default viewer launches currently
still load original jgl. The parent enables the default no-jgl switch only
after the full gates pass and then repeats normal-launch qualification.

## Constraints and completion

Preserve the counter design and input behavior. Re-render from commands,
model and file/state sources. Do not mirror game pixels, implement jgl's
canvas rasterizer, add production locks, watchdogs or guessed answers.
Unsafe ABI/OS work belongs in jgl-ffi/inproc; behavior belongs in safe crates.
Keep generic crates game-agnostic.

Build on Papa with `nix develop` and `CARGO_INCREMENTAL=0`. Use owned targets
and evidence directories. Do not access Neo, Nexus or wayland-host, copy
asset trees, delete other work, or run broad cleanup/GC. No main merge, push
or deployment from the child. The parent handles authorized Neo deployment
and acceptance after coherent builds and integration review.

Make signed coherent commits. Bind final source and artifacts to logs,
comparison results and restoration receipts. Retain failures and exceptions.
The known P4 subpixel_coverage failure is separate; run affected tests and
guards, and report any other failure. Report exact closed/open gates and
commits. If a gate fails, fix the cause and rerun that gate; do not replace
the expected behavior with a success shortcut.

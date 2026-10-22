# Opportunistic scenario and UI preload successor

This work is separate from the already CPU-validated atlas-reservation storm fix described in atlas-reserve.sol.md. No Neo connection, launch, GPU job or VM/UI operation was performed by this agent. The saved Neo audio sample still does not establish the recurring music-loop cause. No speculative audio hook or disable-music setting was added.

## Source changes

- `crates/civ3-bridge/src/preload.rs`: interface/font phase precedes optional unit animations; later phases append without resetting admitted-byte counters or the original cap; cancellation is checked between individual animation reads. The world snapshot for optional planning contains only unit/rule/civ metadata, never map tiles or complete city records. Resized indexed-art estimates charge actual output RGBA dimensions.
- `crates/civ3-bridge/src/preload/planner.rs`: one reusable worker, one latest game request and one owned result slot for each of three phases. Builtin UI arrives first, scenario Pedia small icons second, optional units last. Per-plan epochs and the existing art-folder cancellation token reject stale results. Later phases cannot overtake the first even when publication races a take. File scans do not trigger publisher scene reconstruction or GPU status queries. Completed phases ring the existing publisher wake.
- `crates/civ3-bridge/src/live.rs`: restricted preload declarations, status/admission and planning blocks only. Budget0/off creates no planner or scan. Title-menu prewarm is allowed. Existing one-chunk GPU-upload acknowledgment and urgent-art priority conditions remain. Indexed art, previously omitted from optional Screen admission, is now retained. Atlas reservation arithmetic saturates rather than overflowing.
- `crates/civ3-bridge/src/text.rs`: only add_style, take_loaded and one prune regression. Optional font requests no longer backdate themselves by one hour. Completion receives the normal finite retention window, preventing prune-before-first-upload while still releasing unused glyph pixels after the existing age.

The optional source planner reads only 128-byte PCX headers and a bounded PediaIcons list, not image pixels. Regions require valid source bounds and present palette/mask dependencies. Alpha masks retain the existing repeat semantics. Only whole small Pedia images, up to256x256, are speculatively included in keyed/unkeyed forms; this does not claim arbitrary future crops or all scenario assets are ready. Missing files and large sheets remain on demand. Names are deduplicated case-insensitively and bounded to8192 references; Pedia text is capped at4MiB. Pixel creation/upload still uses the existing art loader, per-chunk admission, atlas caps and GPU acknowledgments. No installation assets are copied.

## Coordination

preview_smear owns true seek-based FLC layout I/O and AnimFile::read. This preload change only checks cancellation between animation requests. text_perf confirmed no overlap with the narrow font methods. All scene/UI draw, text metrics, preview generations, atlas allocation/accounting/fence and GPU submission policies remain under their existing owners.

## Verification state

The initial full CPU build from immutable source3 encountered stale seeded Cargo artifacts with unchanged source mtimes. The source does contain the new host-clock shared fields; the copied cached dependency did not. The source content was hashed and mtime-refreshed before the fresh CPU build. This is a cache freshness failure, not a missing production source change.

Immutable source4 includes the final source work above, including the finite font retention fix, compact world snapshot, completion-only publisher wake and resized-indexed estimate regression. It is not yet claimed tested. Each source has a separate target; full per-file SHA256 manifests are retained under `/mnt/datasets/c3r-build/preload-sol/`.

Test plans cover existing preload byte overflow/cap tests, ordered phase admission, cap exhaustion persistence, indexed rules/output cost, fonts, unit cancellation before/midscan, source bounds/dependencies/mask repeats, scenario dimensions/missing files/sheets, 1000 coalesced requests, cancellation and phase ordering races. A read-only real WWII European Front scenario test checks many Pedia icon references and bounded admission. The normal existing text prune tests run as well.

No FPS result is claimed. The hardware-specific 4K/240 FPS goal still requires a real measurement. The completed atlas fix proves duplicate suppression in CPU state tests, not the final frame rate.

## Intermediate checks

Source3 passed all18 preload tests and3 existing text prune tests, including the read-only actual WWII European Front scenario. CPU-only; no GPU selected. Source4's new compact-world fixture used a nonexistent exported `UTA_ARMY` constant; production has a private units constant atbit0x12. The fixture was corrected to encode the documented game-rule bit. Source5 differs from4 only by this test constant and is the final test candidate. No production source was changed to accommodate this fixture failure. Source/log/hash evidence for every attempt remains.

The older failed atlas target1 was removed with Cargo's cleanup command after preserving its immutable source/log evidence. Cargo reported2119 files and1.7GiB removed. Target2 retains successful atlas CPU artifacts. Newer immutable preload test snapshots use independent targets.

## Final CPU result

Final immutable source5 passed20 preload tests and4 text prune tests,24 total, exit0. No tests were ignored in these selections. The actual installed WWII European Front scenario ran read-only and passed. This includes bounded byte accounting/overflow,1000 newest-request coalescing, stale epoch/folder rejection, later-phase ordering, disabled/between-animation cancellation, compact planning metadata preserving army/era/colour semantics, actual scenario icon dimensions, indexed resize output costs, and finite font prewarm retention through prune-before-first-upload.

Final evidence:

- `/mnt/datasets/c3r-build/preload-sol/validation5.json`
- `/mnt/datasets/c3r-build/preload-sol/source-identities5.json`
- `/mnt/datasets/c3r-build/preload-sol/source5`
- `/mnt/datasets/c3r-build/preload-sol/run-cpu5.sh`
- `/mnt/datasets/c3r-build/preload-sol/cpu-tests5.log`
- `/mnt/datasets/c3r-build/preload-sol/target5/debug/deps/ifc23-92bcca225146af49`

All four owned shared source files exactly matched the final tested snapshot at final provenance collection. The shader path in this CPU build is an existing precompiled input only; no GPU test ran and this does not validate shader compatibility or frame timing. The FPS broker's CPU lease was released.

Obsolete preload target3 and target4 were cleaned with Cargo after logs/sources/hash manifests and the passed intermediate executable identity were retained. Cargo reported3.7GiB and3.3GiB removed, respectively. These logical byte counts may share physical extents with the final seeded target5. The final preload target5 and passed atlas target2 remain available; all source/log evidence is retained.

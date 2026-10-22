# UI command transactions successor

This supersedes the earlier text-fit-atomic-ui art-residency hold design. Original handovers and frozen releases remain unchanged. Root clarified that a UI transaction is complete commands and final layout, not complete sprite loading. Root explicitly removed the old 150 ms incomplete-command release requirement. Incomplete capture retains the last committed group without waiting on the renderer thread.

## Bridge changes

- Removed UiHold and every whole-scene publication skip based on art or glyph-sheet readiness. Readiness now drives only retry scheduling and diagnostics. Missing sprites independently produce no draw command while other UI commands publish.
- UiModel::committed_frame returns only completed Present snapshots, including the empty group before the first Present. Compatibility frame/shown_frame backbuffer inspection stays available. ui_trace uses committed_frame and returns an empty committed group when capture is active before its first snapshot, preventing independently captured TextOut calls from leaking ahead of backgrounds.
- Added gdi_text::text_layout using GetTextExtentExPointA plus text/underline metrics. It measures metadata only, without a bitmap, TextOut call, glyph raster or CPU composition. Cached final character pens, ascent, descent and underline geometry feed both native 1:1 and zoomed GPU outline commands. They no longer wait for exact or alternate-size glyph-sheet metrics. Fonts::metrics also returns final metadata immediately, removing Preferences estimate-to-sheet reflow.
- Correct Windows-1252 inverse encoding accepts both original byte-valued keys and Unicode curly quotes, ellipsis and euro. The existing fonts::measure uses the same encoding. Actual lfHeight sign conversion remains exact.
- Removed the unused legacy ArtSchedule scene-hold timer/API. Art batching, immediate visible-sprite requests and diagnostics remain.
- Replaced old fill-art-dependent notice test with a no-art GPU SolidRect command test. Direct GPU solids belong to army_units; viewer GPU submission/descriptor safety and worker changes belong to their existing owners.

## Tests

New transaction tests cover first overlay background/text commit, a painted form before its first Present, old committed group during new overlay/form capture, independent omission of failed/slow sprites, and first complete background plus measured text commands with an optional icon absent and no background/glyph sheet pixels.

Text regression injects final layout metadata before any sheet metrics, then adds exact sheet metrics and named atlas art. The emitted commands and panel measurements must remain identical at native and zoomed paths. Native Windows no-assets regressions compare both lfHeight signs with direct GDI and require Unicode/raw ANSI punctuation to produce identical character pens.

Current source-focused evidence:
- ui-commands-sol-text-tests.log: 18/18 host text tests passed, including real cached-art coverage and pixel-layout tests and the new pen invariant.
- ui-commands-sol-transaction-tests.log: 6/6 command transaction tests passed, including direct solid background plus final text without optional icon art.
- ui-commands-sol-gdi-tests.log: 3/3 GDI unit tests passed, including all 256 ANSI byte round trips and Unicode punctuation.
- ui-commands-sol-gdi-windows-check.log: i686-pc-windows-gnu metadata API check passed.

Final broader bridge run is pending viewer worker compilation completion. The initial full attempt in ui-commands-sol-bridge-tests.log hit transient in-progress viewer worker errors; the owning agent was informed, and rebuild retries paused pending its ready notice. New Windows/Wine runtime tests are requested through root's coordinated successor build, not by modifying or retesting the old frozen release.

Build target: /mnt/datasets/c3r-build/ui-commands-sol-target. CARGO_INCREMENTAL=0, TILE_RENDER_ADAPTER=radv, nix develop. No Mac build, GUI, screenshot, commit, reset, stash, new Rust lock, or CPU rendering path.

Coordinated direct-solid test adaptations also made in unit_notices.rs and world_items.rs, tests only. They now assert no fill-art request and direct solid dimensions, preserving KEEP_SIZE world plate geometry under reduced cameras. Army owner requested these narrow adaptations. No production behavior in those modules changed.

Viewer worker compilation is the only current source-test blocker. The specific transient errors are undefined frame/acquire_wait, missing Layout::adopt_presented_mapping, and old/new Gpu field accesses across worker edits. No further bridge retries until the worker owner announces source ready.

Current owned-source manifest is handover/ui-commands-source.sha256. GPU solid candidate source manifest army-solid-artifacts/shader-sources.sha256 verified entirely against current source and its SPV artifact. That candidate hash is 67f3cefc9e905d7404baf63a3d0a3b54ca775bddc61612f52aa57c9b7de1ceb0. Root owns release pairing and deployment.

Final broad host bridge build compiled after worker-ready notice, using verified 67f3 precompiled SPV. Result was 403 passed, 1 failed, 1 ignored in 90 seconds. The only failure is preview::tests::optional_preview_art_can_remain_unavailable_while_the_menu_is_complete at preview.rs1009. Its old Fill label fixture still expects sprite-art residency, but direct GPU fills require no art. Preview owner delegated this fixture correction to army_units. All owned source tests passed in the broad run. The unchanged owned-source manifest verified 12/12 entries afterward. Once the fixture is corrected, only the isolated failed test needs rerun; no production change requires another broad host run.

## Final status

Army owner corrected the sole preview fixture, and its isolated rerun passed 1/1 in ui-commands-sol-preview-rerun.log. Late review also corrected the new measured-outline path to omit an absent cmap glyph independently instead of emitting glyph-zero boxes; measured advances still place later glyphs correctly. This includes unmapped control whitespace. New regression covers that case.

After the final source change, ui-commands-sol-text-final-tests.log passed 19/19, retaining all cached coverage/pixel-layout tests, and ui-commands-sol-transaction-final-tests.log passed 6/6. The broader host run had 403 passes, one corrected fixture failure and one ignored test; that one failure was rerun successfully. No remaining known host test failure. GDI tests 3/3 and i686 GDI API check passed. git diff --check passed for owned tracked files. Final 12-file SHA-256 manifest regenerated at ui-commands-source.sha256.

Requested native/Wine successor execution of game_width_preserves_the_games_lfheight_sign, measured_layout_keeps_unicode_and_raw_ansi_punctuation_pens_equal and ui_transaction:: through the existing release owner. Native test owner is currently restricted to frozen executables, so new-source native execution awaits root's coordinated successor build. Root owns deployment and actual first-frame runtime observation. No runtime result claimed here.

One existing boundary was reported to the viewer submission owner for review: captured drawing groups commit at Present, but native control rectangles are sampled independently from live memory in live.rs. No input behavior was changed in this task.

Dedicated ui-commands-sol-target removed after final validation. No other target/artifact or frozen release modified.

The native control snapshot boundary noted above is now corrected, not outstanding. See ui-present-metadata.sol.md and ui-present-metadata-source.sha256. Root reviewed the final source and authorized release freeze after passing i686 bridge check and x86_64 Windows test compilation. Three metadata boundary regressions await the prescribed frozen Wine aggregate. ui-commands-source.sha256 contains the refreshed final source hashes.

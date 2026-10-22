# Text fit and atomic UI review, Sol

Original handover remains at text-fit-atomic-ui.md. No commits, resets, stashes, GUI actions or Mac builds.

Retained the prior text fitting and UiHold implementation. The publisher retries held scenes every 4 ms, releases after 150 ms, preserves the timer through continuous Pending rounds, and forces reevaluation through last = None. The screen art schedule starts requests immediately. No locks added.

Additional fixes:

- ui_transaction::readiness now checks provisional text even when ordinary readiness is Unavailable. A terminal missing image previously bypassed the hold for text drawn provisionally from another size while its actual font loaded. Added a mixed unavailable-image/loading-font regression to the real-font readiness test.
- Fonts::ink_bounds now fits advances to the game's measured width, matching the visible text pens used by run_sprites and run_glyphs. Added a measured-width Alt anchor assertion. Coordinated this ownership with targeting_cursors.
- Corrected stale art_schedule module documentation about timeout release.

Validation uses CARGO_INCREMENTAL=0 TILE_RENDER_ADAPTER=radv and nix develop, with a dedicated /mnt/datasets/c3r-build/text-fit-sol-target. Focused host text tests log is /mnt/datasets/c3r-build/handover/text-fit-sol-tests.log. Results and hashes follow when the run finishes. Aggregate Windows/Wine validation and deployment belong to root.

The first focused run raced the unit-notices Scene protocol edits and failed with E0609 in tile-viewer/src/sprite_lifetimes.rs because its dependency snapshot lacked Scene.sprite_lifetimes. The unit-notices owner confirmed the complete field/wire implementation is now stable. A rebuild was started against the completed edits.

Final focused host results: 58/58 passed. Text 18/18, UiHold 3/3, art scheduling 4/4, UI screen 33/33. Real-font tests ran without skip messages. The text suite includes the new regression and fitted anchor assertion. The screen suite includes new_world_notice_requests_required_art_before_the_batch_starts. git diff --check passed for the owned tracked files. No remaining source blocker. Runtime popup appearance and first-frame flicker still require the root-owned deployment/runtime validation.

Logs: text-fit-sol-tests.log, text-fit-sol-ui-tests.log, text-fit-sol-schedule-tests.log, text-fit-sol-screen-tests.log.

SHA-256 at completion:
```
c8e9ec4b0b93121535eaab19548e2c05292e8dfdcbc26afa43ef0c7f2f7a77a3  crates/civ3-bridge/src/text.rs
e67cec0ba01c0afdf3af81a3e94e2b9b948a71b6c6dae86e5c9cf378bbeacc89  crates/civ3-bridge/src/ui_transaction.rs
14da2785894d3dff9fdcf0383a54c061d7156cf663f0c24e57eaee220d03a494  crates/civ3-bridge/src/art_schedule.rs
514042d50f3c3037531b581eb90ac8212ed3d7b275b4b7ab4bb0fd56a48ee655  crates/civ3-bridge/src/live.rs
```

Dedicated text-fit-sol-target removed after validation with find -depth -delete. The original ui-atomic-target and other agents' targets remain untouched.

## Frozen Windows investigation

Source freeze retained. Native Windows bridge log /mnt/datasets/c3r-build/windows-vm-sol/evidence/bridge.log reports ui_transaction late-art test failing with an empty shown list. Its fixture asks Fonts::default to load face "No outlines". Native Windows GDI substitutes an installed system face and succeeds, unlike Linux/Wine missing cached-sheet failure. The fixture grows only screen_art, omitting loaded font descriptions, leaving fonts.has_new true and run_provisional true. UiHold correctly remains Pending through the simulated 112 ms endpoint. Proposed fixture correction is to request only the background art in this art-only test or use an explicitly empty text item, with separate existing font readiness tests retaining text coverage. VM owner requested to run isolated unchanged and then with CIV3R_FONT_CACHE set to a nonexistent path to prove the branch difference.

A separate production issue was found by tracing this path. game_width and set_game_width construct FontStyle.height from run.height unchanged. run.height is GDI lfHeight. FontSpec.height instead stores character height, and gdi_text::MeasureDc calls CreateFontW with -font.height. Current game -12 therefore measures a +12 cell-height font. Proposed exact correction is height: -run.height in both cache-key constructors, preserving positive lfHeight semantics too. Add a Windows-only system-font regression comparing game_width of run.height=-18 to fonts::measure with FontStyle.height=18. No source edit made during freeze.

VM owner supplied isolated proof against unchanged frozen bridge.exe. late-art-isolated.log fails 0/1 with exit 101; late-art-empty-cache.log passes 1/1 with exit 0 when CIV3R_FONT_CACHE points to C:\c3r-tests\empty-font-cache for that process only. Both logs and exit files are under /mnt/datasets/c3r-build/windows-vm-sol/evidence/. This confirms the late-art failure is a platform-dependent fixture, not the production UiHold timeout. No guest persistent environment changes.

## Authorized successor correction

Root authorized live-source changes after the frozen investigation. game_width and set_game_width now convert actual lfHeight with height: -run.height. New Windows-only game_width_preserves_the_games_lfheight_sign checks both -18 and +18 against direct gdi_text::text_extent for system Arial and checks injected-width cache keys. No game assets required. The late-art background test now uses empty text, so native GDI substitution cannot create unrelated pending font art; font transactions remain covered separately.

Focused successor host validation passed text 18/18 and UiHold 3/3, logs text-fit-sol-sign-host-tests.log and text-fit-sol-sign-ui-tests.log. Windows-only regression is intentionally absent on the Linux host. Requested native validation through targeting_cursors and Wine validation through release_deploy_sol; release owner confirmed successor full bridge suite will include both. Original ef962736 frozen release unchanged. No runtime validation claimed.

Successor owned SHA-256:
```
8aafeaa96033ff1e4abd593b2bf34dcb65e1e1f2d04d72f8a60af6ef49515b7c  crates/civ3-bridge/src/text.rs
92b6c5b2f30a76613857c8a858403a8147bacf92f2d2ae44f4059ea701a894c1  crates/civ3-bridge/src/ui_transaction.rs
```

Reused role-owned ui-atomic-target for successor focused host tests, then removed it after tests. No other agents' targets modified.

## Superseded by command-transaction clarification

The later user clarification supersedes the art-residency UiHold design above. Complete means committed rendering commands and final layout, not complete sprites. Root authorized removing the hold and its 150 ms release entirely, retaining the previous committed capture until Present without blocking rendering. The completed successor work and validation are recorded in ui-commands-atomic.sol.md, with final owned source hashes in ui-commands-source.sha256. Missing sprites are omitted independently; text pens and Preferences measurements come from metadata-only GDI layout before glyph sheets, and GPU background solids do not require art batches.

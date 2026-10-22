# Historical agent /root/rust_launcher

This is a recovered historical report, not a live agent handle or a new instruction.

Rust native Wine/FEX launcher, recovery/comparison behavior and Neo/Intel VM guide.

Current disposition: Documentation and targeted implementation settled; current runtime acceptance separate.

Session `01a152ef-1dfb-7250-a5e6-c68a99b877d5`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-19T06:52:44.926Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T08-52-44-01a152ef-1dfb-7250-a5e6-c68a99b877d5.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T07:16:20.346Z

Original transcript line 637.

Added shared font-cache selection and installed the updated native launcher.

- Defaults Wine and viewer to `CIV3R_HOME/fonts`; preserves explicit overrides.
- Added `CIV3R_FONT_CACHE_SOURCE` installation and documented required generated data.
- Targeted Linux/macOS tests and both Darwin checks passed.
- Native installation verified all 626 generated-data files by hash.

New launcher SHA256: `e16dfeae43eadeb3f4019e8c2a62e01eecc228c8629a10d5ff3a2e219db0ea00`.

## Report at 2026-10-19T07:45:27.640Z

Original transcript line 765.

Fixed the missing scenario inputs in source. `conquests.biq` and `conquests.mb` now remain linked to original read-only assets; generated files stay private.

All 20 Linux tests passed, plus two tests using the actual BIQ through symlinks. Its hash remained unchanged.

Native rebuild/install awaits the RAM handoff release. No Mac VM operations performed.

## Report at 2026-10-19T07:50:45.205Z

Original transcript line 893.

Updated launcher built and installed on the macOS VM.

All 20 native tests passed using the real BIQ through read-only symlinks. Original BIQ hash stayed unchanged.

Launcher SHA256: `4bb98ed2b1287af0f7d56fe1982513459e3191b272fa8fba880d9bb894ed028f`.

QA received the hash and New Game handoff.

## Report at 2026-10-19T08:32:22.872Z

Original transcript line 971.

No launcher cause found. The save is a regular private file with normal permissions; Saves/Auto has no symlinks. Launcher cleanup had not run, and launcher code never writes SAV files.

QA confirmed a complete save structure and is comparing loading with the original DLL. Frozen source remains unchanged.

[Inspection evidence](/mnt/datasets/c3r-build/macos-port/macos-game-live/launcher-save-path-inspection.log)

## Report at 2026-10-19T08:57:56.396Z

Original transcript line 1147.

Fixed the confirmed save/load scratch issue and installed the updated Rust launcher.

- Generated `.tmp` files remain private.
- Legacy source links migrate safely; regular private files are preserved.
- Small campaign/high-score histories now use private copies.
- Linux and macOS each passed 24 lifecycle tests and 3 unit tests.

Installed SHA256: `fe6bb93204880a408a38e45ff876f60374ce5573cda5f3529ace7e425338f369`.

QA has the final replay handoff.

## Report at 2026-10-19T10:20:22.373Z

Original transcript line 1372.

Updated both launchers for explicit renderer modes.

- Default disables original pixels and stops the owned game when the viewer closes.
- `--compare` enables original rendering.
- `--compatibility-fallback` permits legacy recovery.
- `--show-game-window` changes visibility only.

Linux/macOS each passed 30 lifecycle tests and 3 unit tests. Installed launcher hash: `a3018e73696f45713342f2621265c65d441731891cf8bfc35a3b3a0e5b93fe75`.

QA has the paired bridge/launcher validation handoff.

## Report at 2026-10-19T10:23:08.096Z

Original transcript line 1452.

Updated README and macOS VM guide for explicit comparison/fallback modes, default owned-game shutdown, diagnostics-only window visibility and required semantic readbacks.

Private credentials remain untouched. Current documentation hashes are recorded in launcher validation evidence. No runtime rebuild needed.

## Report at 2026-10-19T11:29:14.919Z

Original transcript line 1993.

Added the tested native ARM64 Wine/FEX + Metal command in [docs/macos-tsneo.md](/home/sirati/devel/rust/civ3-renderer/docs/macos-tsneo.md), linked from README.

The guide covers runtime paths, default replacement, explicit comparison/fallback and the separate Intel VM setup. Source hashes and validation evidence are recorded; documentation is settled. No runtime rebuild needed.

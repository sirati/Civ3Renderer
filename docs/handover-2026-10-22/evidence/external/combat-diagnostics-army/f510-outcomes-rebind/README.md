# Diagnostic outcome overlay rebound to exact f510

This is a separate source-only package. It does not change shared sources or any previous proposal/test evidence. No compiler, game, GPU or VM was run.

Frozen base is `/mnt/datasets/c3r-build/release-1019/src-f51034b34b94eeb3bc3a8dcfbd2e4faf8eabc6350721b7f7172da7b406504703`. Both `base/` and `candidate/` began as exact copies of that frozen source. The prior diagnostic patch was context-applied to this fresh candidate, rather than copying old whole candidate files over it.

`pre-rebase-leaf-comparison.json` compares every prior diagnostic patch leaf to its old base and f510 before rebase. Only existing `crates/civ3-sys/src/lib.rs` drifted. Its old base lacked `pub mod audio_init;`; f510 includes it. Two diagnostic modules are new files. All other existing diagnostic leaves, including Cargo.lock, matched f510 exactly.

`verification-manifest.json` verifies all 673 frozen manifest entries, records every candidate file hash and the exact nine diagnostic changes, and rejects unexpected changes or removal. Removing only the diagnostic module declaration from the rebound sys lib reproduces exact f510 lib bytes, including `pub mod audio_init;`. Audio-init implementation and bridge lib/live remain byte-identical to f510. Every launcher/process source leaf remains byte-identical, preserving the current launcher/cancellation implementation. The candidate's copied SOURCE-MANIFEST.json identifies the frozen base only; this external verification manifest binds the diagnostic candidate.

Apply `f510-combat-diagnostics.patch` only to a verified copy of this exact frozen base. Its application check passes against `base/`. The complete rebound candidate is available for inspection and eventual isolated builds, but it must not be used as a replacement for an independently changed shipping checkout. The patch is the diagnostic-only delta, including the optional sha2 lock entry and feature gate.

Diagnostic implementation and prepared tests are unchanged from the prior outcome proposal. Refer to `../outcomes-successor/README.md` for native field/status/identity semantics and eleven prepared checks, and the independent review `/mnt/datasets/c3r-build/handover/combat-outcomes-independent-review.startup.sol.json` for the logic review of that proposal. The new binding requires independent source-identity review and a separate broker compiler grant. No prior check result is relabeled as a pass for this rebound source.

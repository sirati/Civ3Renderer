# Unit notices continuation (Sol)

Original `unit-notices.md` preserved. Work is uncommitted in the shared tree.

Implemented the inherited map-message timestamp decode and publisher tracker, plus actual viewer-clock expiry required by root. Dedupe keys are tile/text and new game tells are detected by slot/started/tile/text. One notice per tile, eight total, game duration 1..5000 ms or 2500 ms fallback. Modal notices do not renew or reinsert themselves when polled, fixing an eviction/reinsertion bug in inherited work.

Protocol now VERSION 47. Added generic `Scene.sprite_lifetimes`, `SpriteLifetime { serial, sprites: (start,end), duration_ms }`, and `Update::SpriteLifetimes` wire tag 27. All serials in a producer session must increase; repeating a serial never renews its timer. Scene apply/diff/Changes and wire encoding/decoding updated. No FreeSprite construction changes.

Bridge includes lease serials in the unchanged-scene comparison, so another identical notice can refresh its timer even when text/art is unchanged. It emits individual notice world sprite ranges after screen/world partitioning, preserving their indices. Viewer owns monotonic deadlines, masks expired clips only in derived world.free render lists, preserving canonical Scene clips, marks sprites changed for GPU rebuild, preserves unrelated sprites and all sprite indices. High-water serial prevents stale replays after expiry/removal from reviving notices. Tracker memory is bounded by currently listed lifetimes, with one scalar high-water value. Lifetime-only renewals restore original clips without requiring another Sprites update. No stale-world snapshot reuse. Snapshot-error continue remains safe for notice expiry because viewer expiry needs no incoming updates.

Files owned/edited: civ3-bridge/src/unit_notices.rs, live.rs notice state/layout/range publication, inherited modal.rs notice lifecycle, civ3-sys/src/map_messages.rs inherited timestamp decoding; scene/src/lib.rs and wire.rs narrow lifetime metadata; tile-viewer/src/sprite_lifetimes.rs new, tile-viewer/src/lib.rs App initialization/field and draw-frame expiry; docs/unit-map-notices.md.

Coordination: root authorized scene/wire change and VERSION46->47. scenario_crash, slider_arrow_keys, targeting_cursors and text_fit_atomic_ui notified. Edits coherent; earlier peers saw transient E0609 when Scene compiled before edits and viewer after, rerun rebuilds Scene.

Focused regression tests added: unit notice modal polling does not evict newer game notices or renew leases; identical new tell renews serial; viewer expires with no incoming scenes, keeps unrelated sprites, refuses stale serial replay even after omission, accepts new serial; scene lifetime wire roundtrip/diff/apply/clear. Inherited 50-spam/extended lifetime, frozen game records, per-tile/cap tests retained.

Validation on papa with own target `/mnt/datasets/c3r-build/unit-notices-sol`, `nix develop`, CARGO_INCREMENTAL=0, TILE_RENDER_ADAPTER=radv:

- `cargo test -p civ3-bridge --lib unit_notices::tests -- --nocapture`: 10 passed. Log `unit-notices-sol-bridge-tests.log`.
- `cargo test -p civ3-sys --lib map_messages -- --nocapture`: 4 passed. Log `unit-notices-sol-sys-tests.log`.
- `cargo test -p scene -p tile-viewer --lib sprite_lifetimes -- --nocapture`: one wire test and two viewer expiry/renewal tests. Log `unit-notices-sol-lifetimes-tests.log`.
- Scoped `git diff --check` passed.
- Source SHA256 evidence in `unit-notices-sol-source.sha256`. Shared files include other agents' edits, so later peer changes can alter their hashes.

Root owns aggregate Windows/Wine tests and deployment. Windows-only live.rs publication code still needs that aggregate check. No Mac builds or GUI operations. All five release artifacts must use VERSION47 together.

Review follow-up from root addressed: initial implementation masked shown_scene clips; changed to immutable canonical Scene observation plus masking only derived render lists. New regression applies exactly `Scene.apply(Update::SpriteLifetimes(new serial))` after expiry, with no Sprites update, and proves identical art restores its original nonempty clip. Bounds are clamped to the world sprite count, so lifetimes cannot hide overlays.

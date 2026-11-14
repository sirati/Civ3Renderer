# Archived workspace

This checkout preserves the original monorepo and migration history. Active
development belongs in the separate sibling checkouts under
`/home/sirati/devel/rust`:

- `tilerenderer`: GPU tile rendering, GPU text and shared build support.
- `tilerenderer-gameengine`: generic viewer, layout, input and scene protocol.
- `civ3renderer`: Civilization III integration and application shader selection.
- `glass-shader-rs`: glass host implementation and shader library.
- `pointer-control-rs`: pointer APIs and platform backends.
- `wine-unixlib-rs`: Rust interfaces to Wine's existing unixlib mechanism.
- `inproc-rs`: guarded Windows memory access and in-process hooks.
- `nix-osx-kvm`: source-only VM configuration and Python/Nix management.

Use SSH for Git repository access. Do not copy components back into this
archived workspace to bypass cross-repository dependencies. Migration
reports and preserved history may still be maintained here.

The generic game engine must not depend on glass packages or shaders. The
Civ III application selects that effect through the engine's extension
interface. Preserve the user's stub-and-replace requirement for legacy
GDI/jgl drawing; do not grow a CPU canvas-pixel reconstruction engine.

The dataset inventory is
`docs/evidence/repository-split-20261114/dataset-inventory.json`. Its marked
Cargo directories are cleanup candidates; the mixed build tree, source
snapshots, evidence, backups and VM/game runtime data are not disposable as
one directory. No dataset files were deleted during this audit.

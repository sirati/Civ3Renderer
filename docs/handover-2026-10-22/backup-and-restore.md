# Renderer backup and restore

The configured origin is the missing local path `/home/sirati/Projects/civ3-renderer-old`. The user's standing fallback is a committed Git tree on Papa's laptop and Mac Neo. Keep existing backups immutable and create successor checkpoints for new work.

## Existing source backups

| Location | Directory |
| --- | --- |
| Local archive | `/mnt/datasets/c3r-source-backups/20261020T1918Z` |
| Papa's laptop, SSH alias `tspapalaptop` | `/home/sirati/civ3r-source-backups/20261020T1918Z` |
| Neo, SSH host `sirati@siratis-macbook-neo.tailb624ab.ts.net` | `/Users/sirati/civ3r-source-backups/20261020T1918Z` |

Both hosts were reachable during handover preparation on 22 October and their existing main mirrors reported `8106ae5a4debbe247cb5e64523d4972474f4fe39`. The 20 October receipts preserve the full original hash/ref/fsck checks.

| File or repository | Contents |
| --- | --- |
| `civ3-renderer.git` and `civ3-renderer.bundle` | Main source, branches, tags and available history. Bundle SHA-256 `8135d86e5019319a720e312abee121a337e6710a92b6352a0738960b441a9d4e`. |
| `civ3-renderer-git-metadata.tar.zst` | Raw Git configuration, reflogs and worktree registration metadata. Prefer a normal clone for routine restoration. |
| `development-source.git` | Separate committed private renderer experiments, QA harnesses, patches and historical handovers. Main snapshot `6162a7debb8adcc78eac147611eeb1539c7e78c8`. |
| `development-source-current.bundle` | Complete successor bundle including `checkpoints/loaded-owner-driver-20261020` at `ce5e104270258e207e16c3a8cf7a0ff5336c41d5`. SHA-256 `7c6c204751b0f0835ccc57db01adf528c2527eabd102949145da42fb7d3c173a`. |
| `development-source.bundle` | Earlier private snapshot. It lacks the later QA checkpoint. Use the current bundle for the latest work. |
| `fex-git-tree.tar.zst` | Separate clean FEX checkout and Git metadata. Its original uninitialized submodules remain uninitialized. |
| `RESTORE.md`, manifests and verification receipts | File hashes, refs, restoration results and original-path mapping. |

The private snapshot preserved 55,602 source/harness/text files. Root checked complete active manifests for f510 source, binary-cell source, wider-color source and combat source. Those files are not automatically accepted production code.

## Handover successor backup

The handover checkpoint uses a new directory, `20261022-vm-window`, under the local and Papa-laptop backup roots. Neo replication is pending while Neo is unavailable; no connection to Neo is allowed during this investigation. Its `civ3-renderer.bundle` contains the committed documentation and all available main-repository refs. Its `backup-receipt.json` records the actual documentation commit, bundle SHA-256, restored main ref, and per-host integrity verification. Consult that receipt for the completed transfer result. It is outside the Git commit whose identity it records.

The previous development/FEX archives remain in `20261020T1918Z`; do not assume the new main bundle contains them. Retain both backup directories together.

## Restore into a new directory

Run these from a directory containing the downloaded bundles. Each clone creates a new checkout and leaves the current workspace alone.

```bash
git clone civ3-renderer.bundle restored-renderer
git -C restored-renderer status --short
git -C restored-renderer log -1 --oneline
git -C restored-renderer fsck --full

git clone development-source-current.bundle restored-development
git -C restored-development show-ref --heads
git -C restored-development switch checkpoints/loaded-owner-driver-20261020
```

Clone the private snapshot's `main` branch separately, or switch back to it, to read `SOURCE_INDEX.json` and recover original paths. The QA branch is a distinct checkpoint. Verify archive SHA-256 against its receipt before extracting raw Git metadata or the FEX archive into a fresh directory.

## Existing worktrees

The registered worktrees at handover preparation were:

| Path | Commit or branch |
| --- | --- |
| `/home/sirati/devel/rust/civ3-renderer` | Main implementation baseline `8106ae5`, followed by documentation commits. |
| `/mnt/datasets/c3r-wt/perf` | `perf` at `22cd2e4`. |
| `/mnt/datasets/c3r-wt/perf-baseline-reference` | `921bf96`. |
| `/mnt/datasets/c3r-wt/perf-final-preflight` | `2c2a22f`. |
| `/mnt/datasets/c3r-wt/retire` | `9e497cd`. |
| `/mnt/datasets/c3r-build/perf-scratch/base` | Prunable registration at `3fc9dcb`. |
| `/tmp/c3r-zoom` | Prunable registration at `22cd2e4`. |

Do not prune registrations or dangling checkpoints as part of documentation work. Commit objects and unique saves were preserved during earlier cleanup; read the retained cleanup inventories before removing old QA prefixes or scratch trees.

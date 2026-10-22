# Renderer source backup, 2026-10-20

The main repository checkpoint is 8106ae5a4debbe247cb5e64523d4972474f4fe39. It contains the exact 673-file f510 production source checkpoint and all current shared source changes. The existing configured origin was a missing local path, /home/sirati/Projects/civ3-renderer-old. No working upstream could be pushed.

The backups preserve all available main Git history, branches, tags and worktree commit objects. Clone the local bare mirror with `git clone civ3-renderer.git restored-renderer`, or use `git clone civ3-renderer.bundle restored-renderer`. Both commands create a new directory. The bundle needs no external history. The raw Git metadata archive additionally preserves local configuration, reflogs and registered worktree metadata; prefer the bundle or bare mirror for a normal restore.

`development-source.git` and `development-source.bundle` preserve the separate committed snapshot of private renderer experiments, QA harnesses, patches and handover records. Clone it to a new directory. Its SOURCE_INDEX.json records captured source hashes and original paths under /mnt/datasets/c3r-build. These experiments remain separate from accepted production code. Current frozen, binary-cell, wider and combat source manifests were checked in full before backup.

`fex-git-tree.tar.zst` preserves the separate clean FEX checkout, including its Git metadata and available source. Extract into a new directory. Its original uninitialized submodules remain uninitialized; this archive does not claim to include their missing content.

`backup-manifest.json` records bundle/archive identities. Host verification records report exact remote SHA-256, Git refs and fsck results. No game assets, bulk compiler caches, VM disks or installed game runtime were transferred as part of these source backups.

# Historical agent /root/macos_pointer/wayland_backend

This is a recovered historical report, not a live agent handle or a new instruction.

Wayland physical-pixel capture, exclusions, lock hints and focus-aware constraints.

Current disposition: Six focused tests passed; live compositor coverage separate.

Session `01a152b4-fff9-70a2-b3f5-d7dedac96e4d`, parent session `01a152ae-73c1-7662-8ba9-ee298b05d6a5`, depth 2. Started 2026-10-19T05:49:16.159Z. Original transcript `/home/sirati/.codex/sessions/2026/10/19/rollout-2026-10-19T07-49-16-01a152b4-fff9-70a2-b3f5-d7dedac96e4d.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-19T05:49:16.163Z

Original transcript line 8.

Implemented and signed as `3bb9356c034239eb7cee908286e74d3ea39e5a06`.

Map edges now repeat adjacent in-map terrain instead of inventing inland coastlines. Real water, wrapped seams, and edge brightness remain correct. Six focused edge/art tests pass.

Validation completed:

- Full workspace, viewer, Wine/i686, release, and launcher checks.
- VM gameplay reached 4000 BC; Alt controls, off-camera units, and city popup verified.
- VM shut down and private overlay cleaned.
- No Nexus checkout or game assets touched.

The laptop is currently offline on Tailscale, so activation could not complete. Renderer-only artifacts and an automatic retry are prepared at `/mnt/datasets/c3r-artifacts/continuation-20261018/viewport-ui-fixes/3bb9356c034239eb7cee908286e74d3ea39e5a06/`. The running game was not interrupted; restart is required after upload.

## Report at 2026-10-19T05:49:16.163Z

Original transcript line 18.

**No, I cannot yet confirm that everything across the entire history is fixed and fully tested. My previous answer overstated completion.**

The recovered pre-compaction work and subagents were included. The identified requests have implementation and automated-test evidence, covering UI, cameras, Alt controls, mouse capture, menus, units, clean maps and map edges.

**Yes, I also investigated related problems and fixed those I found**, including:

- Stale G/right-drag destinations when the camera moves beneath a stationary pointer.
- Pending movement destinations breaking after zoom changes.
- Old AI focus overriding a later unit selection.
- City panels incorrectly inheriting world clean-map settings.
- Hidden menu controls receiving input.

However, work remains before claiming full completion:

- Several original reproductions still lack live validation on the current build.
- A VM missing-art error recovered automatically, but its cause remains unresolved.
- Current-build performance and the final laptop NVIDIA check remain unverified.
- Laptop deployment remains unconfirmed; today's SSH attempt timed out.

The [request-by-request audit](/mnt/datasets/c3r-artifacts/continuation-20261018/viewport-ui-fixes/request-coverage-audit-3bb9356.md) records the coverage and outstanding gaps.

## Report at 2026-10-19T05:49:16.163Z

Original transcript line 25.

Removed about **39 GiB** of rebuildable caches and cleaned stopped QA overlays. The system disk now has **116 GiB free**.

Source checkouts, game files, saves, backups, deployment binaries, and test evidence are preserved.

## Report at 2026-10-19T05:49:16.163Z

Original transcript line 32.

Yes. Currently, the Windows game runs through Wine, with our renderer running natively on Linux.

**macOS looks feasible, but the current build is not Mac-ready.** I checked your MacBook Neo:

- **Metal:** A native ARM64 graphics probe passed on its A18 Pro, including shader compilation, drawing and pixel readback. Our graphics library [supports Metal](https://github.com/gfx-rs/wgpu).
- **Cursor locking:** macOS supports it, and our window library implements it. Actual capture behavior remains untested.
- **Cursor confinement:** Our library [doesn’t implement macOS confinement](https://docs.rs/winit/0.30.13/winit/window/enum.CursorGrabMode.html). Pane boundaries, zoom-area exclusion and single-axis movement need Mac-specific work.
- **Game integration:** The native bridge currently uses Linux-specific code. macOS also requires different window-thread handling.

The promising design is the Windows game under Wine, connected to a separate native ARM64 Mac renderer using Metal. That would keep rendering native on Apple Silicon.

The full game, input behavior and performance still need implementation and testing. The successful Metal probe establishes the graphics foundation, but doesn’t establish that macOS will be faster.

## Report at 2026-10-19T05:52:15.605Z

Original transcript line 166.

Implemented `crates/pointer-wayland` with physical-pixel coordinates, Wayland lock-and-hint movement, fallback grabs, focus-aware confinement, exclusions, and single-axis constraints. Pending placement survives grab/release cancellation.

All six focused tests passed using `nix develop` and the shared target directory. No other files edited; no commits.

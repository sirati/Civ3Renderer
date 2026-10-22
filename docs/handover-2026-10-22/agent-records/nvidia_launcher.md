# Historical agent /root/nvidia_launcher

This is a recovered historical report, not a live agent handle or a new instruction.

Explicit NVIDIA PCI selection and an asset-free headless verification helper.

Current disposition: Historical preparation only; current AMD-only constraint supersedes any NVIDIA run plan.

Session `01a14f49-e1fb-73c1-9254-42738216b6f8`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-18T13:53:24.479Z. Original transcript `/home/sirati/.codex/sessions/2026/10/18/rollout-2026-10-18T15-53-24-01a14f49-e1fb-73c1-9254-42738216b6f8.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T14:00:18.517Z

Original transcript line 184.

Fixed `--nvidia` startup crash. The launcher hid NVIDIA’s EGL driver, which broke Vulkan initialization during concurrent Wine/renderer startup.

Verified on the laptop with five concurrent Vulkan initialization runs and successful device creation on NVIDIA and Intel. Four launcher regression tests pass.

Changes are ready for integration. Full game presentation on NVIDIA remains untested; nothing was deployed or interrupted.

## Report at 2026-10-18T14:03:36.958Z

Original transcript line 254.

Ready for the final build.

- Fixed Quadro selection by PCI vendor; two regressions pass.
- Prepared an asset-free offscreen GPU test helper at `…/nvidia-launcher/run-final-headless.sh`.
- It verifies actual Quadro shader draws and GPU readbacks against the CPU renderer without opening windows or touching the game.

Awaiting the final SHA and test executable.

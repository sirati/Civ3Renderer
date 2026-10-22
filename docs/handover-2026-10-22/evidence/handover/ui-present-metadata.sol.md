# Native UI metadata at Present

Final source stable. No further source changes planned. Root and scenario_crash independently reviewed the implementation without finding. Root prescribed execution of the three new tests in the next frozen Wine aggregate, so prefreeze Wine execution is not required or claimed.

ui_trace::NativeUi captures native controls, title layout, military strips, status row and resolved control look recipes on the game thread at Present. CommittedCapture freezes the completed UiModel, including sprite source mappings, and native/city metadata together. WANT_VIEW cannot refresh these from uncommitted game state. Before the first Present, all committed commands and targets are empty. SnapshotSources binds dynamic lit-order sprite resolution to the one immutable snapshot used by live. Live consumes this metadata instead of re-reading native geometry and includes NativeUi in its unchanged key. Metadata-only changes wake the publisher. Live pointer/targeting updates and modal-close authorization remain live intentionally.

No locks, art/font waits, renderer/GPU changes or CPU rendering added. Only live.rs and ui_trace.rs changed in this follow-up. Existing published draw/Present semantics remain unchanged.

Evidence:
- ui-present-metadata-sol-i686-check.log: current Windows i686 bridge check passed.
- ui-present-metadata-sol-windows-build.log: x86_64 Windows bridge test executable compiled successfully, including present_snapshot_tests.
- ui-present-metadata-sol-tests.log: Linux filter ran zero tests because ui_trace is Windows-only. This is not runtime test coverage.
- ui-present-metadata-source.sha256 records both changed source files and retained Windows test binary at /mnt/datasets/c3r-build/ui-present-metadata-artifacts/present-tests.exe.
- ui-commands-source.sha256 refreshed with final 12-file command/layout source hashes.

New regressions exercise the actual CommittedCapture helper: empty commands/targets/sources before first Present; old commands/control/title/military/status/look metadata and sprite mappings through an intervening WANT_VIEW; atomic next-Present advance; and metadata-only publication changes. Three tests require Windows and await the prescribed frozen Wine execution.

Dedicated ui-present-metadata-sol-target is removed after build validation. Retained binary and logs remain for evidence. Original frozen releases are untouched.

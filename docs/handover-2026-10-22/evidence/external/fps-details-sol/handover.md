# Detailed counter

Settings `display.fps_detailed`, default false, works with original show_fps master. Detail control enables master; disabling detail keeps master. Preferences exposes second checkbox and explanatory caption.

Four rows: original FPS; CPU build; GPU; GPU gap ~. Unsupported/stale values n/a. Existing 24 glyph indices preserved, ten new glyphs appended, four asset grids extended to34.

CPU build sums preparation wall duration on main and worker for exact accepted/presented packet ID. It excludes acquisition, submission, presentation and worker mailbox time. This is not CPU consumption. App commit_ui uses matching pending record; displaced packets never become CPU samples. Worker samples after encoder.finish.

GPU timestamps are asynchronous with nonblocking device Poll. FrameTimes preserves packet ID. Timer instrument IDs include missing/empty/abandoned attempts and full-ring skips. Gap requires adjacent timestamp IDs on same positive period clock. Ordered readback blocks newer ready results behind earlier pending results. Readback failure frees slot. Multiple encoded buffers may be finished then submitted once; pending list maps all submitted buffers. No GPU waits introduced for timing.

App resets floor on timing mode changes; old in-flight packet results are rejected. Worker recreates timer on disabled->enabled. Profile may keep timer active, but App reset still prevents old details results.

Focused tests: settings23, tile-render timing3 actual RADV, render-desc glyph1, tile-viewer fps12 passed. Viewer12 passed again after localized worker final changes, fps-final.log. Early accepted draw attempts returning Retry now discard timing attempt, so no gap spans a no-command attempt. Dedicated build target fps-details-sol-target on Papa RADV, no GUI.

Source ownership: settings, fps.rs, render-desc glyph append, asset prefs/grids, gpu timestamp capability, renderer disable_timing, timing.rs. Worker and App actor integration owned by startup_progress_sol/tile_popup_render_shadow; reviewed common finish_frame/discard/submitted/collect hooks and accepted-ID preparation sums. README adds metric semantics.

## Profiling compatibility follow-up

Restored worker-owned profiling after actor move. State.profile now initialized for TILE_VIEWER_PROFILE and run loop round/report hooks log atlas CPU cost/byte counts, total draw wall cost, whole/reused frame events. Separate State.gpu_profile consumes asynchronous timestamp results, one profile round per FrameTimes, preserving span/pass sum/pass count/per-label durations and increments. No host readback arrival interval is misrepresented as GPU pacing. No waits or timestamp readback changes. Original main profiler, frame_times logs and detailed build measurements preserved.

Regression test delayed_gpu_results_remain_separate_profile_rounds passed, including final reuse-event addition. Log profile-final.log. Full viewer test binary compiled with concurrent recovery fields and latency integration at that snapshot. README documents separate worker and GPU execution summaries.

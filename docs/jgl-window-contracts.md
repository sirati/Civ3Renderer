# Group 7 window and pump contracts

This continuation owns the window/pump implementation and standalone checks.
It merged `83caad24ed9dabe1e899d29fb5ebd9ef4b466e3f` into the app branch
`t3/finish-jgl-groups-switch-4`, preserving the parent's documentation.
`[V]` means the specified binary inspection or test passed. It does not mean
the integrated game acceptance matrix passed.

The evidence root is `/mnt/datasets/c3r-build/jgl-t3-12959774`.
`audit/inputs.sha256` binds original jgl, SHA-256
`0b0cd514de0d95b93d20655f4b5194173fe257325af82e558a152305ff0dbdf2`.
`audit/graphics.asm` retains the disassembly at RVAs `3B290..3D900`.
The original and owned code call the same substituted Win32 functions in
`jgl-ffi/tests/owned_window.rs`; no original object is passed to an owned
slot. `owned_window_live.rs` separately compares actual Win32 queues and
hidden painting in an owned prefix on a private Xvfb display. Neither test
starts Civ3 or a viewer, changes installed game DLLs, or moves a cursor.

## Per-contract matrix

| Contract | Original return, effect and ABI | Owned answer and evidence |
|---|---|---|
| Graphics constructor, `3B2E0` / `3C720` | Initialize the documented window fields and reset the global COPYDATA callback. Export is cdecl with no arguments. | Callback reset corrected. [V] Original/owned fresh-object callback comparison. The uninitialized `+13C` field is excluded. |
| `+00/+0C` object release | Graphics deletion and the circular registry release belong to the settled group-6 lifetime work. They do not substitute for +08's window shutdown. | Bodies retained; not reopened or claimed as new group-7 lifetime evidence. |
| `+04`, `3B400` | Thiscall, 16 stack bytes. Store instance/controller/owner before class registration. Class `JackalClass`, style `2B`, black brush, no cursor, icon resource's low word with application-icon fallback. Register/create failure returns 1. Query height then width for CreateWindowExA; after creation query width then height again. Set depth 8 and transparent DC, call owner slot 0 with `(w,h,8,1)` and ignore its result. Return 0, even if GetDC failed. | Screen sampling/order corrected. [V] Failure, icon fallback, changing-screen, no-DC and success cases in deterministic test. |
| `+08`, `3B390` | No stack arguments. Destroy non-null HWND, zero it, call non-null owner's `+08`, zero owner, unregister class, zero instance. Return UnregisterClassA's BOOL. Retain dimensions and DC field. Display restoration is already suppressed by the accepted display policy. | [V] API sequence, owner callback, return and remaining fields compared. Does not invent WM_QUIT or free the window DC early. |
| `+10/+14/+18/+1C` | No arguments. ShowWindow commands `1/0/6/3`, return its previous-visibility BOOL. | [V] Exact commands/returns in deterministic test. The existing hidden-window adapter changes commands to SW_HIDE when hiding is enabled. |
| `+20/+24/+3C` | No arguments. Return HWND, HINSTANCE, window DC. | [V] Getter values compared. Live handles identify different owned windows and are normalized only where needed. |
| `+28`, `3BC10` | 16 stack bytes. SetWindowPos with HWND_TOP, supplied signed position/size words, flags zero; return its BOOL. | [V] Arguments/return compared. No added show or capture operation. |
| `+2C`, `3BBD0` | 8 stack bytes. GetCursorPos, ScreenToClient, write x/y, return y. | [V] Deterministic coordinates and return compared. Pointer override uses the same inproc client-to-screen answer as the old import hook. It never warps the system pointer. |
| `+30`, `3BBB0` | 4 stack bytes. InvalidateRect with erase FALSE, return its BOOL. | [V] API arguments/return compared. The same hidden-paint adapter receives invalidations and BeginPaint on both paths. |
| `+34/+38`, `3BB90/3BB70` | 8/4 stack bytes. Write stored dimensions and return height; GetWindowRect returns its BOOL and rectangle. | [V] Outputs, effects and returns compared. |
| `+48/+B4` | No arguments, return zero. | [V] Compared. |
| `+4C/+98` | Bare `ret` / `ret 4`; EAX is undefined. | Owned zero is not claimed as a defined original return. These no-op slots perform no window effect. |
| `+50`, `3B7B0` | 12 stack bytes. Call virtual +68, select the first strict signed-frequency maximum matching width/height/depth, call virtual +64 with its index or -1; always return zero. | Empty-list shortcut removed. Selection is safe `jgl::display`; the adapter reads the actual virtual list. Differential cases include tied frequencies, overridden list/index methods and no match. |
| `+54`, `3B9B0` | 4 stack bytes. Query width then height, call virtual +58 with requested depth; preserve its result. | Virtual delegation and a non-Boolean result compared. |
| `+58/+5C/+60` | Accepted display policy answers valid requests with original +60's keep branch, preserving its owner result. Raw +58 keeps the first strict frequency maximum but stops at a later higher frequency above 75. Raw +5C prefers the first exact refresh, otherwise the strict maximum; its low-byte keep branch delegates to virtual +60 even for invalid dimensions. | Wrappers retained. Constant-error fallback bodies removed. [V] Original/owned fallback scans compare OS order, ties, first/late frequencies above 75, negative frequency words, no match, invalid-dimension keep and high-byte keep flags. Real-window dimensions match. |
| `+64`, `3B9E0` | 4 stack bytes. Save current mode at object +4 before checking the index. Out-of-range nonnegative index returns 3. A valid mode updates screen fields and calls the owner after the suppressed display change. | Constant-error shortcut removed. Uses the enumerated mode and preserves the owner's result. |
| `+68`, `3B560` | No arguments. Re-enumerate from index zero, keep duplicates and OS order. Return a stable array header `{vtable,data,capacity,count,growth=1}` and 20-byte records `{w,h,w,bits,frequency}`. Capacity grows by one and survives shrinking/empty refreshes. | Constant empty list removed. Safe `Modes` owns entries/selection; the shim supplies DEVMODEA queries and the ABI header. Differential tests compare logical entries, count, capacity and growth, with pointer identities normalized. |
| `+6C/+70`, `3BC40/3BCB0` | 8/0 stack bytes. Save current movie mode at +98, request movie mode/restore. Under existing display suppression both return zero. | Movie-mode save restored. Both leave the actual display unchanged. |
| Graphics stubs | 24 graphics slots return `18` and pop their catalogued 0/4/8/12/16/24 stack bytes. | [V] Every stub compared through the x86 register/stack probe. |
| WndProc, `3C730` | Stdcall, 16 stack bytes; signed mouse/size/hit-test words, signed wheel delta; full key/character words. Controller thiscall slots `00..34` use the recovered argument counts. | Routing and callback ordering now run in safe `jgl::window::handle`. Deterministic sweep covers every message `000..500` at zero and word/sign boundaries, plus targeted arguments. |
| WM_SIZE/CHAR/move/buttons | Notify controller and return zero; double-click flag precedes coordinates. Middle buttons use DefWindowProcA. | [V] Exact slot/argument/order/return comparisons. |
| WM_NCHITTEST | Return controller +08's full signed LRESULT. | [V] Nontrivial callback result compared. |
| WM_KEYDOWN/UP and SYS variants | Notify controller +0C. Key-down drains WM_CHAR: keypad `60..6F` discards only this window's characters; other keys loop virtual +138 across the thread's windows. Return zero. | [V] Queue effects and delegation compared. Keypad peeks now report heartbeat/handled-input hooks too. |
| WM_PAINT | BeginPaint; only with nonzero DC call controller +04 with a writable copy of rcPaint, then EndPaint with the original PAINTSTRUCT. Return zero. | [V] Rectangle, ordering, null-DC branch, return and a callback that modifies its copy are compared. Hidden dirty-region delivery is checked separately with real Win32. |
| WM_ACTIVATE/palettes | Invalidate whole window without erasing; realize palette only for active/non-minimized; notify +2C, call DefWindowProcA, return zero. Query-new-palette returns 1; changed-palette returns zero and realizes only for another window. | [V] Side effects/order, active/minimized words and returns compared. Palette construction/resource bodies were not changed. |
| Wheel/timer `401` | Notify controller +18/+30, then return DefWindowProcA's result. | [V] Both callback and nonzero default result compared. |
| WM_COPYDATA / `+154` | +154 is thiscall, 4 stack bytes, stores and returns callback. WndProc invokes the cdecl callback with wParam/lParam and returns its answer; no callback means zero. | [V] Set, unset, signed return and constructor reset compared. |
| Screen/power messages | Exact SC_SCREENSAVE/SC_MONITORPOWER comparisons; deny suspend query with `424D5144`; other messages use DefWindowProcA. | [V] Switch-table sweep and exact-command cases compared. |
| `+120`, `3D850` | No arguments. Remove/translate/dispatch one `401`, return 1 or 0. | [V] Original/owned per-call queue comparisons. |
| `+124`, `3D430` | Call virtual +128. Preserve a nonzero result; otherwise return WaitMessage without consuming its wakeup. | Boolean normalization corrected. [V] Non-Boolean virtual result and WaitMessage result compared; real wakeup consumption checked separately. |
| `+128`, `3D450` | Remove any message; `402` calls controller +34 without translate/dispatch. Others translate then dispatch. Return 1 or 0, including consuming WM_QUIT without inventing a quit action. | [V] Exact queue and callback comparisons, including thread-quit messages. |
| `+12C`, `3D4C0` | Short-circuit non-removing peeks over `0..FF`, `109..1FF`, `20B..FFFF`, then remove exact window/message and dispatch. Return 1 or 0. Its name does not imply a merge by timestamp. | Safe queue algorithm preserves range priority. [V] Reversed timestamps, boundaries, other windows and excluded inputs compared. |
| `+130/+134/+138/+13C/+140` | One paint/key/character/mouse/move respectively; translate/dispatch, return 1 or 0. | [V] Each filter, remaining queue and return compared. |
| `+144`, `3D680` | Remove/dispatch one mouse message. A single left/right down also consumes a queued matching up from the same window. Double clicks are not paired. Return 1 or 0. | [V] Pairing, other-window up, intervening move, double-click and remaining queue compared. |
| `+148/+14C/+150` | Discard all key/mouse messages; drain/dispatch all `401` for +150. Return final empty PeekMessage's zero. | [V] All effects, filters and final empty peek compared. |
| Heartbeat / handled input | Every peek beats before the OS; successful removals notify with flags/message/lParam. Wait enters/leaves an idle-answering interval. No event is fabricated for a failed/non-removing peek. | [V] Owned hook notifications checked against every fixture OS peek/wait, including keypad drains. Existing bridge staleness/guard tests cover publisher accounting. |
| Hidden-paint lifetime | Dirty regions and the posted-paint flag belong to one HWND lifetime. A destroyed window cannot consume a permanent entry or leave dirty state for its successor. | The real test found the ninth window lost its dirty union. State now retires at WM_NCDESTROY and successful DestroyWindow, after callbacks. Failed invalidations do not synthesize paint. [V] Twenty-four sequential lifetimes pass, using both WM_CLOSE and explicit shutdown. |

## Standalone results

[V] The optimized deterministic fixture compares 2,840 paired observations
with zero differences. It checks defined return values, complete controller
and Win32 call sequences/arguments, remaining queue, initialized window fields
and out parameters. Every direct method/procedure call probes ESP cleanup and
EBX/ESI/EDI/EBP preservation. A matching record is emitted only after comparing
the complete original and owned observations. Undefined EAX in +4C/+98 is
explicitly normalized; opaque list addresses identify different allocations.

[V] The real Win32 fixture compares 438 paired observations with zero
differences over 24 hidden-window lifetimes. It covers dirty unions, successive
and whole-window paints, dimensions, OS mode entries, pointer override without
warping, posted keyboard/mouse/timer messages, valid COPYDATA payloads and
unsetting, consuming WM_QUIT without destroying the window, both close paths,
WaitMessage waking without consuming its message, and a timer callback that
re-enters +128 to dispatch +402. This is headless Wine behavior, not a game
screen/input acceptance run. The real-window oracle uses Wine staging 11.16;
the deterministic fixture also runs on the production Wine 11.0 build, which
has only the Wayland driver and cannot provide X11 windows.

The exact accepted source and artifact bindings, failed attempts and process
cleanup receipts are recorded in
[the receipt](evidence/jgl-window-contracts/receipt.json). The replay command is
`nix develop -c tools/test-jgl-window`, with `CARGO_TARGET_DIR` and
`CIV3R_JGL_WINDOW_EVIDENCE` pointing under an owned evidence directory. Set
`CIV3R_JGL_WINDOW_PROFILE=release` for the optimized checks. For
`owned_window_live`, also set `CIV3R_JGL_WINDOW_WINE` to the X11-capable Wine
path recorded in the receipt. The runner creates its own prefix and Xvfb,
suppresses installer/debugger prompts, snapshots source and binary hashes,
and stops only those owned processes. Test-process deadlines are confined to
this standalone runner; no production watchdog was added.

## Limits and integration gates

Original undefined behavior is not reproduced as a promised answer. Exclusions
are uninitialized bytes/return registers, failed GetCursorPos without initialized
coordinates, null/invalid caller pointers, negative mode indices indexing before
the original array, and +12C's `FFFFFFFF` message-time tie against uninitialized
peek buffers. Valid lifetime and pointer preconditions apply to every raw ABI
call. Object registry/font/image contracts belong to group 6 and are not reopened.
The hidden-paint table still supports eight concurrent hidden windows; the
lifetime regression test uses one live test window at a time. Retirement is
on the window's owner thread after its final invalidations, consistent with
the single game-thread drawing contract. Neither a multi-thread invalidator
shutdown race nor more than eight simultaneously painted hidden windows is
claimed by this check.

The loader, launch lock, quit protocol, default selection policy and original
pass-through have not been changed. `CIV3R_JGL=own` remains explicit opt-in.
The standalone comparisons intentionally load original jgl as the oracle;
their module lists are not no-jgl acceptance evidence.

Parent integration must still run the actual game matrix: launch lock, confirmed
quit, windowed mode, Alt and capture/focus changes, stale input, save load, all
screen families, a turn change/combat and the 30-minute soak. The group-6 task
owns that game's DLLs, prefix, settings and input lease. No live game or screenshot
inspection was substituted for these pending gates here.

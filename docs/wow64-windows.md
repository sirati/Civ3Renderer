# Windows target: Wine's WoW64 ported to Windows

The game runs as 32-bit code in Wine's WoW64 layer, ported to x64
Windows ([sirati/WineWoW64onWindows], `/home/sirati/dev/c/WineWoW64onWindows`).
The renderer runs as native x64 Windows code in the same process.

The port runs 32-bit guest code only. Windows already is 64-bit, so all
64-bit code is plain native Windows code. That includes the port's host and
our renderer. There is no 64-bit Wine layer.

## One design on every host

The 32-bit side is the same as on Linux and macOS: civ3-bridge (`IFC23.dll`)
and the builtin `civ3r_native.dll`. It loads the unixlib with
`wine_unixlib::pe::UnixLib::load` (`NtQueryVirtualMemory(MemoryWineUnixFuncs)`
and `__wine_unix_call_dispatcher`). Nothing in it knows the host.

| Host | 32-bit builtin | Unixlib (host library) |
|---|---|---|
| Linux | `i386-windows/civ3r_native.dll` | `x86_64-unix/civ3r_native.so` (ELF) |
| macOS | `i386-windows/civ3r_native.dll` | `aarch64-unix/civ3r_native.so` (Mach-O) |
| Windows (port) | `i386-windows/civ3r_native.dll` | `x86_64-native/civ3r_native.dll` (x64 PE) |

All paths are under `WINEDLLPATH`. On Windows the unixlib is a native DLL, so
it is named `.dll`. It lives in a directory of its own because its name
matches the 32-bit builtin.

The only Windows-specific code on our side:

- `wine-unixlib`'s `unix` module on `windows`/`x86_64`. Caller memory goes
  through `ReadProcessMemory`/`WriteProcessMemory`, so a bad address fails
  instead of faulting. There are no signals or cancellation to hold back.
- `civ3r-native` builds its native side for every non-x86 target. On Windows
  the viewer gets a thread of its own with its own window and message loop,
  as on Linux, and game paths are the host's paths.

## The port's part

These were requested from the port's session on 2026-10-28. The port
adopts Wine's existing code for each one.

- **A.** `MemoryWineUnixFuncs` for any builtin: find
  `x86_64-native/NAME.dll` the way Wine's loader finds `NAME.so`, then use
  its `__wine_unix_call_wow64_funcs`.
- **B.** `__wine_unix_call` dispatches `table[code]` without the port's
  own bridge-table bound.
- **C.** Builtins are found through `WINEDLLPATH/i386-windows`.
- **D.** Native threads run beside the guest's, and calls may block (input
  waits).
- **E.** Civ3's DLLs, plus Wine's own win32u (windows, message loop,
  timers, cursor) with Wine's null display driver. The game believes it has
  a window, but nothing is shown. Our renderer's native window shows the
  picture, and it has nothing to do with Wine.
- **F.** The process exits cleanly while a native window thread is live.

## Status

On 2026-10-29 the port (candidate fa562da) ran Civ III with civ3-renderer
7ec7322 in a Windows 11 VM with only WARP ("Microsoft Basic Render
Driver"). The renderer's native window showed the title and main menu.
Closing it ended the game with exit code 0.

Set up as for Linux:
- `CIV3R_VIEWER=native` and `CIV3R_GAME_RENDER=off`;
- `CIV3R_FONT_CACHE` pointing at the font sheets;
- `WGPU_BACKEND=dx12` and `TILE_RENDER_ADAPTER=basic render` for WARP;
- the run inside the interactive desktop session.

Windows binds an import to any loaded module with the same base name. So
the port also accepts Wine's `<module>.so` name for native unixlibs, which
keeps Wine's x64 companions from shadowing system DLLs.

## Multiplayer: DirectPlay in every install

Civ III hosts, finds and joins network games through DirectPlay 8
(`CoCreateInstance(CLSID_DirectPlay8Peer)`). Wine's builtin dpnet stubs
`IDirectPlay8Peer::Host` (E_FAIL), so a hosted game gets no player row, no
Ready box and no buttons (the laptop's VM, 2026-11-23). As on Linux
(`tools/install-directplay`), every WoW64 install therefore carries
Microsoft's DirectPlay, the flake's `directplay` package: dplaysvr.exe,
dplayx.dll, dpmodemx.dll, dpnet.dll, dpnhpast.dll, dpnhupnp.dll,
dpnsvr.exe, dpwsockx.dll (x86). Windows' own 32-bit DirectPlay is not
used.

The port (145b812 on) loads native DLLs from `runtime\native-wine\i386-windows`
with Wine's load order (its README):
- the files go into `runtime\native-wine\i386-windows`;
- the launchers set
  `WINEDLLOVERRIDES=civ3r_native=b;dplaysvr.exe,dplayx,dpmodemx,dpnet,dpnhpast,dpnhupnp,dpnsvr.exe,dpwsockx=n,b`;
- the install registers dplayx, dpnet, dpnhpast and dpnhupnp with the
  packaged `runtime\stock-wine\regsvr32.exe` through `wine-host.exe`
  (`WINEDLLPATH=runtime\native-wine`, the same overrides), but only where
  the DirectPlay 8 classes and service providers are not registered yet in
  the 32-bit view (Windows without its DirectPlay feature): those writes
  are machine-wide, and Windows' own registration is never replaced;
- the install checks the result with a 32-bit DirectPlay 8 host test
  (hosting a TCP/IP session on port 23456): "RESULT host ok".

## Fixtures

`tools/wine-unixlib-fixtures OUT` builds two pairs, each a builtin DLL,
its unixlib and a 32-bit caller. `OUT` is the Linux layout and
`OUT/windows` the port's. Both pass on Linux Wine 11 new-WoW64.

- **`crates/wine-unixlib-echo`** covers the unixlib lookup and 20 calls.
  `wine-unixlib-echo-app.exe` prints `ECHO_OK calls=20` when the load, every
  call and a bad-address call all behave.
- **`crates/wine-unixlib-window`** covers our native threads beside the
  guest's. A winit/wgpu window draws frames on a native thread.
  `wine-unixlib-window-app.exe` runs in one of three modes:
  - `wait`: four guest threads block in calls until 120 more frames are
    drawn, then the window closes. Prints `WINDOW_OK ...`, exit 0.
  - `exit-live`: `ExitProcess(37)` while the window still draws.
  - `worker-exit-live`: a guest worker calls `ExitProcess(38)` while the
    main thread is blocked in a call.

[sirati/WineWoW64onWindows]: https://github.com/sirati/WineWoW64onWindows

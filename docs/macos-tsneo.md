# Run on TS Neo

The TS Neo task installation is `/Users/sirati/civ3r-tsneo-20261019`.
Its native Rust launcher and viewer run as Apple ARM64 processes using Metal.
The Wine loader and server also run natively as ARM64. FEX translates the
Windows game's x86 instructions and its x86 compatibility DLLs; the host
Wine processes do not run through Rosetta. Wine and translation support are
runtime dependencies; the renderer implementation
and production launcher remain Rust.

The original game installation is under `game-transfer/extracted/Civilization III Complete`.
The launcher links those assets into its private game directory and keeps
settings, new saves, load scratch files and game histories there. It does
not make another full game copy each launch. Generated glyph sheets under
`runtime/fonts` are required data for text rendering.

## Launch from Spotlight or Finder

`~/Applications/Civilization III.app` starts the same configuration as the
Terminal command below, in a window. Press Cmd-Space, type
"Civilization III" and press Return, or open it in Finder. From a shell:

```sh
open "$HOME/Applications/Civilization III.app"
```

Closing the viewer window ends the game. To stop it from a shell, run
`"$HOME/Applications/Civilization III.app/Contents/MacOS/civ3-app-macos" stop`.
This sends SIGTERM to the launcher, which stops the viewer and game it owns.
Use the executable inside the installed app bundle for `stop`. The raw
`app-entry/civ3-app-macos` and release copies do not have the adjacent
`Contents/Resources/civ3r-app.conf` required by this command. Raw release
copies are suitable for the `bundle` command shown below.
Each launch writes the launcher's own output to `runtime/logs/app-*.log`,
next to the usual session logs.

The bundle executable is `civ3-app-macos`, a small Rust entry built from
`crates/civ3r-launcher/src/bin/civ3-app-macos.rs`. A Finder launch has no
terminal, a minimal environment and cwd `/`, so the entry reads
`Contents/Resources/civ3r-app.conf`, sets up the environment, logging and
working directory, and then replaces itself with `runtime/bin/civ3-play-macos`.
The bundle sets `LSUIElement`, so only the viewer shows a Dock icon.

Build the launcher, viewer, app entry and both native compatibility libraries
on papa from one source snapshot, then deploy the matching set to TS Neo.
Do not run Cargo, rustc or Nix builds on TS Neo, including test builds.
The release helper `/mnt/datasets/c3r-build/release-1019/deploy-cross.sh`
cross-compiles the binaries on papa and installs them with a manifest,
backups and the app entry's `bundle` command.

After moving an installation, regenerate the bundle on the Mac using the
already cross-built app entry. This writes its configuration, converts the
original game icon with `sips` and `iconutil`, signs ad hoc, clears quarantine
attributes and registers with LaunchServices. Use the release directory for
the installed snapshot:

```sh
c3r_root=/Users/sirati/civ3r-tsneo-20261019
c3r_release="$c3r_root/runtime/releases/<installed-source-sha>"
"$c3r_release/civ3-app-macos" bundle --root "$c3r_root" \
  --icon "$c3r_release/icon.png"
```

Add `--fullscreen` to omit `--window`, or `--out DIR` to write somewhere
other than `~/Applications`.

The first launch after a new `IFC23.dll` can take several minutes before the
viewer window appears, while FEX fills its code cache under the Wine prefix.
Later launches start in seconds.

## Launch from Terminal

Run from Terminal in the logged-in macOS desktop. This command was verified
starting Civ III with native ARM64 Wine/FEX and the Apple A18 Pro Metal
renderer on TS Neo.

```sh
c3r_root=/Users/sirati/civ3r-tsneo-20261019
unset DYLD_LIBRARY_PATH VK_DRIVER_FILES VK_ICD_FILENAMES TILE_VIEWER_SOFTWARE_PRESENT
CIV3R_HOME="$c3r_root/runtime" \
CIV3R_VIEWER_BIN="$c3r_root/runtime/bin/civ3r-viewer" \
CIV3R_BRIDGE="$c3r_root/runtime/bin/IFC23.dll" \
CIV3R_WINE="$c3r_root/native-wine/runtime/dist/bin/wine" \
WINEPREFIX="$c3r_root/native-wine/prefix" \
DYLD_FALLBACK_LIBRARY_PATH="$c3r_root/native-wine/runtime/dist/ext/lib" \
FEX_DISKCACHE=1 WINEARCH=win64 WINEDLLOVERRIDES=mscoree,mshtml= \
WGPU_BACKEND=metal TILE_RENDER_ADAPTER=apple \
"$c3r_root/runtime/bin/civ3-play-macos" \
  --game "$c3r_root/game-transfer/extracted/Civilization III Complete/Conquests" \
  --window
```

This selects native ARM64 Wine and Metal on the Apple GPU. It clears the
Intel VM's SwiftShader/Vulkan and software-presentation overrides. The
older x86_64 Wine 11/Rosetta installation under `wine-runtime` is separate
and is not used by this command.

The launcher starts the game with `FEX_X87REDUCEDPRECISION=1` unless that
variable is set. The music's MP3 decoder is x87 code. With FEX's default
80-bit software x87, the sound thread needs a whole core, falls behind, and
the music repeats a short piece. Windows runs x87 at 53-bit precision anyway.

It also sets `HADRON_HARDWARE_TSO=1` unless that variable is set: the game's
threads get Apple silicon's hardware x86 memory ordering instead of FEX's
partial software emulation. Set it to `0` to compare.

The default replaces original map, UI and window pixel output. The game
continues the semantic readbacks used for hit testing and simulation.
Closing the viewer stops the game owned by its launcher. Omit `--window`
for fullscreen.

The title and game setup screens use the full window. Saved split views and
zoom areas apply after entering a game. The Conquests menu stays visible
while its optional world preview loads asynchronously.

Preferences keep edits in the dialog until Accept applies and saves them.
Cancel asks before discarding changes. With "Scale menus" enabled, the menu
size control uses 25-100% of the available screen height, capped to fit its
width. The title menu scales the same way: at 100% it reaches the top and
bottom of the window. With it disabled, menus use their normal size. The
title and Preferences screens always stay inside the visible area. Downsampled UI
background textures use spatial dithering; text, controls and icons keep
their normal sampling. The native application uses the original Windows
game icon.

Append `--compare` to render the original game in a separate visible window
for comparison. `--compatibility-fallback` explicitly permits recovery to
the original renderer after viewer loss. `--show-game-window` changes only
window visibility; it does not enable original pixels. `--map` is the
explicit original-map-only diagnostic option.

## Renderer inside the game's process

The launcher runs the renderer inside the game's Wine process, as the Linux
native mode does. Wine loads the builtin `civ3r_native.dll` (i686) from
`runtime/native/i386-windows` and its unixlib, an ARM64 Mach-O dylib, from
`runtime/native/aarch64-unix/civ3r_native.so`. The 32-bit game calls it
through new WoW64 under FEX. Wine's Mac driver runs `NSApplication` on the
process's main thread; the game's threads are secondary Wine threads. The
viewer window attaches to Wine's application on the main thread (winit's
attached mode, `vendor/winit`), and frames and window events run there.
Scenes, art and input pass through in-process channels. No viewer process
or socket is involved. `CIV3R_NATIVE_DIR` selects another layout directory.

`--separate-viewer` starts the former separate `civ3r-viewer` process for
comparison. `IFC23.dll`, `civ3r_native.dll`, `civ3r_native.so` and
`civ3r-viewer` must come from the same build: they share the scene protocol.

## Runtime files

| Item | Task-relative location |
| --- | --- |
| Native Rust launcher | `runtime/bin/civ3-play-macos` |
| Native Rust viewer (`--separate-viewer` only) | `runtime/bin/civ3r-viewer` |
| In-process renderer | `runtime/native/i386-windows/civ3r_native.dll`, `runtime/native/aarch64-unix/civ3r_native.so` |
| Windows bridge | `runtime/bin/IFC23.dll` |
| Native ARM64 Wine executable | `native-wine/runtime/dist/bin/wine` |
| Private Wine prefix | `native-wine/prefix` |
| Generated glyph data | `runtime/fonts` |
| Session logs | `runtime/logs` |
| Private game and new saves | `runtime/game/Civilization III Complete/Conquests` |

The Intel macOS VM uses SwiftShader Vulkan and bitmap presentation because
it has no Metal device. Its launch environment is separate from this
physical ARM64/Metal configuration; see [the VM guide](macos-vm.md).

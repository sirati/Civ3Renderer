# Steam multiplayer through the native Steam client (design)

Status: design only. Nothing here is implemented. No Steam build of
Civ III is on papa, so facts about it come from public sources and are
marked as such. Before stage 1 starts, the first step is to look at the real
files.

Goal: the Steam edition of Civ III Complete (app 3910) plays online through
the Steam client the user already runs. That is the Linux Steam client on
Linux and the Mac Steam client on macOS, not a Windows Steam inside Wine.

## What the Steam build calls

- On 2015-03-16, 2K replaced GameSpy in the Steam build with Steamworks
  ([Steam news](https://store.steampowered.com/news/app/3910/view/2877193350100468102),
  [PC Gamer](https://www.pcgamer.com/civilization-iii-now-has-steamworks-multiplayer-support/)).
  The patch notes say LAN and Direct IP are gone from that build. Retail and
  GOG builds were not changed. They use DirectPlay 8 for LAN and Direct IP;
  see [the Neo notes](macos-tsneo.md#multiplayer-needs-native-directplay).
- A third-party reverse-engineering note
  ([sp00nznet/civ3 RECON.md](https://github.com/sp00nznet/civ3/blob/HEAD/docs/RECON.md))
  reports the following for the Steam `Civ3Conquests.exe` (VC6, linked 2015-03-19):
  - 13 imports from `steam_api.dll`: init, callbacks and lobbies;
  - 20 imports from `WSOCK32`;
  - `DPLAYX.dll` loaded by name;
  - `steam_appid.txt` = 3910 next to the game.
- Players create a Steam lobby and friends join it
  ([Steam forum, 2021](https://steamcommunity.com/app/3910/discussions/0/3130541122097179341)).
- Inferred, not verified: a March 2015 `steam_api.dll` is Steamworks SDK
  1.32 or 1.33. That would request roughly `SteamClient017`,
  `SteamUser017`/`018`, `SteamFriends014`/`015`, `SteamMatchMaking009` and
  `SteamNetworking005`.

Open question that decides most of the work: does game traffic go over
Steam P2P (`ISteamNetworking`), or does Steam only do lobbies and the game
then talks DirectPlay 8 or Winsock to the address it learns? The retail build
used GameSpy the second way: GameSpy found the host, and DirectPlay 8 over
TCP/IP carried the game. The WSOCK32 imports and the DirectPlay CLSIDs make
the second way likely. In that case the native DirectPlay install this repo
now does is needed for Steam games too. To settle it, run the
Steam build with Wine's builtin dpnet and `WINEDEBUG=+dpnet` (see the
evidence trail in the DirectPlay commit). Then check `strings` of the Steam
`steam_api.dll` for `SteamNetworking0`/`SteamMatchMaking0`.

## How Proton does it: lsteamclient

Source: `ValveSoftware/Proton`, `lsteamclient/` (tag `proton_10.0`).

- It is a Wine builtin PE DLL (`lsteamclient.dll`) with a unixlib
  (`lsteamclient.so`). The PE side calls the Unix side through
  `__wine_unix_call`, which is exactly our dual-stack pattern.
- `gen_wrapper.py` parses about 100 bundled Steamworks SDK header sets
  (0.99u to 1.65) with libclang. For every interface version it generates:
  - a Unix-side C++ file (`cppISteamMatchmaking_SteamMatchMaking009.cpp`, ...)
    that calls the native object's vtable;
  - a PE-side `__thiscall` vtable (`winISteam*.c`);
  - the dispatch table.

  Hand-written `*_manual.*` files cover methods that need care: networking
  messages, server-list callbacks and input.
- ABI: the generator models four layouts (`u32`, `u64`, `w32`, `w64`) and
  emits a struct conversion only where the Windows (MSVC) and Unix (GCC or
  Clang) layouts differ. Callback structs are packed per platform, and
  `callback_len_utow` converts their sizes.
- WoW64: every Unix function takes a `wow64` flag. A 32-bit game uses the
  same 64-bit `lsteamclient.so`, and pointers it hands back are narrowed
  through a scratch buffer below 4 GiB. Only the PE side is built for i386.
- Loading the native client: on Linux it `dlopen`s
  `~/.steam/sdk64/steamclient.so` (the Unix side's architecture, so `sdk64`
  under WoW64). Under `#ifdef __APPLE__` it opens
  `$STEAM_COMPAT_CLIENT_INSTALL_PATH/steamclient.dylib`, else
  `steamclient.dylib` from the dyld search path. Then it resolves
  `CreateInterface`, `Steam_BGetCallback`, `Steam_FreeLastCallback`,
  `Steam_GetAPICallResult` and a few more.
- How the game's `steam_api.dll` finds it: `steam_api` loads the client named
  in `HKCU\Software\Valve\Steam\ActiveProcess\SteamClientDll`. Proton's
  `wine.inf` points that at `C:\Program Files (x86)\Steam\steamclient.dll`,
  and copies Valve's real Windows `steamclient.dll` there. Proton's patched
  `ntdll` loader then redirects every load of `steamclient`/`steamclient64`
  to `lsteamclient.dll`. Its `steam.exe` helper sets `ActiveProcess\pid`.

## Mapping onto our dual stack

Our `wine-unixlib` crate already has the pieces lsteamclient needs:
- a builtin i386 PE DLL with a native unixlib;
- argument structs passing `Buffer32` instead of pointers;
- reads of the 32-bit caller's memory in the low 4 GiB.

`civ3r_native` is built and loaded the same way: `WINEDLLPATH`, the
`i386-windows` and `<arch>-unix` layout, and the builtin marker.

The plan is a new builtin `steamclient.dll` (i386) with a unixlib
`steamclient.so`:

1. Name the builtin `steamclient.dll` itself and write
   `ActiveProcess\SteamClientDll` to point at it. Unmodified Wine then needs
   no loader hook and no copy of Valve's Windows DLL.
2. The PE side exports what `steam_api.dll` resolves from a client:
   `CreateInterface`, `Steam_BGetCallback`, `Steam_FreeLastCallback`,
   `Steam_GetAPICallResult`, `Steam_ReleaseThreadLocalMemory`,
   `Steam_IsKnownInterface`, `Steam_NotifyMissingInterface`, ... It also
   returns `__thiscall` vtables for the interface versions Civ III asks
   for, and only those.
3. Each method is one unix call. The Unix side holds the native
   `ISteamXxx*`, calls it, and converts structs and callbacks. The
   `wow64`-only case is simpler than Proton's: one i386 PE side and one
   64-bit Unix side.
4. Callbacks: `steam_api` polls `Steam_BGetCallback` from the game's thread.
   That is a plain call returning a converted callback struct, with no
   reverse calls from native code into PE code.
5. The launcher sets `SteamClientDll`, `SteamPath` and `pid` in the prefix and
   `SteamAppId=3910`, and adds the overrides (`steamclient=b`). On Linux it
   also puts `~/.steam/sdk64` in reach.

Two ways to get the thunks:
- (a) Build Proton's lsteamclient sources against our layout: its generated
  thunks plus its `__wine_unix_call` dispatch.
- (b) Generate Rust thunks only for the handful of interface versions Civ III
  uses, from the same SDK header descriptions.

(b) fits the repo, which keeps `unsafe` in `wine-unixlib`, and is small if
Civ III needs about 5 interfaces at one version each. (a) gets every version
for free but brings a C++ build and the licence question below.

## macOS specifics

- Mac Steam has been a Universal (arm64 + x86_64) app since the June 2025
  beta ([9to5mac](https://9to5mac.com/2025/06/12/steam-finally-goes-native-on-apple-silicon-heres-how-to-try-it/)).
  `steamclient.dylib` has no i386 slice. Check its arm64 slice on Neo with
  `lipo -info`.
- Our Unix side on Neo is an ARM64 Mach-O unixlib loaded by native ARM64
  Wine, the same as `civ3r_native.so`. It can `dlopen` the arm64 slice of
  `steamclient.dylib` directly, with no Rosetta. The 32-bit PE side runs
  under FEX like the game, and the thunk from 32-bit PE to 64-bit native code
  is our existing new-WoW64 unix call. This is the same arrangement as the
  renderer, which works on Neo today.
- An x86_64 Wine under Rosetta (the older `wine-runtime` install) would need
  the x86_64 slice instead. Do not mix the two: one Wine, one slice.
- Reported problems with Proton's code on macOS:
  - the Whisky port found that two symbols Proton expects are missing from
    the Mac `steamclient.dylib` and had to make them optional
    ([frankea/Whisky#228](https://github.com/frankea/Whisky/issues/228));
  - Mac Steam may answer libsteam_api's client-discovery IPC only for
    processes it launched itself
    ([AArchX#11](https://github.com/mont127/AArchX/pull/11)).

  The second one is the biggest macOS risk. The launcher may have to be
  started by Steam (as a non-Steam game or through the app's launch
  options), or the bridge must connect the way a Steam-launched process
  does. Projects that bridge to native Mac Steam:
  [NotProton](https://github.com/NotProtonNot/NotProton),
  [steamplay-mac](https://github.com/Kalimai/steamplay-mac),
  [neutron](https://github.com/lixsldotnet/neutron).

  neutron (arm64 Wine + FEX) cannot run 32-bit games. That is a limit of its
  ARM64EC loader. Our new-WoW64 setup already runs the 32-bit game on Neo.
- Linux: Steam's 64-bit client is in beta since 2026-10. Proton's
  `~/.steam/sdk64/steamclient.so` path covers it. Only the 64-bit
  `steamclient.so` is needed.

## Licensing and packaging

- `lsteamclient/LICENSE` is the Steamworks SDK licence, not BSD. It contains
  the SDK headers, which may be used only to develop the licensee's software;
  only `redistributable_bin` files such as `steam_api.dll` may be
  redistributed, and only with that software. Proton's own code is BSD-3.
  Projects that reuse lsteamclient fetch it from Valve at build time and
  keep only patches. A Nix derivation can do that too.
- We ship no Valve binary. The game's own `steam_api.dll` stays in the
  user's Steam install, and the native client is the user's.
- Loading the user's installed `steamclient.so`/`.dylib` at run time is what
  Proton does, and no Valve term forbids it. Interface names, method order
  and struct layouts are needed for the thunks. Writing them from the SDK
  headers makes the thunk code a derivative of the SDK. Keep it in a separate
  crate, licensed to match, and do not publish it beyond what that licence
  allows. Get the user's decision on this before stage 2.

## Staged plan

| Stage | Work | Effort | Main risk |
| --- | --- | --- | --- |
| 0 | Inspect the Steam build: `strings` of `steam_api.dll` and the exe for interface versions; one session with builtin dpnet and `+dpnet` to see whether DirectPlay carries the game after the Steam lobby. | 0.5 day | Needs the Steam files on papa. |
| 1 | Linux spike: a builtin `steamclient.dll` and unixlib that answer `CreateInterface` for `SteamClient0xx`, `SteamUser`, `SteamFriends`, `SteamUtils`, plus `Steam_BGetCallback`; `SteamAPI_Init` succeeds against the native client. | 3-5 days | The `steam_api` to client handshake (pipe and user handles, `ActiveProcess` keys). |
| 2 | Lobbies: `ISteamMatchmaking` (create, join, list, lobby data, chat) and their callbacks and call results; two Linux accounts reach the game's setup screen. | 4-6 days | Callback struct packing; call results with 64-bit handles from a 32-bit caller. |
| 3 | Game traffic: if stage 0 shows DirectPlay, nothing new, because the native DirectPlay install covers it. If `ISteamNetworking` (P2P), thunk its send/read/session calls. | 0-4 days | NAT relay behaviour; packet sizes. |
| 4 | macOS: the same crate built for `aarch64-unix`, `dlopen` of `steamclient.dylib`, launcher keys, and Steam starting the launcher if the IPC restriction applies. | 3-5 days | Mac Steam refusing a process it did not start; missing symbols. |
| 5 | Launcher integration on both platforms, overrides, docs and tests (a fake client library for CI). | 2 days | Steam client updates changing interface versions; mitigated by asking only for the old versions Civ III uses. |

Total is about 3-4 weeks if Steam carries only lobbies, and more for full
P2P. The cheapest useful step is stage 0. Its answer decides whether
stage 3 is empty.

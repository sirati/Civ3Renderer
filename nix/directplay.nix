# Microsoft's 32-bit DirectPlay from the DirectX 9.0c redistributable
# (February 2010), the same files `winetricks directplay` installs. Civ III
# hosts and finds network games through DirectPlay 8; Wine's builtin dpnet
# stubs IDirectPlay8Peer::Host and EnumHosts. tools/install-directplay puts
# these into a prefix.
#
# Microsoft no longer serves this file at its original address; the hash is
# the one winetricks pins, so every mirror must deliver the same bytes.
{ lib, stdenvNoCC, fetchurl, cabextract }:

stdenvNoCC.mkDerivation {
  pname = "directplay-redist";
  version = "9.0c-feb2010";

  src = fetchurl {
    name = "directx_feb2010_redist.exe";
    urls = [
      "https://download.microsoft.com/download/E/E/1/EE17FF74-6C45-4575-9CF4-7FC2597ACD18/directx_feb2010_redist.exe"
      "https://files.holarse-linuxgaming.de/mirrors/microsoft/directx_feb2010_redist.exe"
      "https://web.archive.org/web/20210211045713if_/http://download.microsoft.com/download/E/E/1/EE17FF74-6C45-4575-9CF4-7FC2597ACD18/directx_feb2010_redist.exe"
    ];
    hash = "sha256-9tGR6JqWPXzKNPFp0w9J6rmcHtO7ktpz7ENhfKqh6T8=";
  };

  nativeBuildInputs = [ cabextract ];
  dontUnpack = true;

  # dxnt.cab holds the x86 DLLs; lower-case names, as winetricks writes them.
  installPhase = ''
    runHook preInstall
    cabextract -q -L -F dxnt.cab "$src"
    mkdir -p "$out/i386"
    for f in dplaysvr.exe dplayx.dll dpmodemx.dll dpnet.dll dpnhpast.dll dpnhupnp.dll dpnsvr.exe dpwsockx.dll; do
      cabextract -q -L -d "$out/i386" -F "$f" dxnt.cab
      [ -s "$out/i386/$f" ] || { echo "dxnt.cab has no $f" >&2; exit 1; }
    done
    runHook postInstall
  '';

  meta = {
    description = "Microsoft DirectPlay runtime (x86) from the DirectX 9.0c redistributable";
    license = lib.licenses.unfreeRedistributable;
    platforms = lib.platforms.all;
  };
}

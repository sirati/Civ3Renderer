# Civilization III application icon

`civ3-conquests-icon.png` preserves the pixels of the application icon in
`Civilization III Complete/Conquests/Civ3Conquests.exe`. Its PE resource group
is 125, image resource 1, language 1033. That executable has a single 32×32,
4-bit icon. The PNG preserves its palette colours and one-bit transparency
mask without resampling or redrawing. The original mask is fully opaque.

`icon-provenance.json` records the executable, PNG and decoded RGBA SHA256
hashes. The source executable was only read, and no game files are bundled
here beyond this icon. This game artwork retains its original owners'
copyright and is not covered by the Rust source's MIT/Apache licence.

The native viewer embeds this image, so launching the existing bare binary
also sets its macOS Dock/application icon. It does not depend on an `.app`
bundle, shell wrapper, external icon file or the legacy game window. Other
platforms use the same image as their window icon through `application-icon`.
AppKit calls stay in `application-icon-macos` and use `objc2`.

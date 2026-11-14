# Project boundaries

This is the Civilization III integration. Generic rendering, the viewer/game engine, glass effects, Wine unixlib bindings and in-process hooks have separate repositories.
The objective is to avoid legacy GDI/jgl drawing work, keeping necessary state and ABI contracts. Do not implement a replacement canvas rasterizer, scratch GDI drawing, or CPU reconstruction of canvas pixels for hit testing. Sharing glyph math does not remove duplicated graphics work.
Civ III owns the application shader composition and selects glass. The generic game engine must have no glass shader/package dependency.
VM management belongs to git@github.com:sirati/nix-osx-kvm.git. Do not commit VM disks, writable runtime stores or credentials.
Use SSH for all Git repository access. Commit accepted work to origin; preserve source and unique evidence before cleanup.

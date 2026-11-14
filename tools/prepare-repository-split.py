#!/usr/bin/env python3
"""Extract history-preserving projects and rewrite their Cargo boundaries.

Run with git-filter-repo on PATH. Extraction refuses existing destinations.
The original checkout is never filtered. SSH Git dependencies are pinned by
the --publish step, after each upstream project has been checked and committed.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib

SOURCE = Path(__file__).resolve().parents[1]
SPECS = {
    "tilerenderer": {
        "crates": ["tile-render", "tile-render-shared", "tile-render-shaders", "gpu-text", "render-errors", "handoff", "shader-build", "tile-render-test-spirv", "tile-render-test-shaders"],
        "paths": ["vendor/wgpu-hal", "docs/concurrency.md", "docs/per-view-layers.md"],
        "description": "GPU rendering for layered tile worlds, GPU text and shader compilation support.",
    },
    "pointer-control-rs": {
        "crates": ["pointer-api", "pointer-control", "pointer-fallback", "pointer-macos", "pointer-wayland", "pointer-windows", "pointer-windows-ffi", "pointer-x11", "wl-pointer"],
        "paths": ["vendor/winit"],
        "description": "Physical-pixel pointer capture, confinement and placement across window systems.",
    },
    "wine-unixlib-rs": {
        "crates": ["wine-unixlib", "wine-unixlib-echo", "wine-unixlib-window"],
        "paths": ["tools/mark-wine-builtin", "tools/wine-unixlib-fixtures", "docs/wow64-windows.md"],
        "description": "Rust interfaces to Wine's existing in-process PE/native unixlib mechanism.",
    },
    "inproc-rs": {
        "crates": ["inproc"], "paths": [],
        "description": "Guarded foreign-memory access, function hooks and injected DLL support.",
    },
    "glass-shader-rs": {
        "crates": ["glass", "glass-shared", "glass-shaders", "glass-test-spirv", "glass-test-shaders"], "paths": [],
        "description": "Glass effects: host GPU passes, shared parameters and shader libraries.",
    },
    "tilerenderer-gameengine": {
        "crates": ["application-icon", "application-icon-macos", "function-keys", "function-keys-macos", "marker", "marker-shaders", "viewer-shaders", "viewer-spirv", "render-desc", "scene", "menu", "tile-viewer", "viewer-settings", "viewer-transport", "tile-game"],
        "paths": ["assets/viewer", "docs/generic-crates.md", "docs/concurrency.md"],
        "description": "Generic tile-world viewer and game-engine interfaces, menus, input and scene transport.",
    },
    "civ3renderer": {
        "crates": ["civ3-assets", "civ3-bridge", "civ3-sys", "civ3-ui", "civ3-world", "civ3r-launcher", "civ3r-native", "civ3r-process", "civ3r-viewer", "civ3-shaders", "civ3-spirv", "jgl", "jgl-ffi", "gdi-text"],
        "paths": ["assets/civ3", "docs", "tools", "nix", "README.md"],
        "description": "Civilization III game integration, asset loading, compatibility hooks and launch/deployment tools.",
    },
}
REMOTE = {name: f"git@github.com:sirati/{name}.git" for name in SPECS}
COMMON = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".gitignore", "flake.nix", "flake.lock", "tools/wine-run"]


def run(*args, cwd=None, capture=False):
    return subprocess.run(list(map(str, args)), cwd=cwd, check=True, text=True,
                          stdout=subprocess.PIPE if capture else None).stdout


def quoted(key):
    return key if re.fullmatch(r"[A-Za-z0-9_-]+", key) else json.dumps(key)


def scalar(value):
    if isinstance(value, bool): return "true" if value else "false"
    if isinstance(value, (int, float)): return str(value)
    if isinstance(value, str): return json.dumps(value, ensure_ascii=False)
    if isinstance(value, list): return "[" + ", ".join(scalar(v) for v in value) + "]"
    raise TypeError(type(value))


def toml(value):
    lines = []
    def table(node, path=(), header=True, array=False):
        if path and header:
            lines.append(("[[" if array else "[") + ".".join(map(quoted, path)) + ("]]" if array else "]"))
        for key, item in node.items():
            if isinstance(item, dict) or isinstance(item, list) and item and isinstance(item[0], dict): continue
            lines.append(f"{quoted(key)} = {scalar(item)}")
        lines.append("")
        for key, item in node.items():
            if isinstance(item, dict): table(item, (*path, key))
            elif isinstance(item, list) and item and isinstance(item[0], dict):
                for element in item: table(element, (*path, key), array=True)
    table(value)
    result = "\n".join(lines)
    assert tomllib.loads(result) == value
    return result


def owners():
    return {crate: name for name, spec in SPECS.items() for crate in spec["crates"]}


def rewrite_dependencies(node, manifest, project, destination, published):
    if not isinstance(node, dict): return
    if "path" in node and isinstance(node["path"], str):
        target = (manifest.parent / node["path"]).resolve()
        # Paths such as [lib].path are source files, not package dependencies.
        if (target / "Cargo.toml").is_file() and target.parent.name == "crates":
            crate = target.name
            owner = owners()[crate]
            if owner == project:
                node["path"] = os.path.relpath(destination / owner / "crates" / crate,
                                              destination / project / manifest.parent.relative_to(SOURCE))
            elif published:
                node.pop("path")
                node["git"] = "ssh://git@github.com/sirati/" + owner + ".git"
                node["rev"] = run("git", "rev-parse", "HEAD", cwd=destination / owner, capture=True).strip()
            else:
                node["path"] = os.path.relpath(destination / owner / "crates" / crate,
                                              destination / project / manifest.parent.relative_to(SOURCE))
    for item in node.values():
        if isinstance(item, dict): rewrite_dependencies(item, manifest, project, destination, published)
        elif isinstance(item, list):
            for element in item:
                if isinstance(element, dict): rewrite_dependencies(element, manifest, project, destination, published)


def configure(project, destination, published):
    target = destination / project
    original = tomllib.loads((SOURCE / "Cargo.toml").read_text())
    spec = SPECS[project]
    for crate in spec["crates"]:
        manifest = SOURCE / "crates" / crate / "Cargo.toml"
        data = tomllib.loads(manifest.read_text())
        rewrite_dependencies(data, manifest, project, destination, published)
        (target / "crates" / crate / "Cargo.toml").write_text(toml(data))
    root = copy.deepcopy(original)
    members = [f"crates/{crate}" for crate in spec["crates"] if f"crates/{crate}" in original["workspace"]["members"]]
    root["workspace"]["members"] = members
    root["workspace"]["exclude"] = [f"crates/{crate}" for crate in spec["crates"] if f"crates/{crate}" not in members] + ["vendor"]
    rewrite_dependencies(root["workspace"].get("dependencies", {}), SOURCE / "Cargo.toml", project, destination, published)
    # Keep only inherited dependencies actually used by the extracted packages.
    used = set()
    for crate in spec["crates"]:
        data = tomllib.loads((target / "crates" / crate / "Cargo.toml").read_text())
        def inherit(node):
            if isinstance(node, dict):
                for key, value in node.items():
                    if isinstance(value, dict) and value.get("workspace") is True: used.add(key)
                    inherit(value)
            elif isinstance(node, list):
                for item in node: inherit(item)
        inherit(data)
    root["workspace"]["dependencies"] = {k: v for k, v in root["workspace"]["dependencies"].items() if k in used}
    patches = {}
    for package, owner, path in [("wgpu-hal", "tilerenderer", "vendor/wgpu-hal"), ("winit", "pointer-control-rs", "vendor/winit")]:
        if project in ("wine-unixlib-rs", "inproc-rs"): continue
        if package == "winit" and project in ("tilerenderer", "glass-shader-rs"): continue
        if package == "wgpu-hal" and project == "pointer-control-rs": continue
        if owner == project: patches[package] = {"path": path}
        elif published:
            patches[package] = {"git": "ssh://git@github.com/sirati/" + owner + ".git",
                                "rev": run("git", "rev-parse", "HEAD", cwd=destination / owner, capture=True).strip()}
        else: patches[package] = {"path": os.path.relpath(destination / owner / path, target)}
    root["patch"] = {"crates-io": patches}
    (target / "Cargo.toml").write_text(toml(root))
    config = target / ".cargo/config.toml"
    config.parent.mkdir(exist_ok=True)
    config.write_text("[net]\ngit-fetch-with-cli = true\n")
    if project != "civ3renderer":
        flake = (SOURCE / "flake.nix").read_text()
        start = flake.index("      # Microsoft's DirectPlay")
        end = flake.index("      devShells =", start)
        flake = flake[:start] + flake[end:]
        flake = re.sub(r'description = ".*?";', 'description = ' + json.dumps(spec["description"]) + ';', flake, count=1)
        flake = re.sub(r"          export TEST_FONT=.*", "          export TEST_FONT=''${TEST_FONT:-${pkgs.dejavu_fonts}/share/fonts/truetype/DejaVuSans.ttf}", flake)
        (target / "flake.nix").write_text(flake)
        runner = target / "tools/wine-run"
        text = runner.read_text().replace("CIV3R_WINEPREFIX", "WINE_TEST_PREFIX").replace("CIV3R_WINE", "WINE_TEST_RUNTIME")
        text = text.replace("/mnt/datasets/winvm/wine-dev", f"$HOME/.cache/{project}/wine-tests")
        runner.write_text(text)
    if project == "tilerenderer-gameengine":
        library = target / "crates/tile-viewer/src/lib.rs"
        content = library.read_text()
        if "pub const LOADING_CURSOR_DESCRIPTION:" not in content:
            content += '\n/// Generic busy-cursor art for application render descriptions.\npub const LOADING_CURSOR_DESCRIPTION: &str = include_str!("../../../assets/viewer/loading-cursor.toml");\n'
            library.write_text(content)
        guard = target / "crates/tile-viewer/tests/generic_crates.rs"
        text = guard.read_text()
        allowed = "\n".join(f'    "{crate}",' for crate in sorted(spec["crates"]))
        text = re.sub(r'const GENERIC: &\[&str\] = &\[.*?\n\];', 'const GENERIC: &[&str] = &[\n' + allowed + '\n];', text, flags=re.S)
        # The migration writes dependency subtables as well as inline tables.
        text = text.replace('out.push(name.to_owned());', 'if name == "package" { out.push(value.trim().trim_matches(\'"\').to_owned()); }\n        else { out.push(name.to_owned()); }')
        guard.write_text(text)
    install_renderer_guard(project, destination)
    if project == "tilerenderer-gameengine":
        (target / "AGENTS.md").write_text(
            "# Project boundaries\n\n"
            "This is the generic viewer/game engine. It must not depend on Civ III or glass packages, including build and shader dependencies.\n"
            "Applications supply optional effects through ViewerConfig::shaders. The default shader module contains tile and marker entry points only.\n"
            "The GPU renderer is in ../tilerenderer; platform pointer support is in ../pointer-control-rs.\n"
            "Use SSH for Git access. Preserve file history and commit accepted work to origin.\n"
        )
    elif project == "civ3renderer":
        loading = target / "crates/civ3-ui/src/loading_cursor.rs"
        loading.write_text(loading.read_text().replace('include_str!("../../../assets/viewer/loading-cursor.toml")', 'tile_viewer::LOADING_CURSOR_DESCRIPTION'))
        shots = target / "crates/civ3-bridge/tests/cursor_shots.rs"
        shots.write_text(shots.read_text().replace('include_str!("../../../assets/viewer/loading-cursor.toml")', 'civ3_ui::loading_cursor::description()'))
        (target / "AGENTS.md").write_text(
            "# Project boundaries\n\n"
            "This is the Civilization III integration. Generic rendering, the viewer/game engine, glass effects, Wine unixlib bindings and in-process hooks have separate repositories.\n"
            "The objective is to avoid legacy GDI/jgl drawing work, keeping necessary state and ABI contracts. Do not implement a replacement canvas rasterizer, scratch GDI drawing, or CPU reconstruction of canvas pixels for hit testing. Sharing glyph math does not remove duplicated graphics work.\n"
            "Civ III owns the application shader composition and selects glass. The generic game engine must have no glass shader/package dependency.\n"
            "VM management belongs to git@github.com:sirati/nix-osx-kvm.git. Do not commit VM disks, writable runtime stores or credentials.\n"
            "Use SSH for all Git repository access. Commit accepted work to origin; preserve source and unique evidence before cleanup.\n"
        )
    if project != "civ3renderer":
        readme = f"# {project}\n\n{spec['description']}\n\nExtracted from the Civ3Renderer workspace with file history preserved.\n\nCrates:\n\n" + "".join(f"- `{crate}`\n" for crate in spec["crates"])
        if project == "tilerenderer": readme += "\nThe renderer takes the application shader module as input. Its GPU tests compile a tile-only module. `handoff` and `render-errors` remain reusable support packages here.\n"
        if project == "inproc-rs": readme += "\n`inproc` means in-process. It provides safe Rust interfaces for Windows process memory and hooks; it does not render graphics or implement Wine.\n"
        if project == "wine-unixlib-rs": readme += "\nWine supplies the unixlib mechanism. This library provides Rust caller-memory validation, function-table bindings and native-thread support for it.\n"
        if project == "glass-shader-rs": readme += "\nGlass effects are independent of the viewer. Tests compile their own tile-and-glass shader module; the application links its production module.\n"
        (target / "README.md").write_text(readme)
    else:
        readme = (SOURCE / "README.md").read_text()
        links = "\n".join(f"- `{REMOTE[name]}`" for name in SPECS if name != project)
        (target / "README.md").write_text(
            "# Civ3renderer\n\nCivilization III integration and application-specific rendering configuration.\n"
            "Generic rendering, the viewer/game engine, glass effects and platform libraries live in separate repositories:\n\n"
            + links + "\n\nThis application selects the glass plugin; the generic viewer has no glass dependency.\n"
            "The macOS VM manager is provided by `git@github.com:sirati/nix-osx-kvm.git`.\n\n"
            "The usage and historical implementation notes below predate the repository split.\n\n"
            + readme.split("\n", 1)[1]
        )
        (target / "tools/macos-vm.py").unlink(missing_ok=True)
        # The Unixlib fixture builder belongs to its SDK repository.
        (target / "tools/wine-unixlib-fixtures").unlink(missing_ok=True)
        (target / "tools/prepare-repository-split.py").unlink(missing_ok=True)
        bytecode = target / "tools/__pycache__"
        if bytecode.exists():
            for file in bytecode.glob("*.pyc"): file.unlink()
            if not any(bytecode.iterdir()): bytecode.rmdir()
        ignore = target / ".gitignore"
        if "__pycache__/" not in ignore.read_text(): ignore.write_text(ignore.read_text() + "\n__pycache__/\n")
        flake = "git+ssh://git@github.com/sirati/nix-osx-kvm.git?rev=bc6f76cff637fc7dffa9eb00d3bb1af02d01435a"
        (target / "tools/macos-vm").write_text(
            '#!/usr/bin/env bash\nset -euo pipefail\n'
            'export OSX_KVM_VM_ROOT=${OSX_KVM_VM_ROOT:-${CIV3R_MACVM_ROOT:-/mnt/datasets/macvm}}\n'
            'if command -v osx-kvm-manager >/dev/null 2>&1; then exec osx-kvm-manager "$@"; fi\n'
            f'vm_flake=${{CIV3R_MACVM_FLAKE:-{flake}}}\n'
            'exec nix run "$vm_flake#manager" -- "$@"\n'
        )
        (target / "tools/test-macos-vm").write_text(
            '#!/usr/bin/env bash\nset -euo pipefail\n'
            f'vm_flake=${{CIV3R_MACVM_FLAKE:-{flake}}}\n'
            'exec nix build "$vm_flake#checks.x86_64-linux.manager" --no-link\n'
        )


def install_renderer_guard(project, destination):
    primary = {"tilerenderer-gameengine": "tile-viewer", "civ3renderer": "civ3-bridge"}.get(project)
    if primary is None: return
    package = destination / project / "crates" / primary
    test = (SOURCE / "crates/handoff/tests/no_locks.rs").read_text()
    test = test.replace('relative.ends_with("handoff/tests/no_locks.rs")', 'relative.ends_with("/tests/no_locks.rs")')
    (package / "tests").mkdir(exist_ok=True)
    (package / "tests/no_locks.rs").write_text(test)
    (package / "locks-allowlist.txt").write_text((SOURCE / "crates/handoff/locks-allowlist.txt").read_text())


def extract(destination, projects):
    for project in projects:
        spec = SPECS[project]
        target = destination / project
        if target.exists(): raise SystemExit(f"refusing existing extraction destination {target}")
        absent = [crate for crate in spec["crates"] if not (SOURCE / "crates" / crate / "Cargo.toml").exists()]
        if absent: raise SystemExit(f"shader preparation is not complete: {absent}")
        run("git", "clone", "--quiet", "--no-hardlinks", "--single-branch", "--branch", "main", SOURCE, target)
        paths = COMMON + spec["paths"] + ["crates/" + crate for crate in spec["crates"]]
        args = ["git", "filter-repo", "--force"]
        for path in dict.fromkeys(paths): args.extend(["--path", path])
        run(*args, cwd=target)
        run("git", "remote", "add", "origin", REMOTE[project], cwd=target)
    for project in projects: configure(project, destination, False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--extract", choices=list(SPECS), action="append", help="extract only selected projects")
    parser.add_argument("--publish", choices=list(SPECS), help="pin this project's foreign dependencies to committed SSH origins")
    args = parser.parse_args()
    destination = args.destination.expanduser().resolve()
    if args.publish: configure(args.publish, destination, True)
    else:
        destination.mkdir(parents=True, exist_ok=True)
        extract(destination, args.extract or list(SPECS))


if __name__ == "__main__": main()

//! Finder/Spotlight entry for "Civilization III.app".
//!
//! A bundle launch has no terminal, an almost empty environment and cwd `/`.
//! This binary is the bundle's `CFBundleExecutable`. It reads
//! `Contents/Resources/civ3r-app.conf`, rebuilds the launch environment from
//! it, redirects output to a log file and replaces itself with
//! `civ3-play-macos`, so the launcher keeps the process launched by macOS.
//!
//! Subcommands:
//! - `stop`: send SIGTERM to the launcher holding the launch lock. The
//!   launcher then stops the viewer and the game it owns.
//! - `bundle --root DIR --icon PNG [--out DIR] [--fullscreen] [--separate-viewer]`: create
//!   `Civilization III.app` in `--out` (default `~/Applications`) for the
//!   installation at `--root`, with an icon made from the original game PNG.
#![forbid(unsafe_code)]

#[cfg(unix)]
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match args.first().and_then(|arg| arg.to_str()) {
        Some("stop") => app::stop(),
        Some("bundle") => app::bundle(&args[1..]),
        Some("--help" | "-h" | "help") => {
            println!("{}", app::HELP);
            Ok(())
        }
        // Older LaunchServices versions pass a process serial number.
        _ if args
            .iter()
            .all(|arg| arg.to_string_lossy().starts_with("-psn_")) =>
        {
            app::run()
        }
        Some(other) => Err(format!("unknown command {other}; see --help").into()),
        None => Err("arguments are not valid UTF-8".into()),
    };
    if let Err(error) = result {
        eprintln!("civ3-app-macos: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("civ3-app-macos: this application entry requires macOS");
    std::process::exit(1);
}

#[cfg(unix)]
mod app {
    use std::{
        env,
        ffi::OsString,
        fs::{self, OpenOptions},
        os::unix::{fs::PermissionsExt, process::CommandExt},
        path::{Path, PathBuf},
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    pub const HELP: &str =
        "Usage: civ3-app-macos [stop | bundle --root DIR --icon PNG [--out DIR] [--fullscreen] [--separate-viewer]]
  (no arguments)  Start the game as configured in Contents/Resources/civ3r-app.conf.
  stop            Ask the running launcher to stop the viewer and game.
  bundle          Create \"Civilization III.app\" for the installation at --root.";

    const APP_NAME: &str = "Civilization III";
    const BUNDLE_ID: &str = "org.sirati.civ3r";
    const EXECUTABLE: &str = "civ3-app-macos";
    const CONF: &str = "civ3r-app.conf";
    const GAME: &str = "game-transfer/extracted/Civilization III Complete/Conquests";

    /// Launch description. One `key=value` per line; `#` starts a comment.
    /// Keys: `root`, `exec`, `cwd`, `log_dir`, repeatable `arg`, `env`
    /// (`NAME=VALUE`) and `unset` (`NAME`).
    #[derive(Default)]
    struct Conf {
        root: Option<PathBuf>,
        exec: Option<PathBuf>,
        cwd: Option<PathBuf>,
        log_dir: Option<PathBuf>,
        args: Vec<String>,
        env: Vec<(String, String)>,
        unset: Vec<String>,
    }

    impl Conf {
        fn parse(text: &str) -> Result<Self> {
            let mut conf = Self::default();
            for (number, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let (key, value) = line
                    .split_once('=')
                    .ok_or_else(|| format!("line {}: expected key=value", number + 1))?;
                let value = value.to_owned();
                match key {
                    "root" => conf.root = Some(value.into()),
                    "exec" => conf.exec = Some(value.into()),
                    "cwd" => conf.cwd = Some(value.into()),
                    "log_dir" => conf.log_dir = Some(value.into()),
                    "arg" => conf.args.push(value),
                    "unset" => conf.unset.push(value),
                    "env" => {
                        let (name, value) = value
                            .split_once('=')
                            .ok_or_else(|| format!("line {}: env needs NAME=VALUE", number + 1))?;
                        conf.env.push((name.to_owned(), value.to_owned()));
                    }
                    _ => return Err(format!("line {}: unknown key {key}", number + 1).into()),
                }
            }
            Ok(conf)
        }

        fn load() -> Result<Self> {
            let exe = env::current_exe()?.canonicalize()?;
            let contents = exe
                .parent()
                .and_then(Path::parent)
                .ok_or("executable is not inside an application bundle")?;
            let path = contents.join("Resources").join(CONF);
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            Self::parse(&text)
        }

        fn root(&self) -> Result<&Path> {
            self.root
                .as_deref()
                .ok_or_else(|| "configuration has no root".into())
        }
    }

    fn unix_time() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_secs())
    }

    pub fn run() -> Result<()> {
        let conf = Conf::load()?;
        let root = conf.root()?;
        let exec = conf
            .exec
            .clone()
            .unwrap_or_else(|| root.join("runtime/bin/civ3-play-macos"));
        let log_dir = conf
            .log_dir
            .clone()
            .unwrap_or_else(|| root.join("runtime/logs"));
        fs::create_dir_all(&log_dir)?;
        let log_path = log_dir.join(format!("app-{}-{}.log", unix_time(), std::process::id()));
        let log = OpenOptions::new()
            .append(true)
            .create_new(true)
            .open(&log_path)?;
        let mut command = Command::new(&exec);
        command
            .args(&conf.args)
            .current_dir(conf.cwd.as_deref().unwrap_or(root))
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        for name in &conf.unset {
            command.env_remove(name);
        }
        for (name, value) in &conf.env {
            command.env(name, value);
        }
        // exec only returns on failure.
        let error = command.exec();
        Err(format!("cannot start {}: {error}", exec.display()).into())
    }

    pub fn stop() -> Result<()> {
        let conf = Conf::load()?;
        let home = conf
            .env
            .iter()
            .find(|(name, _)| name == "CIV3R_HOME")
            .map(|(_, value)| PathBuf::from(value))
            .unwrap_or(conf.root()?.join("runtime"));
        // Only a launcher that holds the lock now: a pid left in it by one
        // that is gone may be another process by now.
        let Some(holder) = civ3r_launcher::launch_lock::holder(&home)? else {
            println!("civ3-app-macos: not running");
            return Ok(());
        };
        if holder.pid == 0 {
            return Err(format!("no pid in {}", civ3r_launcher::launch_lock::path(&home).display()).into());
        }
        let pid = holder.pid.to_string();
        let status = Command::new("/bin/kill").args(["-TERM", &pid]).status()?;
        if !status.success() {
            return Err(format!("kill -TERM {pid} failed").into());
        }
        println!("civ3-app-macos: asked launcher {pid} to stop");
        Ok(())
    }

    fn checked(command: &mut Command) -> Result<()> {
        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{command:?} failed with {status}").into())
        }
    }

    fn launch_conf(root: &Path, fullscreen: bool, separate_viewer: bool) -> String {
        let r = root.display();
        let mut text = format!(
            "# Generated by civ3-app-macos bundle. Mirrors docs/macos-tsneo.md.\n\
             root={r}\n\
             exec={r}/runtime/bin/civ3-play-macos\n\
             cwd={r}/runtime\n\
             log_dir={r}/runtime/logs\n\
             unset=DYLD_LIBRARY_PATH\n\
             unset=VK_DRIVER_FILES\n\
             unset=VK_ICD_FILENAMES\n\
             unset=TILE_VIEWER_SOFTWARE_PRESENT\n\
             env=CIV3R_HOME={r}/runtime\n\
             env=CIV3R_VIEWER_BIN={r}/runtime/bin/civ3r-viewer\n\
             env=CIV3R_NATIVE_DIR={r}/runtime/native\n\
             env=CIV3R_BRIDGE={r}/runtime/bin/IFC23.dll\n\
             env=CIV3R_WINE={r}/native-wine/runtime/dist/bin/wine\n\
             env=WINEPREFIX={r}/native-wine/prefix\n\
             env=DYLD_FALLBACK_LIBRARY_PATH={r}/native-wine/runtime/dist/ext/lib\n\
             env=PATH={r}/native-wine/runtime/dist/bin:/usr/bin:/bin:/usr/sbin:/sbin\n\
             env=FEX_DISKCACHE=1\n\
             env=WINEARCH=win64\n\
             env=WINEDLLOVERRIDES=mscoree,mshtml=\n\
             env=WGPU_BACKEND=metal\n\
             env=TILE_RENDER_ADAPTER=apple\n\
             arg=--game\n\
             arg={r}/{GAME}\n"
        );
        if !fullscreen {
            text.push_str("arg=--window\n");
        }
        if separate_viewer {
            text.push_str("arg=--separate-viewer\n");
        }
        text
    }

    fn info_plist() -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>{APP_NAME}</string>
	<key>CFBundleExecutable</key>
	<string>{EXECUTABLE}</string>
	<key>CFBundleIconFile</key>
	<string>civ3</string>
	<key>CFBundleIdentifier</key>
	<string>{BUNDLE_ID}</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>{APP_NAME}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.strategy-games</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>LSUIElement</key>
	<true/>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
"#,
            version = option_env!("CARGO_PKG_VERSION").unwrap_or("0.1.0"),
        )
    }

    /// Build `civ3.icns` from the 32x32 original game icon with sips/iconutil.
    fn make_icns(icon: &Path, resources: &Path, scratch: &Path) -> Result<()> {
        let iconset = scratch.join("civ3.iconset");
        fs::create_dir_all(&iconset)?;
        for size in [16u32, 32, 128, 256, 512] {
            for (scale, suffix) in [(1, ""), (2, "@2x")] {
                let pixels = (size * scale).to_string();
                let out = iconset.join(format!("icon_{size}x{size}{suffix}.png"));
                checked(
                    Command::new("/usr/bin/sips")
                        .args(["-s", "format", "png", "-z", &pixels, &pixels])
                        .arg(icon)
                        .arg("--out")
                        .arg(&out)
                        .stdout(std::process::Stdio::null()),
                )?;
            }
        }
        checked(
            Command::new("/usr/bin/iconutil")
                .args(["-c", "icns"])
                .arg(&iconset)
                .arg("-o")
                .arg(resources.join("civ3.icns")),
        )
    }

    pub fn bundle(args: &[OsString]) -> Result<()> {
        let mut root = None;
        let mut out = None;
        let mut icon = None;
        let mut fullscreen = false;
        let mut separate_viewer = false;
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--fullscreen") => fullscreen = true,
                Some("--separate-viewer") => separate_viewer = true,
                Some(flag @ ("--root" | "--out" | "--icon")) => {
                    let value =
                        PathBuf::from(args.next().ok_or_else(|| format!("{flag} needs a value"))?);
                    match flag {
                        "--root" => root = Some(value),
                        "--out" => out = Some(value),
                        _ => icon = Some(value),
                    }
                }
                _ => return Err(format!("unknown option {}", arg.to_string_lossy()).into()),
            }
        }
        let root = root.ok_or("--root is required")?.canonicalize()?;
        let out = out.unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_default()).join("Applications")
        });
        let icon =
            icon.ok_or("--icon (crates/civ3r-viewer/assets/civ3-conquests-icon.png) is required")?;
        let renderer: &[&str] = if separate_viewer {
            &["runtime/bin/civ3r-viewer"]
        } else {
            &["runtime/native/i386-windows/civ3r_native.dll", "runtime/native/aarch64-unix/civ3r_native.so"]
        };
        for required in ["runtime/bin/civ3-play-macos", "runtime/bin/IFC23.dll"].iter().chain(renderer) {
            if !root.join(required).is_file() {
                return Err(format!("missing {}", root.join(required).display()).into());
            }
        }
        fs::create_dir_all(&out)?;
        let app = out.join(format!("{APP_NAME}.app"));
        let staging = out.join(format!(".{APP_NAME}.app.new-{}", std::process::id()));
        let _ = fs::remove_dir_all(&staging);
        let contents = staging.join("Contents");
        let macos = contents.join("MacOS");
        let resources = contents.join("Resources");
        fs::create_dir_all(&macos)?;
        fs::create_dir_all(&resources)?;
        let result = (|| -> Result<()> {
            let exe = macos.join(EXECUTABLE);
            fs::copy(env::current_exe()?, &exe)?;
            fs::set_permissions(&exe, fs::Permissions::from_mode(0o755))?;
            fs::write(contents.join("Info.plist"), info_plist())?;
            fs::write(contents.join("PkgInfo"), "APPL????")?;
            fs::write(resources.join(CONF), launch_conf(&root, fullscreen, separate_viewer))?;
            make_icns(
                &icon,
                &resources,
                &out.join(format!(".civ3-iconset-{}", std::process::id())),
            )?;
            checked(
                Command::new("/usr/bin/codesign")
                    .args(["--force", "--deep", "-s", "-"])
                    .arg(&staging),
            )?;
            Ok(())
        })();
        let _ = fs::remove_dir_all(out.join(format!(".civ3-iconset-{}", std::process::id())));
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        if app.exists() {
            fs::remove_dir_all(&app)?;
        }
        fs::rename(&staging, &app)?;
        let _ = Command::new("/usr/bin/xattr").arg("-cr").arg(&app).status();
        checked(
            Command::new(
                "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
            )
            .arg("-f")
            .arg(&app),
        )?;
        println!("{}", app.display());
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn generated_conf_round_trips() {
            let conf = Conf::parse(&launch_conf(Path::new("/r"), false, false)).unwrap();
            assert_eq!(conf.root.as_deref(), Some(Path::new("/r")));
            assert_eq!(conf.args, ["--game", &format!("/r/{GAME}"), "--window"]);
            assert!(conf.env.contains(&("CIV3R_NATIVE_DIR".into(), "/r/runtime/native".into())));
            let separate = Conf::parse(&launch_conf(Path::new("/r"), true, true)).unwrap();
            assert_eq!(separate.args, ["--game", &format!("/r/{GAME}"), "--separate-viewer"]);
            assert!(
                conf.env
                    .contains(&("WINEDLLOVERRIDES".into(), "mscoree,mshtml=".into()))
            );
            assert!(conf.unset.iter().any(|name| name == "DYLD_LIBRARY_PATH"));
        }

        #[test]
        fn conf_rejects_unknown_keys() {
            assert!(Conf::parse("bogus=1").is_err());
            assert!(Conf::parse("env=NOVALUE").is_err());
        }
    }
}

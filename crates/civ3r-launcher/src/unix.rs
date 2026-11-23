mod owned_debuggers;
mod wine_compat;

use crate::launch_lock;
use crate::startup::{Progress, Update, stage};
use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Write},
    net::SocketAddr,
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt, symlink},
        process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const START_TIMEOUT: Duration = Duration::from_secs(60);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

const HELP: &str = "Usage: civ3-play-macos --game /path/to/Conquests [options]
  --game PATH          Existing Conquests directory. Assets stay in place.
  --wine PATH          Wine executable, or use CIV3R_WINE.
  --prefix PATH        Private Wine prefix, or use WINEPREFIX.
  --window             Open a window instead of fullscreen.
  --scale N            Override desktop scale, at least 1.
  --map                Enable only the original map pixels for diagnostics.
  --show-game-window   Show the original window; its pixels stay disabled.
  --compare            Draw the original game beside the native viewer.
  --compare-game       Alias for --compare.
  --compatibility-fallback
                       Allow native game recovery after viewer loss.
  --separate-viewer    Draw in a separate native viewer process (the former
                       mode, for comparison) instead of inside the game's
                       process. The default elsewhere than on macOS.
  --in-process         Draw inside the game's process (the default on macOS).
  --remove-lock        Start even if another launcher seems to hold the launch
                       lock (a second copy of the game may then run), and stop
                       a game an earlier launcher left running.
  --help               Show this help.
On macOS the renderer runs inside the game's Wine process: Wine loads the
builtin civ3r_native.dll and its native unixlib from CIV3R_NATIVE_DIR
(default ~/Library/Application Support/civ3r/native, laid out as
i386-windows/civ3r_native.dll and <arch>-unix/civ3r_native.so).
The separate viewer and Windows IFC23.dll are installed in
~/Library/Application Support/civ3r/bin, or selected with
CIV3R_VIEWER_BIN and CIV3R_BRIDGE. Game assets are never installed by this tool.
Network games need Microsoft's DirectPlay: its eight files in
~/Library/Application Support/civ3r/directplay (or CIV3R_DIRECTPLAY_DIR) are
installed into the Wine prefix once, with native DLL overrides.";

/// Wine's directory of native unixlibs for this host (`get_so_dir`).
const UNIX_DIR: &str = if cfg!(target_arch = "aarch64") {
    "aarch64-unix"
} else {
    "x86_64-unix"
};
const NATIVE_DLL: &str = "civ3r_native.dll";
const NATIVE_SO: &str = "civ3r_native.so";

struct Options {
    home: PathBuf,
    viewer: PathBuf,
    /// The builtin DLL and unixlib of the in-process renderer.
    native: PathBuf,
    /// The former separate viewer process instead of the in-process one.
    separate_viewer: bool,
    bridge: PathBuf,
    /// Optional replacement 32-bit `sound.dll` staged when present.
    sound_dll: Option<PathBuf>,
    /// Microsoft's DirectPlay files for network games (`install_directplay`).
    directplay: PathBuf,
    wine: OsString,
    prefix: PathBuf,
    game: PathBuf,
    scale: Option<OsString>,
    font_cache: Option<OsString>,
    game_render: OsString,
    fullscreen: bool,
    map: bool,
    show_game: bool,
    /// Remove the launch lock even if a launcher seems to hold it, and stop
    /// a game an earlier launcher left running (`--remove-lock`).
    remove_lock: bool,
}

fn environment(name: &str) -> Option<OsString> {
    env::var_os(name).filter(|value| !value.is_empty())
}

impl Options {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Self>> {
        let home = environment("CIV3R_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(environment("HOME").unwrap_or_default())
                    .join("Library/Application Support/civ3r")
            });
        let mut result = Self {
            viewer: environment("CIV3R_VIEWER_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("bin/civ3r-viewer")),
            native: environment("CIV3R_NATIVE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("native")),
            // (Linux has its own in-process path, tools/civ3-native.)
            separate_viewer: !cfg!(target_os = "macos"),
            bridge: environment("CIV3R_BRIDGE")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("bin/IFC23.dll")),
            sound_dll: environment("CIV3R_SOUND_DLL")
                .map(PathBuf::from)
                .or_else(|| {
                    let default = home.join("bin/sound.dll");
                    default.is_file().then_some(default)
                }),
            directplay: environment("CIV3R_DIRECTPLAY_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("directplay")),
            wine: environment("CIV3R_WINE")
                .or_else(|| environment("WINE"))
                .unwrap_or_else(|| "wine".into()),
            prefix: environment("WINEPREFIX")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("wine-prefix")),
            game: environment("CIV3R_GAME")
                .map(PathBuf::from)
                .unwrap_or_default(),
            scale: environment("CIV3R_SCALE"),
            font_cache: env::var_os("CIV3R_FONT_CACHE"),
            game_render: environment("CIV3R_GAME_RENDER").unwrap_or_else(|| "off".into()),
            fullscreen: true,
            map: false,
            show_game: false,
            remove_lock: false,
            home,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--help" | "-h" | "help") => {
                    println!("{HELP}");
                    return Ok(None);
                }
                Some("--game" | "--wine" | "--prefix" | "--scale") => {
                    let value = args
                        .next()
                        .ok_or_else(|| format!("{} needs a value", arg.to_string_lossy()))?;
                    match arg.to_str() {
                        Some("--game") => result.game = value.into(),
                        Some("--wine") => result.wine = value,
                        Some("--prefix") => result.prefix = value.into(),
                        Some("--scale") => result.scale = Some(value),
                        _ => unreachable!(),
                    }
                }
                Some("--window") => result.fullscreen = false,
                Some("--map") => result.map = true,
                Some("--show-game-window") => result.show_game = true,
                Some("--compare" | "--compare-game") => result.game_render = "on".into(),
                Some("--compatibility-fallback") => result.game_render = "compat".into(),
                Some("--separate-viewer") => result.separate_viewer = true,
                Some("--in-process") => result.separate_viewer = false,
                Some("--remove-lock") => result.remove_lock = true,
                _ => return Err(format!("unknown option {}", arg.to_string_lossy()).into()),
            }
        }
        match result.game_render.to_str() {
            Some("on" | "compare") => {
                result.map = true;
                result.show_game = true;
            }
            Some("off" | "compat" | "compatibility") => (),
            _ => {
                return Err(
                    "CIV3R_GAME_RENDER must be off, on, compare, compat or compatibility".into(),
                );
            }
        }
        if let Some(scale) = &result.scale {
            if !valid_scale(scale) {
                return Err("--scale must be a number at least 1".into());
            }
        }
        Ok(Some(result))
    }
}

fn valid_scale(value: &OsStr) -> bool {
    let Some(value) = value.to_str() else {
        return false;
    };
    let mut parts = value.split('.');
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    digits(parts.next().unwrap_or_default())
        && parts.next().is_none_or(digits)
        && parts.next().is_none()
        && value
            .parse::<f64>()
            .is_ok_and(|value| value.is_finite() && value >= 1.0)
}

fn executable(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn resolve_command(command: &OsStr) -> Result<PathBuf> {
    let path = Path::new(command);
    if path.components().count() > 1 {
        if executable(path) {
            return Ok(path.canonicalize()?);
        }
    } else if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            let candidate = directory.join(path);
            if executable(&candidate) {
                return Ok(candidate.canonicalize()?);
            }
        }
    }
    Err(format!("no Wine executable at {}", path.display()).into())
}

// Resolve existing parents before creating anything, so a runtime or prefix
// alias cannot cause writes into the original installation.
fn writable_destination(path: &Path) -> Result<PathBuf> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let name = path
                .file_name()
                .ok_or("use a normalized writable directory path")?;
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            Ok(writable_destination(parent)?.join(name))
        }
        Err(error) => Err(error.into()),
    }
}

fn no_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "writable private path must not be a symlink: {}",
            path.display()
        )
        .into()),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn link_if_missing(source: &Path, destination: &Path) -> Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            symlink(source, destination)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

/// A question for the user before the launcher goes on (see `startup`):
/// whether to do `action`. `false` refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Question {
    pub title: String,
    pub text: String,
    pub action: &'static str,
}

/// How the user is asked: a dialog, the startup window or the terminal.
pub(crate) type Ask = Box<dyn Fn(&Question) -> bool + Send>;

/// The question for a launch lock someone seems to hold.
fn busy_question(path: &Path, busy: &launch_lock::Busy) -> Question {
    let who = busy.record().map_or_else(|| "a launcher that is starting".into(), |r| r.describe(SystemTime::now()));
    Question {
        title: "Civilization III is already running".into(),
        text: format!(
            "Another launch holds the lock {} ({who}).\n\nIf no other copy of Civilization III is running (none in the Dock, none in Activity Monitor), remove the lock and start. If one is running, a second copy would use the same saved games and settings: quit it first.",
            path.display()
        ),
        action: "Remove lock and start",
    }
}

/// The question for a game an earlier launcher left running.
fn earlier_game_question(stale: &launch_lock::Record, game: &[civ3r_process::Process]) -> Question {
    Question {
        title: "An earlier game is still running".into(),
        text: format!(
            "The game {} started is still running ({} process{}), but its launcher is gone. Stop that game and start a new one? Anything unsaved in it is lost.",
            stale.describe(SystemTime::now()),
            game.len(),
            if game.len() == 1 { "" } else { "es" }
        ),
        action: "Stop it and start",
    }
}

/// The game's processes (not the shared wineserver or services) of an
/// earlier session, found by its exact session marker.
fn earlier_game(stale: &launch_lock::Record) -> Vec<civ3r_process::Process> {
    let Some(marker) = &stale.session else { return Vec::new() };
    match civ3r_process::carrying_environment(owned_debuggers::MARKER.as_bytes(), marker.as_bytes()) {
        Ok(processes) => processes.into_iter().filter(|p| runs_the_game(p.arguments())).collect(),
        Err(error) => {
            eprintln!("civ3-play-macos: cannot look for the earlier session's game: {error}");
            Vec::new()
        }
    }
}

fn runs_the_game(arguments: &[Vec<u8>]) -> bool {
    arguments.iter().any(|argument| argument.to_ascii_lowercase().windows(17).any(|w| w == b"civ3conquests.exe"))
}

fn stop_processes(processes: &[civ3r_process::Process]) {
    for signal in [civ3r_process::Signal::Terminate, civ3r_process::Signal::Kill] {
        for process in processes {
            let _ = process.signal(signal);
        }
        let deadline = Instant::now() + STOP_TIMEOUT;
        while Instant::now() < deadline && processes.iter().any(|p| !p.exited().unwrap_or(true)) {
            thread::sleep(POLL_INTERVAL);
        }
    }
}

/// Takes the launch lock in `home`. A lock whose launcher is gone is taken
/// over (logged); one a launcher seems to hold, and a game an earlier
/// launcher left running, are the user's to decide (`ask`, or `force`).
fn take_lock(home: &Path, force: bool, ask: &Ask) -> Result<launch_lock::LaunchLock> {
    use launch_lock::Claim;
    let path = launch_lock::path(home);
    let mut removed = false;
    loop {
        match launch_lock::claim(home).map_err(|e| format!("cannot claim {}: {e}", path.display()))? {
            Claim::Owned { lock, stale } => {
                if let Some(stale) = stale {
                    eprintln!(
                        "civ3-play-macos: took over the launch lock {}: {} ended without letting it go and no longer runs",
                        path.display(),
                        stale.describe(SystemTime::now())
                    );
                    let game = earlier_game(&stale);
                    if !game.is_empty() {
                        if !(force || ask(&earlier_game_question(&stale, &game))) {
                            return Err(format!("the game {} started is still running; quit it first (or start with --remove-lock to stop it)", stale.describe(SystemTime::now())).into());
                        }
                        eprintln!("civ3-play-macos: stopping the earlier session's game ({} processes)", game.len());
                        stop_processes(&game);
                    }
                }
                return Ok(lock);
            }
            Claim::Busy(busy) => {
                let who = busy.record().map_or_else(|| "a launcher that is starting".into(), |r| r.describe(SystemTime::now()));
                // Removed once already: another launcher took it since.
                if removed || !(force || ask(&busy_question(&path, &busy))) {
                    return Err(format!(
                        "another launch owns {} ({who}); if no other copy of the game is running, start with --remove-lock",
                        path.display()
                    )
                    .into());
                }
                eprintln!("civ3-play-macos: removing the launch lock {} of {who}, as the user asked", path.display());
                launch_lock::remove(home).map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
                removed = true;
            }
        }
    }
}

/// Turns on again the system shortcuts (F1-F12 on macOS) a game left paused,
/// when made and when dropped: made before the launch lock is taken, so a
/// start that is refused replays them too, and dropped after the game's
/// process, however it ended.
struct ShortcutRecovery<F: Fn(&Path) -> usize> {
    journal: PathBuf,
    replay: F,
}

impl<F: Fn(&Path) -> usize> ShortcutRecovery<F> {
    fn new(journal: PathBuf, replay: F) -> Self {
        let recovery = Self { journal, replay };
        recovery.run();
        recovery
    }

    fn run(&self) {
        let turned_on = (self.replay)(&self.journal);
        if turned_on > 0 {
            eprintln!(
                "civ3-play-macos: turned on again {turned_on} system shortcut(s) a game left paused ({})",
                self.journal.display()
            );
        }
    }
}

impl<F: Fn(&Path) -> usize> Drop for ShortcutRecovery<F> {
    fn drop(&mut self) {
        self.run();
    }
}

fn random_hex() -> Result<String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("cannot create secure session token: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

struct Session(PathBuf);
impl Session {
    fn create(home: &Path) -> Result<Self> {
        let path = home.join(format!("session.{}", random_hex()?));
        DirBuilder::new().mode(0o700).create(&path)?;
        let session = Self(path);
        let mut token = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(session.0.join("token"))?;
        writeln!(token, "{}", random_hex()?)?;
        Ok(session)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.path("token"));
        let _ = fs::remove_file(self.path("ready"));
        let _ = fs::remove_file(self.path("menu-ready"));
        let _ = fs::remove_file(self.path("viewer-closed"));
        let _ = fs::remove_dir(&self.0);
    }
}

struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn original_dll(game: &Path) -> Result<PathBuf> {
    for name in ["ifc23_orig.dll", "IFC23.dll", "ifc23.dll"] {
        let path = game.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("the original IFC23.dll is missing".into())
}

fn original_sound_dll(game: &Path) -> Option<PathBuf> {
    for name in ["sound_orig.dll", "sound.dll", "SOUND.DLL", "Sound.dll"] {
        let path = game.join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn sound_game_file(name: &OsStr) -> bool {
    name.eq_ignore_ascii_case("sound.dll") || name.eq_ignore_ascii_case("sound_orig.dll")
}

fn scratch_file(name: &OsStr) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("tmp"))
}

fn history_file(name: &OsStr) -> bool {
    name.eq_ignore_ascii_case("CampaignRecord.hof") || name.eq_ignore_ascii_case("HighScores.cv3")
}

/// Upgrade links created by older launchers without following them for writes.
/// Validate every legacy target and read every history seed before mutation.
fn prepare_private_state(private: &Path, game: &Path) -> Result<()> {
    let mut scratch_links = Vec::new();
    match fs::read_dir(private) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                let name = entry.file_name();
                if (scratch_file(&name) || history_file(&name)) && entry.file_type()?.is_symlink() {
                    if fs::read_link(entry.path())? != game.join(&name) {
                        return Err(format!(
                            "untrusted private state symlink: {}",
                            entry.path().display()
                        )
                        .into());
                    }
                    if scratch_file(&name) {
                        scratch_links.push(entry.path());
                    }
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let mut histories = Vec::new();
    for entry in fs::read_dir(game)? {
        let entry = entry?;
        if !history_file(&entry.file_name()) {
            continue;
        }
        let destination = private.join(entry.file_name());
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if !metadata.file_type().is_symlink() => continue,
            Ok(_) => (), // Validated as an owned legacy link above.
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        histories.push((destination, fs::read(entry.path())?));
    }
    fs::create_dir_all(private)?;
    for (destination, bytes) in histories {
        // Rename replaces only the link itself, never its original-game target.
        let temporary_path = private.join(format!(".history.{}", random_hex()?));
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary_path)?;
        let temporary = TemporaryFile(temporary_path);
        output.write_all(&bytes)?;
        drop(output);
        fs::rename(&temporary.0, destination)?;
    }
    for path in scratch_links {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Atomically replaces renderer-owned `name` in the private game with a copy
/// of `source`, even when it was a symlink.
fn install_copy(private: &Path, source: &Path, name: &str) -> Result<()> {
    let temporary_path = private.join(format!("{name}.{}", random_hex()?));
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary_path)?;
    let temporary = TemporaryFile(temporary_path);
    io::copy(&mut File::open(source)?, &mut output)?;
    drop(output);
    fs::rename(&temporary.0, private.join(name))?;
    Ok(())
}

fn private_install(
    home: &Path,
    game: &Path,
    bridge: &Path,
    sound_dll: Option<&Path>,
) -> Result<PathBuf> {
    let private_install = home.join("game/Civilization III Complete");
    let private = private_install.join("Conquests");
    for path in [
        home.join("game"),
        private_install.clone(),
        private.clone(),
        private.join("Saves"),
        private.join("Saves/Auto"),
        private.join("conquests.ini"),
        private.join("Conquests.ini"),
        private.join("civ3-bridge.log"),
        private.join("LSANS.fot"),
    ] {
        no_symlink(&path)?;
    }
    if private == game {
        return Err("supply the original game, not the private install".into());
    }
    let original = original_dll(game)?;
    let orig_sound = sound_dll.and_then(|_| original_sound_dll(game));
    let replace_sound = sound_dll.is_some() && orig_sound.is_some();
    let private_original = private.join("ifc23_orig.dll");
    match fs::symlink_metadata(&private_original) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            if fs::read_link(&private_original)? != original {
                return Err(
                    "private install belongs to another game; use another CIV3R_HOME".into(),
                );
            }
        }
        Ok(_) => return Err("unexpected original DLL in the private install".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    if let Some(orig_sound) = &orig_sound {
        let private_sound_orig = private.join("sound_orig.dll");
        match fs::symlink_metadata(&private_sound_orig) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if fs::read_link(&private_sound_orig)? != *orig_sound {
                    return Err(
                        "private install belongs to another game; use another CIV3R_HOME".into(),
                    );
                }
            }
            Ok(_) => return Err("unexpected original sound DLL in the private install".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    prepare_private_state(&private, game)?;
    fs::create_dir_all(private.join("Saves/Auto"))?;
    let source = game.parent().ok_or("game directory has no parent")?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.path() != game {
            link_if_missing(&entry.path(), &private_install.join(entry.file_name()))?;
        }
    }
    for entry in fs::read_dir(game)? {
        let entry = entry?;
        let name = entry.file_name();
        if !private_game_file(&name) && !(replace_sound && sound_game_file(&name)) {
            link_if_missing(&entry.path(), &private.join(name))?;
        }
    }
    match fs::read_dir(game.join("Saves")) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.file_name() != "Auto" {
                    link_if_missing(
                        &entry.path(),
                        &private.join("Saves").join(entry.file_name()),
                    )?;
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    link_if_missing(&original, &private_original)?;
    install_copy(&private, bridge, "IFC23.dll")?;
    if let (Some(sound_src), Some(orig_sound)) = (sound_dll, &orig_sound) {
        link_if_missing(orig_sound, &private.join("sound_orig.dll"))?;
        install_copy(&private, sound_src, "sound.dll")?;
    }
    let ini = private.join("conquests.ini");
    if !ini.exists() {
        let bytes = if game.join("conquests.ini").is_file() {
            fs::read(game.join("conquests.ini"))?
        } else {
            b"[Conquests]\nKeepRes=1\nPlayIntro=0\n".to_vec()
        };
        let mut output = OpenOptions::new().create_new(true).write(true).open(ini)?;
        output.write_all(&bytes)?;
    }
    Ok(private)
}

// Only files generated or overwritten by a game session are private.
// conquests.biq and conquests.mb are immutable original rules inputs and must
// remain accessible through the ordinary read-only asset links.
fn private_game_file(name: &OsStr) -> bool {
    scratch_file(name)
        || history_file(name)
        || name.to_str().is_some_and(|name| {
            matches!(
                name,
                "IFC23.dll"
                    | "ifc23.dll"
                    | "ifc23_orig.dll"
                    | "civ3r_native.dll"
                    | "conquests.ini"
                    | "Conquests.ini"
                    | "Saves"
                    | "civ3-bridge.log"
                    | "LSANS.fot"
            ) || name.starts_with("civ3-trace")
        })
}

/// The group leader stays unreaped until all group signals have been sent.
/// Its reserved PID prevents an unrelated group from reusing this group ID.
struct ManagedChild(Child, Option<owned_debuggers::OwnedDebuggers>);
impl ManagedChild {
    fn spawn_wine(command: &mut Command) -> Result<Self> {
        let debuggers = owned_debuggers::OwnedDebuggers::new(command)?;
        Ok(Self(command.spawn()?, Some(debuggers)))
    }

    /// The marker every process of this game carries (`CIV3R_SESSION_TOKEN`).
    fn session_marker(&self) -> Option<&str> {
        self.1.as_ref().map(owned_debuggers::OwnedDebuggers::marker)
    }

    fn group(&self) -> rustix::process::Pid {
        rustix::process::Pid::from_raw(self.0.id() as i32)
            .expect("a spawned Unix child has a positive pid")
    }

    fn status(&self) -> io::Result<Option<ExitStatus>> {
        use rustix::process::{WaitId, WaitIdOptions, waitid};
        let status = waitid(
            WaitId::Pid(self.group()),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )?;
        // WNOWAIT observes termination without consuming the owned child.
        Ok(status.map(|status| {
            let raw = status
                .exit_status()
                .map(|code| code << 8)
                .unwrap_or_else(|| status.terminating_signal().unwrap_or(1));
            ExitStatus::from_raw(raw)
        }))
    }
}
impl ManagedChild {
    fn stop_group(&mut self) {
        let group = self.group();
        // No code outside this guard can reap its private Child. If an external
        // SIGCHLD handler nevertheless consumed it, do not signal a stale ID.
        if self.status().is_err_and(|error| {
            error.raw_os_error() == Some(rustix::io::Errno::CHILD.raw_os_error())
        }) {
            return;
        }
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::TERM);
        let deadline = Instant::now() + STOP_TIMEOUT;
        while Instant::now() < deadline {
            if self.status().ok().flatten().is_some() {
                // Let helpers react to TERM before the final bounded cleanup.
                thread::sleep(POLL_INTERVAL);
                break;
            }
            thread::sleep(POLL_INTERVAL);
        }
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        // This is deliberately the only consuming wait in the guard. No group
        // signal is permitted after this releases the leader's reserved PID.
        let _ = self.0.wait();
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        self.stop_group();
        // The original leader may already be reaped here. Detached helpers
        // are signaled only by their independent, identity-stable handles.
        if let Some(debuggers) = &self.1 { debuggers.stop(); }
    }
}

fn output_log(path: &Path) -> Result<(File, File)> {
    // Do not follow a planted log symlink or overwrite a previous session log.
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    Ok((file.try_clone()?, file))
}

/// FEX's switch for 64-bit x87 emulation (ignored where no FEX runs).
const FEX_X87: &str = "FEX_X87REDUCEDPRECISION";

/// Civ III's music is MP3 decoded with x87 code on sound.dll's thread. FEX
/// emulates x87's 80-bit format in software by default, which takes that
/// thread a whole core on Apple silicon; it then falls behind and DirectSound
/// replays its stale ring (the music loops a short piece). Windows runs x87
/// at 53-bit precision anyway, so 64-bit emulation loses nothing. An explicit
/// setting wins.
fn fex_x87_reduced_precision(explicit: Option<OsString>) -> OsString {
    explicit.unwrap_or_else(|| "1".into())
}

/// Hadron's switch for Apple silicon's hardware x86 memory ordering (TSO)
/// on the game process's threads. Its Wine and FEX call
/// `thread_set_x86_64_compat` per thread; without the loader's entitlement
/// that fails and FEX keeps emulating. Ignored where no Hadron runs.
const HARDWARE_TSO: &str = "HADRON_HARDWARE_TSO";

/// The game, Wine's 32-bit ntdll (heap, critical sections, wait queues) and
/// the bridge are x86 code relying on x86 ordering, with several threads.
/// FEX's default emulation orders plain loads and stores only, not vector
/// or memcpy accesses, and costs barriers on every access; hardware TSO is
/// both complete and cheaper. Hadron leaves it opt-in because it also slows
/// native code on those threads (Metal drivers in-process). Our renderer,
/// also in-process, draws on the process's main thread and threads of its
/// own, none of them a game thread. An explicit setting wins.
fn hardware_tso(explicit: Option<OsString>) -> OsString {
    explicit.unwrap_or_else(|| "1".into())
}

fn configure(command: &mut Command, options: &Options, prefix: &Path, settings: &Path) {
    command.process_group(0);
    command
        .env("WINEPREFIX", prefix)
        .env("CIV3R_SETTINGS", settings)
        .env(
            "WGPU_BACKEND",
            environment("WGPU_BACKEND").unwrap_or_else(|| "metal".into()),
        );
    if let Some(scale) = &options.scale {
        command.env("CIV3R_SCALE", scale);
    }
    if let Some(cache) = &options.font_cache {
        command.env("CIV3R_FONT_CACHE", cache);
    }
}

fn initialize_registry(
    wine: &wine_compat::Runtime,
    options: &Options,
    prefix: &Path,
    settings: &Path,
    private: &Path,
    log: &Path,
    signal: &AtomicUsize,
) -> Result<Option<i32>> {
    let base = private
        .parent()
        .ok_or("private game directory has no parent")?;
    let conquests = windows_path(private)?;
    let ptw = windows_path(&base.join("civ3PTW"))?;
    let base = format!("{}\\", windows_path(base)?);
    let values = [
        (
            r"HKLM\Software\Infogrames\Conquests",
            "Install_Path",
            "REG_SZ",
            conquests.as_str(),
        ),
        (
            r"HKLM\Software\Infogrames\Conquests",
            "Complete_Version",
            "REG_DWORD",
            "1",
        ),
        (
            r"HKLM\Software\Infogrames\Civ3PTW",
            "Install_Path",
            "REG_SZ",
            ptw.as_str(),
        ),
        (
            r"HKLM\Software\Infogrames\Civ3PTW",
            "Gold_Version",
            "REG_DWORD",
            "1",
        ),
        (
            r"HKLM\Software\Infogrames Interactive\Civilization III",
            "Install_Path",
            "REG_SZ",
            base.as_str(),
        ),
    ];
    // Every `wine reg.exe` boots the prefix's services and costs seconds, so
    // write only values the saved registry lacks. Wine saves system.reg when
    // its server exits; a stale or unreadable file just means writing again.
    let saved = fs::read_to_string(prefix.join("system.reg")).unwrap_or_default();
    let values: Vec<_> = values
        .into_iter()
        .filter(|(key, name, kind, value)| !registered_32(&saved, key, name, kind, value))
        .collect();
    if values.is_empty() {
        return Ok(None);
    }
    let step = WineStep::new(
        wine,
        options,
        prefix,
        settings,
        private,
        log,
        signal,
        "Wine registry initialization",
    )?;
    for (key, name, kind, value) in values {
        let args = [
            "reg.exe", "add", key, "/v", name, "/t", kind, "/d", value, "/f", "/reg:32",
        ];
        if let Some(status) = step.run(&args, name)? {
            return Ok(Some(status));
        }
    }
    Ok(None)
}

/// One-time prefix work done with short Wine commands before the game
/// starts, all under one deadline, their output in one log.
struct WineStep<'a> {
    wine: &'a wine_compat::Runtime,
    options: &'a Options,
    prefix: &'a Path,
    settings: &'a Path,
    private: &'a Path,
    log: &'a Path,
    output: File,
    signal: &'a AtomicUsize,
    deadline: Instant,
    what: &'static str,
}

impl<'a> WineStep<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        wine: &'a wine_compat::Runtime,
        options: &'a Options,
        prefix: &'a Path,
        settings: &'a Path,
        private: &'a Path,
        log: &'a Path,
        signal: &'a AtomicUsize,
        what: &'static str,
    ) -> Result<Self> {
        Ok(Self {
            wine,
            options,
            prefix,
            settings,
            private,
            log,
            output: output_log(log)?.0,
            signal,
            deadline: Instant::now() + START_TIMEOUT,
            what,
        })
    }

    /// Runs `wine args...`; `Some(signal)` if interrupted, an error if it
    /// fails or the deadline passes.
    fn run(&self, args: &[&str], item: &str) -> Result<Option<i32>> {
        let interrupted = self.signal.load(Ordering::Relaxed);
        if interrupted != 0 {
            return Ok(Some(interrupted as i32));
        }
        let mut command = self.wine.command();
        configure(&mut command, self.options, self.prefix, self.settings);
        command
            .args(args)
            .current_dir(self.private)
            .stdin(Stdio::null())
            .stdout(self.output.try_clone()?)
            .stderr(self.output.try_clone()?);
        let child = ManagedChild::spawn_wine(&mut command)?;
        loop {
            let interrupted = self.signal.load(Ordering::Relaxed);
            if interrupted != 0 {
                return Ok(Some(interrupted as i32));
            }
            if let Some(status) = child.status()? {
                if !status.success() {
                    return Err(format!(
                        "{} failed for {item}; see {}",
                        self.what,
                        self.log.display()
                    )
                    .into());
                }
                return Ok(None);
            }
            if Instant::now() >= self.deadline {
                return Err(
                    format!("{} timed out; see {}", self.what, self.log.display()).into(),
                );
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
}

/// Microsoft's DirectPlay files, as `winetricks directplay` installs them
/// (nix/directplay.nix). Civ III hosts, finds and joins network games through
/// DirectPlay 8; Wine's builtin dpnet stubs `IDirectPlay8Peer::Host` and
/// `EnumHosts`.
const DIRECTPLAY_FILES: [&str; 8] = [
    "dplaysvr.exe",
    "dplayx.dll",
    "dpmodemx.dll",
    "dpnet.dll",
    "dpnhpast.dll",
    "dpnhupnp.dll",
    "dpnsvr.exe",
    "dpwsockx.dll",
];
const DIRECTPLAY_REGISTER: [&str; 4] = ["dplayx.dll", "dpnet.dll", "dpnhpast.dll", "dpnhupnp.dll"];
/// In the prefix: written once the files are in place and registered.
const DIRECTPLAY_MARKER: &str = "civ3r-directplay";

/// `HKCU\Software\Wine\DllOverrides` with every DirectPlay file native.
fn directplay_overrides() -> String {
    let mut reg = String::from("REGEDIT4\r\n\r\n[HKEY_CURRENT_USER\\Software\\Wine\\DllOverrides]\r\n");
    for file in DIRECTPLAY_FILES {
        let name = file.strip_suffix(".dll").unwrap_or(file);
        reg.push_str(&format!("\"{name}\"=\"native\"\r\n"));
    }
    reg
}

/// Whether the prefix's 32-bit system directory already holds exactly the
/// files in `source` and the marker says they were registered.
fn directplay_current(source: &Path, system: &Path, prefix: &Path) -> bool {
    prefix.join(DIRECTPLAY_MARKER).is_file()
        && DIRECTPLAY_FILES.iter().all(|file| {
            matches!(
                (fs::read(source.join(file)), fs::read(system.join(file))),
                (Ok(a), Ok(b)) if a == b
            )
        })
}

/// Installs native DirectPlay into the prefix once (`tools/install-directplay`
/// does the same on Linux): the files from `source` into the 32-bit system
/// directory, native DLL overrides, and the four DLLs registered with the
/// 32-bit regsvr32. Without the files the game still starts; only network
/// games are unavailable.
fn install_directplay(
    wine: &wine_compat::Runtime,
    options: &Options,
    prefix: &Path,
    settings: &Path,
    private: &Path,
    log: &Path,
    signal: &AtomicUsize,
) -> Result<Option<i32>> {
    let source = &options.directplay;
    if !DIRECTPLAY_FILES.iter().all(|file| source.join(file).is_file()) {
        eprintln!(
            "civ3-play-macos: no DirectPlay files in {}; network games will not work",
            source.display()
        );
        return Ok(None);
    }
    let windows = prefix.join("drive_c/windows");
    let (system, regsvr32) = if windows.join("syswow64").is_dir() {
        (windows.join("syswow64"), r"C:\windows\syswow64\regsvr32.exe")
    } else if windows.join("system32").is_dir() {
        (windows.join("system32"), r"C:\windows\system32\regsvr32.exe")
    } else {
        eprintln!(
            "civ3-play-macos: the prefix {} has no system directory yet; DirectPlay is installed at the next start",
            prefix.display()
        );
        return Ok(None);
    };
    if directplay_current(source, &system, prefix) {
        return Ok(None);
    }
    let _ = fs::remove_file(prefix.join(DIRECTPLAY_MARKER));
    for file in DIRECTPLAY_FILES {
        install_copy(&system, &source.join(file), file)?;
    }
    let reg = log.with_extension("reg");
    fs::write(&reg, directplay_overrides())?;
    let reg_windows = windows_path(&reg)?;
    let step = WineStep::new(
        wine,
        options,
        prefix,
        settings,
        private,
        log,
        signal,
        "DirectPlay installation",
    )?;
    let steps = std::iter::once(["regedit.exe", "/S", reg_windows.as_str()])
        .chain(DIRECTPLAY_REGISTER.map(|dll| [regsvr32, "/s", dll]));
    for args in steps {
        let interrupted = step.run(&args, args[2]);
        if !matches!(interrupted, Ok(None)) {
            let _ = fs::remove_file(&reg);
            return interrupted;
        }
    }
    let _ = fs::remove_file(&reg);
    fs::write(prefix.join(DIRECTPLAY_MARKER), "installed\n")?;
    Ok(None)
}

/// Whether a 64-bit prefix's saved `system.reg` already holds this value in
/// the 32-bit registry view that `reg.exe /reg:32` writes. Anything this
/// does not recognise (other hives, escaped characters) counts as missing.
fn registered_32(saved: &str, key: &str, name: &str, kind: &str, value: &str) -> bool {
    let Some(key) = key.strip_prefix(r"HKLM\Software\") else {
        return false;
    };
    let plain = |text: &str| text.chars().all(|c| (' '..='~').contains(&c));
    if !plain(key) || !plain(name) || !plain(value) {
        return false;
    }
    let escape = |text: &str| text.replace('\\', r"\\").replace('"', "\\\"");
    let header = format!("[{}]", escape(&format!(r"Software\Wow6432Node\{key}")));
    let expected = match kind {
        "REG_SZ" => format!("\"{}\"", escape(value)),
        "REG_DWORD" => match value.parse::<u32>() {
            Ok(number) => format!("dword:{number:08x}"),
            Err(_) => return false,
        },
        _ => return false,
    };
    let entry = format!("\"{}\"=", escape(name));
    let mut lines = saved.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.get(header.len()..) else {
            continue;
        };
        if !line[..header.len()].eq_ignore_ascii_case(&header)
            || !(rest.is_empty() || rest.starts_with(' '))
        {
            continue;
        }
        return lines
            .take_while(|line| !line.starts_with('['))
            .filter_map(|line| {
                let head = line.get(..entry.len())?;
                head.eq_ignore_ascii_case(&entry)
                    .then(|| &line[entry.len()..])
            })
            .any(|found| found == expected);
    }
    false
}

fn ready_address(path: &Path) -> Result<Option<String>> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() == 0 => return Ok(None),
        Ok(metadata) if metadata.len() > 128 => {
            return Err("native viewer published an invalid address".into());
        }
        Ok(_) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let value = fs::read_to_string(path)?;
    let value = value.trim();
    let address: SocketAddr = value
        .parse()
        .map_err(|_| "native viewer published an invalid address")?;
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err("native viewer must listen on a nonzero loopback address".into());
    }
    Ok(Some(value.to_owned()))
}

fn windows_path(path: &Path) -> Result<String> {
    let path = path
        .to_str()
        .ok_or("Wine game path must contain valid Unicode")?;
    Ok(format!("Z:{}", path.replace('/', "\\")))
}

/// Run the launcher. All filesystem and process operations use safe Rust APIs.
/// Only renderer-owned files and new game state are written to the private home.
pub fn launch(
    args: impl IntoIterator<Item = OsString>,
    progress: Option<Progress>,
    signal: Arc<AtomicUsize>,
    ask: Ask,
) -> Result<i32> {
    let Some(mut options) = Options::parse(args)? else {
        return Ok(0);
    };
    let settings = environment("CIV3R_SETTINGS")
        .map(PathBuf::from)
        .unwrap_or_else(|| options.home.join("settings.toml"));
    // System shortcuts a game paused to have F1-F12 (macOS) are on again now,
    // before anything can refuse this start (a game that died without
    // turning them on, a launcher AppKit ended), and once this game is gone,
    // however it ended (dropped after its process, declared below).
    let _function_keys = ShortcutRecovery::new(function_keys::journal_path(&settings), function_keys::replay);
    if options.game.as_os_str().is_empty() {
        return Err("supply the existing game with --game or CIV3R_GAME".into());
    }
    if !options.game.join("Civ3Conquests.exe").is_file() {
        return Err(format!("no Civ3Conquests.exe in {}", options.game.display()).into());
    }
    if !options.game.join("conquests.biq").is_file() {
        return Err(format!(
            "no required rules file conquests.biq in {}",
            options.game.display()
        )
        .into());
    }
    let native_dll = options.native.join("i386-windows").join(NATIVE_DLL);
    let native_so = options.native.join(UNIX_DIR).join(NATIVE_SO);
    if options.separate_viewer {
        if !executable(&options.viewer) {
            return Err(format!("no native viewer at {}", options.viewer.display()).into());
        }
        options.viewer = options.viewer.canonicalize()?;
    } else {
        for file in [&native_dll, &native_so] {
            if !file.is_file() {
                return Err(format!(
                    "no in-process renderer at {} (or use --separate-viewer)",
                    file.display()
                )
                .into());
            }
        }
        options.native = options.native.canonicalize()?;
    }
    if !options.bridge.is_file() {
        return Err(format!("no renderer bridge at {}", options.bridge.display()).into());
    }
    options.game = options.game.canonicalize()?;
    options.bridge = options.bridge.canonicalize()?;
    if let Some(sound_dll) = &options.sound_dll {
        if !sound_dll.is_file() {
            return Err(format!("no sound replacement at {}", sound_dll.display()).into());
        }
        options.sound_dll = Some(sound_dll.canonicalize()?);
    }
    let wine = resolve_command(&options.wine)?;
    options.home = writable_destination(&options.home)?;
    // Wine and the native asset worker share the generated Windows glyph data.
    // Native font loading translates Z: paths to the corresponding Unix path.
    if options.font_cache.is_none() {
        options.font_cache = Some(windows_path(&options.home.join("fonts"))?.into());
    }
    options.prefix = writable_destination(&options.prefix)?;
    let source = options
        .game
        .parent()
        .ok_or("game directory has no parent")?;
    if options.home.starts_with(source) || options.prefix.starts_with(source) {
        return Err(
            "private runtime and Wine prefix must be outside the original game installation".into(),
        );
    }
    fs::create_dir_all(&options.home)?;
    let mut lock = take_lock(&options.home, options.remove_lock, &ask)?;
    let private = private_install(
        &options.home,
        &options.game,
        &options.bridge,
        options.sound_dll.as_deref(),
    )?;
    if !options.separate_viewer {
        // Wine loads a builtin DLL only if a copy is on the Windows search
        // path too; the one it maps is WINEDLLPATH's.
        install_copy(
            &private,
            &options.native.join("i386-windows").join(NATIVE_DLL),
            NATIVE_DLL,
        )?;
    }
    fs::create_dir_all(options.prefix.join("dosdevices"))?;
    let prefix = options.prefix.canonicalize()?;
    link_if_missing(Path::new("/"), &prefix.join("dosdevices/z:"))?;
    let logs = options.home.join("logs");
    no_symlink(&logs)?;
    fs::create_dir_all(&logs)?;
    let session = Session::create(&options.home)?;
    let wine_runtime = wine_compat::Runtime::prepare(&wine, &session.0)?;
    let wine = &wine_runtime;
    let time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let log = logs.join(format!("session-{time}-{}", std::process::id()));
    let viewer_log = log.with_extension("viewer.log");
    let wine_log = log.with_extension("wine.log");
    if let Some(progress) = &progress {
        let _ = progress.send(Update::Logs(format!(
            "Wine: {}\nRenderer: {}\nBridge: {}\nRegistry: {}",
            wine_log.display(),
            viewer_log.display(),
            private.join("civ3-bridge.log").display(),
            log.with_extension("registry.log").display()
        )));
    }
    stage(
        &progress,
        "Preparing Wine prefix and registry",
        "Checking installation paths. Missing registry values need Wine services to start, usually after an update or moved install.",
    );
    if let Some(status) = initialize_registry(
        &wine,
        &options,
        &prefix,
        &settings,
        &private,
        &log.with_extension("registry.log"),
        &signal,
    )? {
        return Ok(status);
    }
    stage(
        &progress,
        "Installing DirectPlay",
        "Network games need Microsoft's DirectPlay in the Wine prefix. This runs once.",
    );
    if let Some(status) = install_directplay(
        &wine,
        &options,
        &prefix,
        &settings,
        &private,
        &log.with_extension("directplay.log"),
        &signal,
    )? {
        return Ok(status);
    }
    let menu_ready = session.path("menu-ready");
    // Written by the in-process viewer when its window closes
    // (`civ3r_native`). There is no viewer process to follow then.
    let viewer_closed = session.path("viewer-closed");
    stage(
        &progress,
        "Starting Wine",
        "Starting Windows services and loading the game executable.",
    );
    let mut overrides = OsString::from("ifc23=n,b;mscoree,mshtml=;");
    let (stdout, stderr) = output_log(&wine_log)?;
    let mut command = wine.command();
    configure(&mut command, &options, &prefix, &settings);
    let viewer = if options.separate_viewer {
        let (viewer, address) =
            match start_viewer(&options, &prefix, &settings, &session, &viewer_log, &signal)? {
                Started::Running(viewer, address) => (viewer, address),
                Started::Interrupted(status) => return Ok(status),
            };
        let token = fs::read_to_string(session.path("token"))?;
        command
            .env("CIV3R_VIEWER_ADDR", address)
            .env("CIV3R_VIEWER_TOKEN", token.trim())
            .env(
                "CIV3R_VIEWER",
                if options.fullscreen {
                    "external-fullscreen"
                } else {
                    "external"
                },
            );
        println!(
            "civ3-play-macos: native viewer log {}",
            viewer_log.display()
        );
        Some(viewer)
    } else {
        let mut path = options.native.clone().into_os_string();
        if let Some(more) = environment("WINEDLLPATH") {
            path.push(":");
            path.push(more);
        }
        overrides.push("civ3r_native=b;");
        command
            .env("CIV3R_VIEWER_CLOSED_FILE", &viewer_closed)
            .env("WINEDLLPATH", path)
            .env(
                "CIV3R_VIEWER",
                if options.fullscreen {
                    "native-fullscreen"
                } else {
                    "native"
                },
            );
        println!(
            "civ3-play-macos: renderer inside the game's process; its log is in the game log and {}",
            wine_log.display()
        );
        None
    };
    overrides.push(environment("WINEDLLOVERRIDES").unwrap_or_default());
    command
        .arg(windows_path(&private.join("Civ3Conquests.exe"))?)
        .current_dir(&private)
        .env(
            "CIV3R_GAME_WINDOW",
            if options.show_game { "show" } else { "hide" },
        )
        .env("CIV3R_GAME_MAP", if options.map { "on" } else { "off" })
        .env("CIV3R_GAME_RENDER", &options.game_render)
        .env("WINEDLLOVERRIDES", overrides)
        .env(FEX_X87, fex_x87_reduced_precision(environment(FEX_X87)))
        .env(HARDWARE_TSO, hardware_tso(environment(HARDWARE_TSO)))
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    println!(
        "civ3-play-macos: game log {}",
        private.join("civ3-bridge.log").display()
    );
    command.env("CIV3R_STARTUP_READY_FILE", &menu_ready);
    let bridge_log = private.join("civ3-bridge.log");
    let bridge_before = fs::metadata(&bridge_log)
        .ok()
        .and_then(|m| m.modified().ok());
    let game = ManagedChild::spawn_wine(&mut command)?;
    if let Some(marker) = game.session_marker() {
        // So a later launcher can find this game if this launcher ends first.
        if let Err(error) = lock.note_session(marker) {
            eprintln!("civ3-play-macos: cannot note the session in the launch lock: {error}");
        }
    }
    stage(
        &progress,
        "Waiting for the game",
        if cfg!(target_arch = "aarch64") {
            "Wine may be translating x86 code to ARM64. A first launch after an update can take minutes. Waiting for the bridge and renderer."
        } else {
            "Wine is loading the game. Waiting for the bridge and renderer."
        },
    );
    let mut bridge_seen = false;
    let mut menu_seen = false;
    let follow_viewer = !matches!(
        options.game_render.to_str(),
        Some("compat" | "compatibility")
    );
    // A dying game drops its viewer connection, so the viewer may exit cleanly
    // before Wine has finished tearing the game down. Give the game this long
    // to report its own status before treating the viewer exit as a user quit.
    let mut viewer_gone: Option<Instant> = None;
    loop {
        if !bridge_seen
            && fs::metadata(&bridge_log)
                .ok()
                .and_then(|m| m.modified().ok())
                .is_some_and(|modified| Some(modified) != bridge_before)
        {
            bridge_seen = true;
            stage(
                &progress,
                "Loading renderer and game art",
                "The game bridge is writing its log. Waiting for the first complete menu to be presented.",
            );
        }
        if !menu_seen && menu_ready.is_file() {
            menu_seen = true;
            if let Some(progress) = &progress {
                let _ = progress.send(Update::Ready);
            }
        }
        let interrupted = signal.load(Ordering::Relaxed);
        if interrupted != 0 {
            return Ok(interrupted as i32);
        }
        let game_status = game.status()?;
        let viewer_status = match &viewer {
            Some(viewer) if follow_viewer => viewer.status()?,
            // The in-process viewer closed. The game's side may be unable to
            // end its own process (a DLL detach waiting for a dead thread),
            // so its process group is stopped after the same timeout.
            None if follow_viewer && viewer_closed.is_file() => Some(ExitStatus::from_raw(0)),
            _ => None,
        };
        if viewer_status.is_some() && viewer_gone.is_none() {
            viewer_gone = Some(Instant::now() + STOP_TIMEOUT);
        }
        if game_status.is_some() || viewer_gone.is_some_and(|end| Instant::now() >= end) {
            return match session_end(
                game_status,
                viewer_status,
                menu_seen || menu_ready.is_file(),
            ) {
                Ending::Quit => {
                    if game_status.is_none() {
                        eprintln!("civ3-play-macos: native viewer closed; stopping owned game");
                    }
                    Ok(0)
                }
                Ending::GameFailed { status, code } => {
                    let message = format!(
                        "the game exited unexpectedly ({status}); see {} and {}",
                        wine_log.display(),
                        private.join("civ3-bridge.log").display()
                    );
                    eprintln!("civ3-play-macos: {message}");
                    if let Some(progress) = &progress {
                        let _ = progress.send(Update::Failure(message));
                    }
                    Ok(code)
                }
                Ending::ViewerFailed => Err(format!(
                    "native viewer failed; stopping owned game; see {}",
                    viewer_log.display()
                )
                .into()),
            };
        }
        thread::sleep(POLL_INTERVAL);
    }
}

enum Started {
    Running(ManagedChild, String),
    Interrupted(i32),
}

/// Starts the separate viewer process and waits for its address.
fn start_viewer(
    options: &Options,
    prefix: &Path,
    settings: &Path,
    session: &Session,
    viewer_log: &Path,
    signal: &AtomicUsize,
) -> Result<Started> {
    let (stdout, stderr) = output_log(viewer_log)?;
    let mut viewer = Command::new(&options.viewer);
    configure(&mut viewer, options, prefix, settings);
    viewer
        .args([
            OsStr::new("--listen"),
            OsStr::new("127.0.0.1:0"),
            OsStr::new("--token-file"),
        ])
        .env("CIV3R_STARTUP_READY_FILE", session.path("menu-ready"))
        .arg(session.path("token"))
        .arg("--ready-file")
        .arg(session.path("ready"))
        .arg("--wine-prefix")
        .arg(prefix)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    let viewer = ManagedChild(viewer.spawn()?, None);
    let deadline = Instant::now() + START_TIMEOUT;
    let address = loop {
        let interrupted = signal.load(Ordering::Relaxed);
        if interrupted != 0 {
            return Ok(Started::Interrupted(interrupted as i32));
        }
        if viewer.status()?.is_some() {
            eprint!("{}", fs::read_to_string(viewer_log).unwrap_or_default());
            return Err("native viewer failed to start".into());
        }
        if let Some(address) = ready_address(&session.path("ready"))? {
            break address;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "native viewer startup timed out; see {}",
                viewer_log.display()
            )
            .into());
        }
        thread::sleep(POLL_INTERVAL);
    };
    Ok(Started::Running(viewer, address))
}

/// How a game session ended, as far as the launcher can tell.
#[derive(Debug, PartialEq)]
enum Ending {
    /// The game quit cleanly, or the user closed the viewer while it ran.
    Quit,
    /// The game crashed or failed; `code` is the launcher's exit code.
    GameFailed { status: String, code: i32 },
    /// The viewer failed while the game was still running.
    ViewerFailed,
}

/// The game's own status wins: a viewer that exits because the game dropped
/// its connection must not hide the game's failure. `None` means still running.
fn session_end(game: Option<ExitStatus>, viewer: Option<ExitStatus>, menu_ready: bool) -> Ending {
    match (game, viewer) {
        (Some(status), _) if status.success() && !menu_ready => Ending::GameFailed {
            status: "exit code 0 before the first complete menu appeared".into(),
            code: 1,
        },
        (Some(status), _) if status.success() => Ending::Quit,
        (Some(status), _) => match (status.code(), status.signal()) {
            (Some(code), _) => Ending::GameFailed {
                status: format!("exit code {code}"),
                code,
            },
            (None, Some(signal)) => Ending::GameFailed {
                status: format!("killed by signal {signal}"),
                code: 128 + signal,
            },
            (None, None) => Ending::GameFailed {
                status: format!("{status}"),
                code: 1,
            },
        },
        (None, Some(status)) if !status.success() => Ending::ViewerFailed,
        (None, _) => Ending::Quit,
    }
}

// Failures are displayed by the native startup window when a display is available.

#[cfg(test)]
mod tests {
    use super::*;

    /// A private home for one test, removed at the end.
    struct TestHome(PathBuf);
    impl TestHome {
        fn new(name: &str) -> Self {
            let path = env::temp_dir().join(format!("civ3r-launcher-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestHome {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn held(home: &Path) -> launch_lock::LaunchLock {
        match launch_lock::claim(home).unwrap() {
            launch_lock::Claim::Owned { lock, .. } => lock,
            launch_lock::Claim::Busy(busy) => panic!("{busy:?}"),
        }
    }

    fn answering(yes: bool, asked: Arc<AtomicUsize>) -> Ask {
        Box::new(move |question: &Question| {
            assert!(question.text.contains("launch.lock"), "{question:?}");
            asked.fetch_add(1, Ordering::Relaxed);
            yes
        })
    }

    #[test]
    fn the_shortcut_journal_is_replayed_on_a_refused_start_and_when_it_ends() {
        // As `launch`: the recovery first, then the lock, which another
        // launcher holds here.
        let home = TestHome::new("refused");
        let _other = held(&home.0);
        let replays = Arc::new(AtomicUsize::new(0));
        let counted = replays.clone();
        let recovery = ShortcutRecovery::new(home.0.join("paused-system-shortcuts.txt"), move |journal: &Path| {
            assert!(journal.ends_with("paused-system-shortcuts.txt"));
            counted.fetch_add(1, Ordering::Relaxed);
            17
        });
        assert_eq!(replays.load(Ordering::Relaxed), 1, "replayed before the lock is tried");
        let asked = Arc::new(AtomicUsize::new(0));
        let refused = take_lock(&home.0, false, &answering(false, asked.clone())).unwrap_err().to_string();
        assert!(refused.contains("another launch owns") && refused.contains("--remove-lock"), "{refused}");
        assert_eq!(asked.load(Ordering::Relaxed), 1);
        drop(recovery);
        assert_eq!(replays.load(Ordering::Relaxed), 2, "and again as the refused launcher ends");
    }

    #[test]
    fn a_stale_lock_is_taken_over_without_asking() {
        let home = TestHome::new("stale");
        let mut child = Command::new("true").spawn().unwrap();
        let dead = child.id();
        child.wait().unwrap();
        fs::create_dir(home.0.join("launch.lock")).unwrap();
        fs::write(home.0.join("launch.lock/pid"), format!("{dead}\n")).unwrap();
        let asked = Arc::new(AtomicUsize::new(0));
        let _lock = take_lock(&home.0, false, &answering(false, asked.clone())).unwrap();
        assert_eq!(asked.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_held_lock_is_removed_only_when_the_user_says_so_or_with_remove_lock() {
        let home = TestHome::new("override");
        let first = held(&home.0);
        let asked = Arc::new(AtomicUsize::new(0));
        let second = take_lock(&home.0, false, &answering(true, asked.clone())).unwrap();
        assert_eq!(asked.load(Ordering::Relaxed), 1);
        drop(first);
        // `--remove-lock` asks nobody.
        let _third = take_lock(&home.0, true, &answering(false, asked.clone())).unwrap();
        assert_eq!(asked.load(Ordering::Relaxed), 1);
        drop(second);
    }

    #[test]
    fn only_the_games_own_processes_count_as_an_earlier_game() {
        let args = |list: &[&str]| list.iter().map(|a| a.as_bytes().to_vec()).collect::<Vec<_>>();
        assert!(runs_the_game(&args(&["wine", "Z:\\r\\game\\Civ3Conquests.exe"])));
        assert!(runs_the_game(&args(&["C:\\x\\CIV3CONQUESTS.EXE"])));
        assert!(!runs_the_game(&args(&["/r/wine/bin/wineserver"])));
        assert!(!runs_the_game(&args(&["C:\\windows\\system32\\services.exe"])));
        // No session noted: nothing to look for.
        assert!(earlier_game(&launch_lock::Record { pid: 1, started: 1, session: None }).is_empty());
    }

    #[test]
    fn clean_game_exit_before_a_menu_is_a_startup_failure() {
        let ok = ExitStatus::from_raw(0);
        assert_eq!(
            session_end(Some(ok), None, false),
            Ending::GameFailed {
                status: "exit code 0 before the first complete menu appeared".into(),
                code: 1,
            }
        );
        assert_eq!(
            session_end(Some(ok), Some(ok), false),
            Ending::GameFailed {
                status: "exit code 0 before the first complete menu appeared".into(),
                code: 1,
            }
        );
        assert_eq!(session_end(Some(ok), None, true), Ending::Quit);
        // A closed viewer while the game is still alive is the user's close.
        assert_eq!(session_end(None, Some(ok), false), Ending::Quit);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn managed_child_cleans_an_escaped_debugger_and_preserves_shared_and_unmarked_roles() {
        use std::io::{BufRead, BufReader};
        struct ChildGuard(Child);
        impl Drop for ChildGuard { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
        struct FixtureDir(PathBuf);
        impl Drop for FixtureDir {
            fn drop(&mut self) {
                let _ = fs::remove_file(self.0.join("winedbg"));
                let _ = fs::remove_file(self.0.join("wineserver"));
                let _ = fs::remove_dir(&self.0);
            }
        }
        let target = env::var_os("CARGO_TARGET_DIR").map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
        fs::create_dir_all(&target).unwrap();
        let fixture = FixtureDir(target.join(format!("owned-debugger-fixture-{}", random_hex().unwrap())));
        DirBuilder::new().mode(0o700).create(&fixture.0).unwrap();
        let executable = fixture.0.join("winedbg");
        let status = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .arg("--edition=2024")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/owned-debugger.rs"))
            .arg("-o").arg(&executable).env("TMPDIR", &fixture.0).status().unwrap();
        assert!(status.success());
        fs::hard_link(&executable, fixture.0.join("wineserver")).unwrap();
        let mut command = Command::new(&executable);
        command.env("CIV3R_CLEANUP_FIXTURE_ROLE", "parent")
            .env("WINEPREFIX", "same-prefix-fixture").process_group(0).stdout(Stdio::piped());
        let mut child = ManagedChild::spawn_wine(&mut command).unwrap();
        let marker = command.get_envs().find(|(key, _)| *key == "CIV3R_SESSION_TOKEN")
            .and_then(|(_, value)| value).unwrap().to_owned();
        let mut unmarked = ChildGuard(Command::new(&executable).args(["--auto", "1", "2"])
            .env("CIV3R_CLEANUP_FIXTURE_ROLE", "leaf").env("WINEPREFIX", "same-prefix-fixture")
            .env_remove("CIV3R_SESSION_TOKEN").stdout(Stdio::null()).spawn().unwrap());
        let mut shared = ChildGuard(Command::new(fixture.0.join("wineserver")).args(["--auto", "272", "528"])
            .env("CIV3R_CLEANUP_FIXTURE_ROLE", "leaf").env("WINEPREFIX", "same-prefix-fixture")
            .env("CIV3R_SESSION_TOKEN", marker).stdout(Stdio::null()).spawn().unwrap());
        let stdout = child.0.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
                if line == "DETACHED_DEBUGGER_READY" { let _ = tx.send(()); break; }
            }
        });
        rx.recv_timeout(Duration::from_secs(5)).expect("detached fixture did not start");
        let escaped = child.1.as_ref().unwrap().processes().unwrap();
        assert_eq!(escaped.len(), 1, "shared marked server and unmarked debugger must be excluded");
        drop(child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !escaped[0].exited().unwrap() {
            assert!(Instant::now() < deadline, "escaped debugger survived ManagedChild drop");
            thread::yield_now();
        }
        assert!(unmarked.0.try_wait().unwrap().is_none(), "unmarked same-prefix debugger must survive");
        assert!(shared.0.try_wait().unwrap().is_none(), "marked shared wineserver role must survive");
    }

    #[test]
    fn cancellation_releases_the_owned_process_group() {
        let cancel = Arc::new(AtomicUsize::new(0));
        let worker_cancel = cancel.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let mut command = Command::new("sleep");
            command.arg("30").process_group(0);
            let child = ManagedChild(command.spawn().unwrap(), None);
            tx.send(child.group()).unwrap();
            while worker_cancel.load(Ordering::Relaxed) == 0 {
                thread::sleep(POLL_INTERVAL);
            }
            drop(child);
        });
        let group = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        cancel.store(143, Ordering::Relaxed);
        worker.join().unwrap();
        assert_eq!(
            rustix::process::kill_process_group(group, rustix::process::Signal::TERM).unwrap_err(),
            rustix::io::Errno::SRCH
        );
    }

    #[test]
    fn status_peek_keeps_leader_reserved_until_group_cleanup() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exit 7"]).process_group(0);
        let child = ManagedChild(command.spawn().unwrap(), None);
        let group = child.group();
        let deadline = Instant::now() + Duration::from_secs(2);
        while child.status().unwrap().is_none() {
            assert!(Instant::now() < deadline, "test child did not exit");
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(child.status().unwrap().unwrap().code(), Some(7));
        let options = rustix::process::WaitIdOptions::EXITED
            | rustix::process::WaitIdOptions::NOHANG
            | rustix::process::WaitIdOptions::NOWAIT;
        assert!(
            rustix::process::waitid(rustix::process::WaitId::Pid(group), options)
                .unwrap()
                .is_some(),
            "status must not reap the group leader"
        );
        drop(child);
        assert_eq!(
            rustix::process::waitid(rustix::process::WaitId::Pid(group), options).unwrap_err(),
            rustix::io::Errno::CHILD
        );
    }

    #[test]
    fn session_end_reports_game_failures_over_viewer_exit() {
        let ok = ExitStatus::from_raw(0);
        let code5 = ExitStatus::from_raw(5 << 8);
        let segv = ExitStatus::from_raw(11);
        assert_eq!(session_end(Some(ok), None, true), Ending::Quit);
        assert_eq!(session_end(Some(ok), Some(code5), true), Ending::Quit);
        assert_eq!(session_end(None, Some(ok), true), Ending::Quit);
        assert_eq!(session_end(None, None, true), Ending::Quit);
        assert_eq!(session_end(None, Some(code5), true), Ending::ViewerFailed);
        let failed = Ending::GameFailed {
            status: "exit code 5".into(),
            code: 5,
        };
        assert_eq!(session_end(Some(code5), None, true), failed);
        assert_eq!(session_end(Some(code5), Some(ok), true), failed);
        assert_eq!(
            session_end(Some(segv), Some(ok), true),
            Ending::GameFailed {
                status: "killed by signal 11".into(),
                code: 139,
            }
        );
    }

    #[test]
    fn scale_rejects_nonfinite_and_shell_numeric_extensions() {
        for value in [
            "", "0", "0.5", "-1", "NaN", "inf", "1e2", "2.", ".5", "1.2.3", " 2",
        ] {
            assert!(!valid_scale(OsStr::new(value)), "{value}");
        }
        for value in ["1", "2", "1.5", "1.000", "002"] {
            assert!(valid_scale(OsStr::new(value)), "{value}");
        }
    }
    #[test]
    fn fex_x87_defaults_to_reduced_precision_unless_set() {
        assert_eq!(fex_x87_reduced_precision(None), "1");
        assert_eq!(fex_x87_reduced_precision(Some("0".into())), "0");
    }

    #[test]
    fn hardware_tso_defaults_on_unless_set() {
        assert_eq!(hardware_tso(None), "1");
        assert_eq!(hardware_tso(Some("0".into())), "0");
    }

    #[test]
    fn saved_32_bit_registry_values_are_recognised() {
        let saved = r#"WINE REGISTRY Version 2

[Software\\Wow6432Node\\Infogrames\\Conquests] 1792408986
#time=1dd5fbc365e65f6
"Complete_Version"=dword:00000001
"Install_Path"="Z:\\Users\\a b\\Conquests"

[Software\\Infogrames\\Civ3PTW] 1792408990
"Gold_Version"=dword:00000001
"#;
        let key = r"HKLM\Software\Infogrames\Conquests";
        let path = r"Z:\Users\a b\Conquests";
        assert!(registered_32(saved, key, "Install_Path", "REG_SZ", path));
        assert!(registered_32(
            saved,
            key,
            "Complete_Version",
            "REG_DWORD",
            "1"
        ));
        assert!(!registered_32(
            saved,
            key,
            "Complete_Version",
            "REG_DWORD",
            "2"
        ));
        assert!(!registered_32(
            saved,
            key,
            "Install_Path",
            "REG_SZ",
            r"Z:\Other"
        ));
        assert!(!registered_32(saved, key, "Missing", "REG_SZ", path));
        // Only the 32-bit view counts, and a value must be in its own key.
        let ptw = r"HKLM\Software\Infogrames\Civ3PTW";
        assert!(!registered_32(saved, ptw, "Gold_Version", "REG_DWORD", "1"));
        assert!(!registered_32(
            saved,
            r"HKLM\Software\Infogrames",
            "Install_Path",
            "REG_SZ",
            path
        ));
        assert!(!registered_32("", key, "Install_Path", "REG_SZ", path));
        assert!(!registered_32(
            saved,
            key,
            "Install_Path",
            "REG_SZ",
            "Z:\\ü"
        ));
    }

    #[test]
    fn wine_path_preserves_spaces_and_backslash_converts_only_separators() {
        assert_eq!(
            windows_path(Path::new("/Users/Test Name/遊戲/Conquests.exe")).unwrap(),
            "Z:\\Users\\Test Name\\遊戲\\Conquests.exe"
        );
    }

    #[test]
    fn private_install_stages_sound_dll_only_when_replacement_is_supplied() {
        let root = TestHome::new("sound-stage");
        let game = root.0.join("orig/Civilization III Complete/Conquests");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("IFC23.dll"), b"orig-ifc").unwrap();
        fs::write(game.join("sound.dll"), b"orig-sound").unwrap();
        let bridge = root.0.join("bridge.dll");
        fs::write(&bridge, b"new-ifc").unwrap();
        let replacement_sound = root.0.join("new-sound.dll");
        fs::write(&replacement_sound, b"new-sound").unwrap();

        // Without a replacement sound.dll, original sound.dll stays linked directly.
        let home_plain = root.0.join("home-plain");
        fs::create_dir_all(&home_plain).unwrap();
        let priv_plain = private_install(&home_plain, &game, &bridge, None).unwrap();
        assert!(!priv_plain.join("sound_orig.dll").exists());
        assert!(fs::symlink_metadata(priv_plain.join("sound.dll")).unwrap().file_type().is_symlink());

        // With a replacement sound.dll, sound_orig.dll links to original and sound.dll is replaced.
        let home_sound = root.0.join("home-sound");
        fs::create_dir_all(&home_sound).unwrap();
        let priv_sound =
            private_install(&home_sound, &game, &bridge, Some(&replacement_sound)).unwrap();
        assert!(fs::symlink_metadata(priv_sound.join("sound_orig.dll")).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(priv_sound.join("sound_orig.dll")).unwrap(), game.join("sound.dll"));
        assert!(!fs::symlink_metadata(priv_sound.join("sound.dll")).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(priv_sound.join("sound.dll")).unwrap(), b"new-sound");
    }
}

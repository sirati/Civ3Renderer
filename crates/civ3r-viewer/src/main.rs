#![forbid(unsafe_code)]

//! The native window owns the process main thread, as required by AppKit.
mod art;
mod icon;
mod paths;

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use viewer_transport::Message;

struct Options {
    listen: SocketAddr,
    token: String,
    ready: Option<PathBuf>,
    paths: paths::PathMapper,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut options = Self {
            listen: "127.0.0.1:0".parse().unwrap(),
            token: std::env::var("CIV3R_VIEWER_TOKEN").unwrap_or_default(),
            ready: None,
            paths: Default::default(),
        };
        options.paths.prefix = std::env::var_os("WINEPREFIX").map(PathBuf::from);
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            if flag == "--help" {
                println!(
                    "civ3r-viewer [--listen 127.0.0.1:0] --token-file FILE [--ready-file FILE] [--wine-prefix PATH] [--map-root WINDOWS=LOCAL]..."
                );
                std::process::exit(0);
            }
            let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
            match flag.as_str() {
                "--listen" => {
                    options.listen = value.parse().map_err(|e| format!("--listen: {e}"))?
                }
                "--token-file" => {
                    options.token = std::fs::read_to_string(value)
                        .map_err(|e| e.to_string())?
                        .trim()
                        .to_owned()
                }
                "--ready-file" => options.ready = Some(value.into()),
                "--wine-prefix" => options.paths.prefix = Some(value.into()),
                "--map-root" => options.paths.add(&value)?,
                _ => return Err(format!("unknown option {flag}")),
            }
        }
        if !options.listen.ip().is_loopback() {
            return Err("--listen must be a numeric loopback address".into());
        }
        if !viewer_transport::valid_token(&options.token) {
            return Err("supply a random 32..=128-character hex token through --token-file or CIV3R_VIEWER_TOKEN".into());
        }
        Ok(options)
    }
}

struct ReadyFile(Option<PathBuf>);
impl Drop for ReadyFile {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn ready_file(path: Option<&PathBuf>, address: SocketAddr) -> io::Result<ReadyFile> {
    if let Some(path) = path {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        writeln!(file, "{address}")?;
    }
    Ok(ReadyFile(path.cloned()))
}

struct StopGuard {
    stop: Arc<AtomicBool>,
    outgoing: mpsc::SyncSender<Message>,
}
impl Drop for StopGuard {
    fn drop(&mut self) {
        let _ = self.outgoing.try_send(Message::Quit);
        self.stop.store(true, Ordering::Release);
    }
}

fn session(
    mut stream: TcpStream,
    driver: scene::Driver,
    jobs: Arc<art::ArtJobs>,
    art: Arc<art::State>,
    stop: Arc<AtomicBool>,
    outgoing: mpsc::Receiver<Message>,
) -> io::Result<()> {
    let mut reader = stream.try_clone()?;
    let closer = stream.try_clone()?;
    let (tx, incoming) = mpsc::sync_channel(1);
    let read_thread = std::thread::Builder::new()
        .name("viewer-receive".into())
        .spawn(move || {
            loop {
                let message = viewer_transport::read_message(&mut reader);
                let done = message.is_err();
                if tx.send(message).is_err() || done {
                    break;
                }
            }
        })?;
    let result = (|| {
        let mut pending_scene = None;
        let mut latest_serial = 0;
        let mut status = (u64::MAX, u64::MAX);
        'serve: loop {
            for message in outgoing.try_iter() {
                viewer_transport::write_message(&mut stream, &message)?;
                if matches!(message, Message::Quit) {
                    break 'serve;
                }
            }
            // This process's render errors, for the game's render error log.
            for line in render_errors::take_forward() {
                viewer_transport::write_message(&mut stream, &Message::Error(line))?;
            }
            if stop.load(Ordering::Acquire) {
                viewer_transport::write_message(&mut stream, &Message::Quit)?;
                break;
            }
            if let Some(serial) = pending_scene {
                if !driver.busy() {
                    viewer_transport::write_message(&mut stream, &Message::SceneTaken(serial))?;
                    pending_scene = None;
                }
            }
            let inputs = driver.take_input();
            if !inputs.is_empty() {
                if inputs.len() > 4096 {
                    return Err(io::Error::other("native input backlog exhausted"));
                }
                let quit = inputs.iter().any(|i| matches!(i, scene::Input::Quit));
                viewer_transport::write_message(
                    &mut stream,
                    &Message::Input(scene::wire::encode_input(&inputs)),
                )?;
                if quit {
                    viewer_transport::write_message(&mut stream, &Message::Quit)?;
                    break;
                }
            }
            let processed = art.processed.load(Ordering::Acquire);
            let uploaded = art
                .assets
                .get()
                .map_or(0, tile_viewer::Assets::uploaded_generation);
            if status != (processed, uploaded) {
                viewer_transport::write_message(
                    &mut stream,
                    &Message::Status {
                        processed,
                        uploaded,
                    },
                )?;
                status = (processed, uploaded);
            }
            match incoming.recv_timeout(Duration::from_millis(4)) {
                Ok(Ok(Message::Assets(bytes))) => {
                    let assets = scene::wire::decode_assets(&bytes)
                        .map_err(|e| io::Error::other(e.to_string()))?;
                    jobs.push(assets).map_err(io::Error::other)?;
                }
                Ok(Ok(Message::Scene { serial, bytes })) => {
                    if pending_scene.is_some() || serial <= latest_serial {
                        return Err(io::Error::other("game exceeded one outstanding scene"));
                    }
                    let updates = viewer_transport::decode_updates_bounded(&bytes)?;
                    latest_serial = serial;
                    driver.send(updates);
                    pending_scene = Some(serial);
                }
                Ok(Ok(Message::Quit)) => break,
                Ok(Ok(_)) => return Err(io::Error::other("unexpected message from game")),
                Ok(Err(e)) => return Err(e),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        Ok(())
    })();
    if let Err(error) = &result {
        let _ = viewer_transport::write_message(&mut stream, &Message::Error(error.to_string()));
        let _ = viewer_transport::write_message(&mut stream, &Message::Quit);
    }
    jobs.close();
    stop.store(true, Ordering::Release);
    let _ = closer.shutdown(Shutdown::Both);
    drop(incoming);
    let _ = read_thread.join();
    result
}

fn run(options: Options) -> Result<(), String> {
    let max_texture =
        tile_render::gpu::max_texture_dimension().ok_or("no native graphics adapter")?;
    let listener = TcpListener::bind(options.listen).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    let _ready = ready_file(options.ready.as_ref(), address).map_err(|e| e.to_string())?;
    log::info!("native viewer listening on {address}, texture limit {max_texture}");
    // The first launch after a bridge update waits minutes for FEX to translate
    // the bridge before it connects; the launcher stops a viewer it outlives.
    let deadline = Instant::now() + Duration::from_secs(900);
    let (stream, fullscreen) = loop {
        if Instant::now() >= deadline {
            return Err("no game connected within 15 minutes".into());
        }
        match listener.accept() {
            Ok((stream, _)) => {
                match viewer_transport::accept(stream, &options.token, max_texture) {
                    Ok(accepted) => break accepted,
                    Err(error) => log::warn!("rejected viewer connection: {error}"),
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(error) => return Err(error.to_string()),
        }
    };
    drop(listener);
    let (driver, display) = scene::connect();
    let jobs = Arc::new(art::ArtJobs::default());
    let state = Arc::new(art::State::default());
    let stop = Arc::new(AtomicBool::new(false));
    let (outgoing, outputs) = mpsc::sync_channel(32);
    let (initial_tx, initial) = mpsc::sync_channel(1);
    let worker = art::worker(
        jobs.clone(),
        state.clone(),
        options.paths,
        max_texture,
        initial_tx,
        outgoing.clone(),
    )
    .map_err(|e| e.to_string())?;
    let network_stop = stop.clone();
    let network_jobs = jobs.clone();
    let network = std::thread::Builder::new()
        .name("viewer-session".into())
        .spawn(move || {
            let result = session(
                stream,
                driver,
                network_jobs,
                state,
                network_stop.clone(),
                outputs,
            );
            network_stop.store(true, Ordering::Release);
            if let Err(error) = result {
                log::warn!("game connection: {error}");
            }
        })
        .map_err(|e| e.to_string())?;
    let guard = StopGuard {
        stop: stop.clone(),
        outgoing,
    };
    let result = (|| {
        let assets = loop {
            match initial.recv_timeout(Duration::from_millis(50)) {
                Ok(assets) => break assets?,
                Err(mpsc::RecvTimeoutError::Timeout) if !stop.load(Ordering::Acquire) => {}
                _ => return Err("game disconnected before initial art was ready".into()),
            }
        };
        let config = tile_viewer::ViewerConfig {
            title: "Civ III (native renderer)".into(),
            application_icon: icon::application(),
            size: (1024, 768),
            fullscreen,
            overhang: (128, 128, 128, 128),
            ..Default::default()
        };
        tile_viewer::run_until(config, assets, display, stop.clone()).map_err(|e| e.to_string())
    })();
    if let Err(error) = &result {
        let _ = guard
            .outgoing
            .try_send(Message::Error(format!("viewer: {error}")));
    }
    drop(guard);
    jobs.close();
    let _ = network.join();
    let _ = worker.join();
    result
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    if let Err(error) = Options::parse().and_then(run) {
        eprintln!("civ3r-viewer: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn startup_and_later_mandatory_art_failure_restore_game_without_false_ack() {
        for initial_failure in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let jobs = Arc::new(art::ArtJobs::default());
            let state = Arc::new(art::State::default());
            let stop = Arc::new(AtomicBool::new(false));
            let (outgoing, outputs) = mpsc::sync_channel(32);
            let (initial_tx, initial) = mpsc::sync_channel(1);
            let worker = art::worker(
                jobs.clone(),
                state.clone(),
                Default::default(),
                16384,
                initial_tx,
                outgoing,
            )
            .unwrap();
            let server = std::thread::spawn(move || {
                let (stream, _) =
                    viewer_transport::accept(listener.accept().unwrap().0, TOKEN, 16384).unwrap();
                let (driver, _display) = scene::connect();
                session(stream, driver, jobs, state, stop, outputs).unwrap();
            });
            let client = viewer_transport::Client::connect(address, TOKEN, false).unwrap();
            let description = if initial_failure {
                "invalid ["
            } else {
                "grid = { layout = \"orthogonal\", step = [1, 1] }"
            };
            client
                .set_assets(&scene::wire::Assets {
                    generation: 9,
                    description: description.into(),
                    ..Default::default()
                })
                .unwrap();
            if initial_failure {
                assert!(
                    initial
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap()
                        .is_err()
                );
            } else {
                let assets = initial
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap()
                    .unwrap();
                assert_eq!(assets.latest(), 9);
                assert_eq!(assets.uploaded_generation(), 0);
                let deadline = Instant::now() + Duration::from_secs(10);
                while client.status() != 9 {
                    assert!(Instant::now() < deadline);
                    std::thread::yield_now();
                }
                client
                    .set_assets(&scene::wire::Assets {
                        generation: 10,
                        append: true,
                        description: "invalid [".into(),
                        ..Default::default()
                    })
                    .unwrap();
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if client
                    .take_input()
                    .iter()
                    .any(|i| matches!(i, scene::Input::Quit))
                {
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            assert_eq!(client.status(), if initial_failure { 0 } else { 9 });
            assert_eq!(client.uploaded_generation(), 0);
            worker.join().unwrap();
            server.join().unwrap();
        }
    }

    #[test]
    fn scene_is_acknowledged_only_after_display_consumes_it() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (driver, mut display) = scene::connect();
        let stop = Arc::new(AtomicBool::new(false));
        let network_stop = stop.clone();
        let (_outgoing, outputs) = mpsc::sync_channel(32);
        let server = std::thread::spawn(move || {
            let (stream, _) =
                viewer_transport::accept(listener.accept().unwrap().0, TOKEN, 16384).unwrap();
            session(
                stream,
                driver,
                Arc::new(art::ArtJobs::default()),
                Arc::new(art::State::default()),
                network_stop,
                outputs,
            )
        });
        let client = viewer_transport::Client::connect(address, TOKEN, false).unwrap();
        assert!(client.publish(&[scene::Update::Tick(21)]).0);
        assert!(!client.publish(&[scene::Update::Tick(22)]).0);
        assert!(display.receive(Duration::from_secs(10)));
        assert_eq!(display.scene().tick, 21);
        let deadline = Instant::now() + Duration::from_secs(10);
        while client.busy() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        stop.store(true, Ordering::Release);
        server.join().unwrap().unwrap();
    }
}

//! The reimplemented renderer, running inside the game: a publisher thread
//! turns the game's world and view into scenes, a viewer window draws them.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use civ3_sys::{Civ3, Civ3Error};
use render_desc::Compiled;
use scene::Link;

use crate::assets;
use crate::scene_map::Mapper;
use crate::world::{Civ3City, Civ3Tile, Civ3View, Civ3World, WallBuilding};

/// The viewer window's title (also how we tell it from the game's window).
const VIEWER_TITLE: &str = "Civ III (reimplemented renderer)";

/// How often the publisher looks at the game.
const POLL: Duration = Duration::from_millis(33);

/// A copy of the parts of the game state the map is drawn from.
pub fn snapshot(game: &Civ3) -> Result<(Civ3World, Civ3View), Civ3Error> {
    let view = game.view()?;
    let map = game.map()?;
    let tiles = game
        .tiles(&map)?
        .iter()
        .map(|t| Civ3Tile {
            square_type: t.square_type(),
            square_parts: t.square_parts() as u16,
            overlays: t.overlays(),
            terrain_overlays: t.terrain_overlays(),
            river: t.river_code(),
            resource: t.resource_type(),
            owner: t.territory_owner(),
            city: t.city_id(),
            fow_status: t.fow_status(),
            visibility: t.visibility(),
            explored: t.explored(),
            v3: t.v3(),
            visibility_d0: t.visibility_d0(),
            visible_overlays: t.visible_overlays(),
            victory_location: t.victory_location(),
        })
        .collect();
    let players = game.players()?;
    let cities = game
        .cities()?
        .into_iter()
        .map(|c| Civ3City { id: c.id, x: c.x, y: c.y, civ: c.civ, size: c.size, improvements: c.improvements })
        .collect();
    let world = Civ3World {
        player: view.player_civ,
        resource_icons: game.resource_icons()?,
        resource_visible: game.resources_visible_to(view.player_civ)?,
        eras: players.eras,
        color_tables: players.color_tables,
        capitals: players.capitals,
        races: players.races,
        culture_groups: players.culture_groups,
        max_town: players.max_town,
        max_city: players.max_city,
        cities,
        governments: players.governments,
        wall_buildings: players
            .wall_buildings
            .iter()
            .map(|b| WallBuilding { id: b.id, government: b.government, obsolete_for: b.obsolete_for })
            .collect(),
        width: map.width,
        height: map.height,
        wrap_x: map.wrap_x,
        wrap_y: map.wrap_y,
        seed: map.seed,
        flags: map.flags,
        tiles,
    };
    let view = Civ3View { camera_x: view.camera_x, camera_y: view.camera_y, tile_x: view.tile_x, tile_y: view.tile_y };
    Ok((world, view))
}

#[derive(Debug, thiserror::Error)]
pub enum LiveError {
    #[error("render description {0}: {1}")]
    Desc(String, String),
    #[error("game folder: {0}")]
    Install(std::io::Error),
}

/// The game install root ("Civilization III Complete"): the parent of the
/// folder the running executable is in.
fn install_root() -> Result<PathBuf, LiveError> {
    let exe = std::env::current_exe().map_err(LiveError::Install)?;
    exe.parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .ok_or_else(|| LiveError::Install(std::io::Error::other("executable has no grandparent folder")))
}

/// The description text and a name for messages.
fn description() -> Result<(String, String), LiveError> {
    match std::env::var("CIV3R_DESC") {
        Ok(path) => {
            let text = std::fs::read_to_string(&path).map_err(|e| LiveError::Desc(path.clone(), e.to_string()))?;
            Ok((path, text))
        }
        Err(_) => Ok(("built-in civ3.toml".to_owned(), assets::BUILTIN_DESC.to_owned())),
    }
}

/// Where the game currently loads art from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ArtFolders {
    scenario: Option<String>,
    search: Vec<String>,
}

impl ArtFolders {
    fn of(game: &Civ3) -> Self {
        Self { scenario: game.scenario_folder().ok().flatten(), search: game.search_roots().unwrap_or_default() }
    }
}

struct Compiler {
    install: PathBuf,
    name: String,
    text: String,
}

impl Compiler {
    fn compile(&self, folders: &ArtFolders) -> Result<Arc<Compiled>, LiveError> {
        let files = assets::game_files(&self.install, folders.scenario.as_deref(), &folders.search);
        let compiled = assets::compile(&self.text, files).map_err(|e| LiveError::Desc(self.name.clone(), e.to_string()))?;
        log::info!("live renderer: {} layers, {} sprites ({folders:?})", compiled.layers.len(), compiled.sprite_names.len());
        Ok(Arc::new(compiled))
    }
}

/// Starts the publisher and the viewer window; returns at once.
pub fn start(game: Civ3) -> Result<(), LiveError> {
    let (name, text) = description()?;
    let compiler = Compiler { install: install_root()?, name, text };
    let folders = ArtFolders::of(&game);
    let assets = tile_viewer::Assets::new(compiler.compile(&folders)?);
    let link = Link::new();
    {
        let (assets, link) = (assets.clone(), link.clone());
        std::thread::Builder::new()
            .name("civ3r-publish".into())
            .spawn(move || publish(game, &compiler, folders, &assets, &link))
            .map_err(LiveError::Install)?;
    }
    std::thread::Builder::new()
        .name("civ3r-viewer".into())
        .spawn(move || {
            let config = tile_viewer::ViewerConfig {
                title: VIEWER_TITLE.into(),
                size: (1024, 768),
                fullscreen: std::env::var("CIV3R_VIEWER").is_ok_and(|v| v == "fullscreen"),
                overhang: (0, 128, 0, 128),
                ..Default::default()
            };
            if let Err(e) = tile_viewer::run(config, assets, link) {
                log::error!("viewer: {e}");
            }
        })
        .map_err(LiveError::Install)?;
    Ok(())
}

/// Publishes a new scene whenever what the game shows changes.
fn publish(game: Civ3, compiler: &Compiler, mut folders: ArtFolders, assets: &tile_viewer::Assets, link: &Link) {
    let mut compiled = assets.current().1;
    let mut mapper = Mapper::new(&compiled);
    let mut last: Option<(Civ3World, Civ3View)> = None;
    let mut seq = 0;
    let mut input = InputForwarder::default();
    loop {
        std::thread::sleep(POLL);
        // A scenario brings its own art.
        let now = ArtFolders::of(&game);
        if now != folders {
            match compiler.compile(&now) {
                Ok(c) => {
                    assets.replace(c.clone());
                    compiled = c;
                    mapper = Mapper::new(&compiled);
                    last = None;
                }
                Err(e) => log::error!("recompiling for {now:?}: {e}"),
            }
            folders = now;
        }
        forward_input(&mut input, link.take_input());
        let state = match snapshot(&game) {
            Ok(s) => s,
            // Between games, or while loading.
            Err(Civ3Error::NoGame) => continue,
            Err(e) => {
                log::debug!("snapshot: {e}");
                continue;
            }
        };
        if last.as_ref() == Some(&state) {
            continue;
        }
        let (world, view) = &state;
        seq += 1;
        let mut scene = mapper.scene(world, view, (1024, 768), 0);
        scene.seq = seq;
        link.publish(scene);
        last = Some(state);
    }
}

/// Delivers the viewer's input to the game's window. The viewer shows the
/// game's map with the game's camera at its own top-left corner, 1:1, so a
/// viewer pixel is the same game client pixel.
#[derive(Default)]
struct InputForwarder {
    window: Option<inproc::window::Window>,
    held: inproc::window::Buttons,
}

impl InputForwarder {
    fn game_window(&mut self) -> Option<inproc::window::Window> {
        if self.window.is_none() {
            self.window = inproc::window::Window::of_this_process()
                .into_iter()
                .filter(|w| w.title() != VIEWER_TITLE)
                .max_by_key(|w| w.client_size().map_or(0, |(w, h)| w as u64 * h as u64));
            if let Some(w) = self.window {
                log::info!("forwarding input to {:?} ({:?})", w.title(), w.client_size());
            }
        }
        self.window
    }
}

fn forward_input(forwarder: &mut InputForwarder, inputs: Vec<scene::Input>) {
    use inproc::window::MouseButton;
    use scene::{Button, Input};
    if inputs.is_empty() {
        return;
    }
    let Some(window) = forwarder.game_window() else { return };
    for input in inputs {
        log::trace!("input {input:?}");
        match input {
            Input::MouseMove { x, y } => window.mouse_move(x, y, forwarder.held),
            Input::MouseButton { button, down, x, y } => {
                let button = match button {
                    Button::Left => {
                        forwarder.held.left = down;
                        MouseButton::Left
                    }
                    Button::Right => {
                        forwarder.held.right = down;
                        MouseButton::Right
                    }
                    Button::Middle => {
                        forwarder.held.middle = down;
                        MouseButton::Middle
                    }
                };
                window.mouse_button(button, down, x, y, forwarder.held);
            }
            Input::Wheel { delta, x, y } => window.wheel(delta, x, y, forwarder.held),
            Input::Key { vk, down, text } => {
                window.key(vk, down);
                if let (true, Some(c)) = (down, text) {
                    window.char(c);
                }
            }
            Input::Resized { .. } | Input::Quit => {}
        }
    }
}

//! The mouse wheel on the game's interface where the game itself drops it
//! (`civ3_sys::controls::wheel`): the Civilopedia's article and lists, the
//! Foreign advisor, the Hall of Fame, the load/save file dialog, text
//! boxes, any vertical scroll bar under the pointer, buttons on screens
//! that scroll their own lists, and diplomacy's trade tables. The viewer
//! sends each notch as its own `WM_MOUSEWHEEL` (`tile_viewer::input`
//! accumulates trackpad and high-resolution deltas into notches); on the
//! game's thread, before the game sees the message, the game's own hit
//! test finds the form the game would give it to. Where that form takes
//! the wheel itself (list boxes with a bar, the advisors that scroll by
//! region, the map) the message goes on unchanged; elsewhere the step is
//! done the way the game's own scroll controls do it (a bar's arrow
//! command, the screen's own wheel handler, a click on a painted arrow),
//! and the message is consumed. Where nothing scrolls it goes on to the
//! game, which ignores it.

use std::sync::OnceLock;

use civ3_sys::controls::wheel::{self, Action};
use civ3_sys::controls::Sites;
use civ3_sys::{Build, Civ3};
use inproc::message::Message;

const WM_MOUSEWHEEL: u32 = 0x020A;

static SITES: OnceLock<Sites> = OnceLock::new();

pub fn install(game: Civ3) -> Result<(), String> {
    if game.build != Build::PcGames {
        return Err(format!("the interface wheel is not decoded for {:?}", game.build));
    }
    let sites = civ3_sys::controls::sites(game.build).ok_or("no control sites")?;
    wheel::live::validate().map_err(|e| e.to_string())?;
    SITES.set(sites).map_err(|_| "interface wheel installed twice".to_owned())?;
    log::info!("interface wheel: scroll bars, text, file lists and diplomacy tables the game leaves without the wheel");
    Ok(())
}

/// Called by the shared game-window interceptor, on the game's thread.
pub fn handle_message(message: Message) -> Option<isize> {
    if message.id != WM_MOUSEWHEEL {
        return None;
    }
    let sites = SITES.get()?;
    let delta = (message.wparam >> 16) as u16 as i16 as i32;
    let (sx, sy) = (message.lparam as u32 as u16 as i16 as i32, (message.lparam as u32 >> 16) as u16 as i16 as i32);
    let (x, y) = message.window.screen_to_client(sx, sy)?;
    let hit = wheel::live::hit(x, y)?;
    let action = wheel::route(&civ3_sys::controls::Live, sites, hit, (x, y), delta);
    if action == Action::Native {
        return None;
    }
    log::debug!("interface wheel {delta} at ({x}, {y}) on {hit:#x}: {action:?}");
    wheel::live::perform(action, delta).then_some(0)
}

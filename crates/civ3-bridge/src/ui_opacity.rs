#![cfg_attr(not(windows), allow(dead_code))]
//! Which regions of image files have no transparent pixels (palette
//! indices 254 and 255, as the interface's colour-keyed pictures are
//! drawn): such a picture hides what it is drawn over, which lets the UI
//! mirror drop what a redraw covers (`UiModel::compact`).
//!
//! The game's thread only looks the answer up and, the first time, asks
//! for it (an unbounded tokio send: no lock). A thread of its own
//! (`civ3r-opacity`) reads and decodes the files and publishes the answers
//! (`Latest`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
pub(crate) use civ3_ui::selection::FilePixels as GameFile;

/// Per file (its path as the game opened it): regions (x, y, w, h) and
/// whether they are opaque.
type Known = HashMap<String, Vec<([i32; 4], bool)>>;

static KNOWN: handoff::Latest<Known> = handoff::Latest::new();
/// The files decoded so far, for their pixels ([`pixel`]): the last
/// [`SHARED_FILES`] of them.
static FILES: handoff::Latest<Vec<(String, Arc<civ3_assets::pcx::Indexed>)>> = handoff::Latest::new();
/// File indices already decoded by the exe. Semantic readers retain an
/// Arc independently of this bounded cache and the source image's lifetime.
static GAME_FILES: handoff::Latest<Vec<(String, Arc<GameFile>)>> = handoff::Latest::new();

pub(crate) fn cache_game_file(path: &str, image: Arc<GameFile>) {
    GAME_FILES.update(|old| {
        let mut shared = old.cloned().unwrap_or_default();
        shared.retain(|(p, _)| p != path);
        if shared.len() >= SHARED_FILES { shared.remove(0); }
        shared.push((path.to_owned(), image.clone()));
        Some(shared)
    });
}
const SHARED_FILES: usize = 64;
static ASK: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<(String, [i32; 4])>> = std::sync::OnceLock::new();

/// Questions sent from this thread: each once (at most this many).
const MAX_ASKED: usize = 1 << 16;
/// Decoded files the thread keeps for the next questions.
const KEPT_FILES: usize = 8;

thread_local! {
    static ASKED: std::cell::RefCell<HashSet<(String, [i32; 4])>> = std::cell::RefCell::new(HashSet::new());
}

/// Whether region `rect` (x, y, w, h) of the image file at `path` has no
/// transparent pixels; `None` while not known (asked for then).
pub fn opaque(path: &str, rect: [i32; 4]) -> Option<bool> {
    let known = KNOWN.load();
    if let Some(&(_, opaque)) = known.as_ref().and_then(|k| k.get(path)).and_then(|regions| regions.iter().find(|(r, _)| *r == rect)) {
        return Some(opaque);
    }
    let ask = ASK.get_or_init(start);
    ASKED.with_borrow_mut(|asked| {
        if asked.len() < MAX_ASKED && asked.insert((path.to_owned(), rect)) {
            let _ = ask.send((path.to_owned(), rect));
        }
    });
    None
}

/// The pixel at (x, y) of the image file at `path`: its colour, `Some(None)`
/// where `keyed` and the pixel is the colour key's (indices 254 and 255,
/// as [`opaque`] counts them) or outside the image; `None` while the file
/// is not known (asked for then). The same files and the same rule as the
/// regions' opacity: one source for what is drawn and what is hit.
pub fn pixel(path: &str, x: i32, y: i32, keyed: bool) -> Option<Option<[u8; 3]>> {
    if let Some(image) = GAME_FILES.load().and_then(|f| f.iter().find(|(p, _)| p == path).map(|(_, i)| i.clone())) {
        return Some(image.pixel(x, y, keyed));
    }
    let files = FILES.load();
    let Some(image) = files.as_ref().and_then(|f| f.iter().find(|(p, _)| p == path).map(|(_, i)| i.clone())) else {
        let _ = opaque(path, [0, 0, 0, 0]);
        return None;
    };
    Some(file_pixel(&image, x, y, keyed))
}

fn cache_file(path: &str, image: Arc<civ3_assets::pcx::Indexed>) {
    FILES.update(|old| {
        let mut shared = old.cloned().unwrap_or_default();
        shared.retain(|(p, _)| p != path);
        if shared.len() >= SHARED_FILES { shared.remove(0); }
        shared.push((path.to_owned(), image.clone()));
        Some(shared)
    });
}

/// Pixel lookup for the asynchronous decoded-file cache. Selection sources
/// use FilePixels, retained independently of either shared cache.
pub(crate) fn file_pixel(image: &civ3_assets::pcx::Indexed, x: i32, y: i32, keyed: bool) -> Option<[u8; 3]> {
    if x < 0 || y < 0 || x >= image.width as i32 || y >= image.height as i32 { return None; }
    let index = image.get(x as u32, y as u32);
    let civ3_assets::Rgb(r, g, b) = image.palette.get(index);
    (!keyed || index < 254).then_some([r, g, b])
}

/// The answering thread; its sender (a closed one if it cannot start).
fn start() -> tokio::sync::mpsc::UnboundedSender<(String, [i32; 4])> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, [i32; 4])>();
    let spawned = std::thread::Builder::new().name("civ3r-opacity".into()).spawn(move || {
        let mut files: Vec<(String, Option<Arc<civ3_assets::pcx::Indexed>>)> = Vec::new();
        // (Bounded sleeps, not a parked receive: `crate::BOUNDED_WAIT`.)
        while let Some(first) = crate::recv_bounded(&mut rx, crate::BOUNDED_WAIT) {
            let mut known = KNOWN.get().unwrap_or_default();
            let mut question = Some(first);
            // (All that is waiting, published at once.)
            while let Some((path, rect)) = question.take().or_else(|| rx.try_recv().ok()) {
                let image = match files.iter().position(|(p, _)| *p == path) {
                    Some(i) => files[i].1.clone(),
                    None => {
                        let image = civ3_assets::files::read_file(&path).ok().and_then(|bytes| civ3_assets::pcx::decode(&bytes).ok()).map(Arc::new);
                        if image.is_none() {
                            log::debug!("UI mirror: opacity of {path}: not readable as a PCX");
                        }
                        if files.len() >= KEPT_FILES {
                            files.remove(0);
                        }
                        files.push((path.clone(), image.clone()));
                        if let Some(image) = &image {
                            cache_file(&path, image.clone());
                        }
                        image
                    }
                };
                let opaque = image.is_some_and(|i| region_opaque(&i, rect));
                known.entry(path).or_default().push((rect, opaque));
            }
            KNOWN.publish(known);
        }
    });
    if let Err(e) = spawned {
        log::warn!("UI mirror: no opacity thread: {e}");
    }
    tx
}

/// Whether `rect` (x, y, w, h) lies inside `image` and has none of the
/// colour key's indices.
pub fn region_opaque(image: &civ3_assets::pcx::Indexed, [x, y, w, h]: [i32; 4]) -> bool {
    let (iw, ih) = (image.width as i32, image.height as i32);
    if x < 0 || y < 0 || w <= 0 || h <= 0 || x + w > iw || y + h > ih {
        return false;
    }
    (y..y + h).all(|row| {
        let start = (row * iw + x) as usize;
        !image.pixels[start..start + w as usize].iter().any(|&p| p >= 254)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_is_opaque_without_key_indices_inside_the_image() {
        let mut pixels = vec![7u8; 16];
        pixels[3] = 255; // (3, 0)
        pixels[12] = 254; // (0, 3)
        let image = civ3_assets::pcx::Indexed { width: 4, height: 4, pixels, palette: Default::default() };
        assert!(region_opaque(&image, [0, 0, 3, 3]));
        assert!(region_opaque(&image, [1, 1, 3, 3]));
        assert!(!region_opaque(&image, [0, 0, 4, 1]), "index 255 is transparent");
        assert!(!region_opaque(&image, [0, 1, 1, 3]), "index 254 too");
        assert!(!region_opaque(&image, [2, 2, 3, 3]), "beyond the image");
        assert!(!region_opaque(&image, [0, 0, 0, 2]), "nothing");
    }
}

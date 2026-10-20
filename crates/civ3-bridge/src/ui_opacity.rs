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

/// Per file (its path as the game opened it): regions (x, y, w, h) and
/// whether they are opaque.
type Known = HashMap<String, Vec<([i32; 4], bool)>>;

static KNOWN: civ3r_sync::Latest<Known> = civ3r_sync::Latest::new();
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
                        let image = std::fs::read(&path).ok().and_then(|bytes| civ3_assets::pcx::decode(&bytes).ok()).map(Arc::new);
                        if image.is_none() {
                            log::debug!("UI mirror: opacity of {path}: not readable as a PCX");
                        }
                        if files.len() >= KEPT_FILES {
                            files.remove(0);
                        }
                        files.push((path.clone(), image.clone()));
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

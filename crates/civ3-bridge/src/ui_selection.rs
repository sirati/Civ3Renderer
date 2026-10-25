//! Palace selection uses the sprite source known by the interface model.
//! The recorded eligible pieces are geometry, not a second rasterised map.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use civ3_ui::selection::Selection;
use crate::ui::Source;

#[derive(Clone, Debug)]
struct Map {
    selection: Selection,
    error: Option<String>,
    anchors: OnceLock<Option<Vec<(u8, [i32; 4])>>>,
    files: HashMap<String, Arc<crate::ui_opacity::GameFile>>,
}

impl Map {
    fn pixel(&self, path: &str, x: i32, y: i32, keyed: bool) -> Option<Option<[u8; 3]>> {
        Some(self.files.get(path)?.pixel(x, y, keyed))
    }
}

#[derive(Default)]
struct Sources {
    images: HashMap<u32, Arc<crate::ui_opacity::GameFile>>,
    sprites: HashMap<u32, Arc<crate::ui_opacity::GameFile>>,
}

thread_local! {
    static MAPS: RefCell<HashMap<u32, Arc<Map>>> = RefCell::new(HashMap::new());
    static SOURCES: RefCell<Sources> = RefCell::new(Sources::default());
}

/// Run after the exe has decoded a selection source. One bulk memory copy
/// at file load, not a PCX read/decode or per-pixel scan in a drawing hook.
/// A sprite and a live selection each retain the source after image clear.
pub(crate) fn loaded(image: u32, path: &str, ct: u32) {
    let key = path.replace('\\', "/").to_ascii_lowercase();
    if !key.contains("art/palaceview/") && !key.ends_with("art/oldinterface.pcx") { return; }
    let Some((w, h, row, indices)) = civ3_sys::ui_hooks::obj::image_indices(image) else { log::error!("selection source has no decoded indices: {path}"); return };
    let Some(colors) = civ3_sys::ui_hooks::obj::image_color_entries(image).or_else(|| civ3_sys::ui_hooks::color_table_entries(ct, 0, 256)) else { log::error!("selection source has no palette: {path}"); return };
    let palette = std::array::from_fn(|i| { let c = colors[i]; [c as u8, (c >> 8) as u8, (c >> 16) as u8] });
    let source = Arc::new(crate::ui_opacity::GameFile { size: (w, h), row, indices, palette });
    crate::ui_opacity::cache_game_file(path, source.clone());
    SOURCES.with_borrow_mut(|s| { s.images.insert(image, source); });
}

pub(crate) fn sliced(sprite: u32, image: u32) {
    SOURCES.with_borrow_mut(|s| {
        if let Some(source) = s.images.get(&image).cloned() { s.sprites.insert(sprite, source); }
        else { s.sprites.remove(&sprite); }
    });
}

pub(crate) fn copied(sprite: u32, from: u32) {
    SOURCES.with_borrow_mut(|s| {
        if let Some(source) = s.sprites.get(&from).cloned() { s.sprites.insert(sprite, source); }
        else { s.sprites.remove(&sprite); }
    });
}

pub(crate) fn reset(sprite: u32) { SOURCES.with_borrow_mut(|s| { s.sprites.remove(&sprite); }); }

pub(crate) fn sprite_index(sprite: u32, x: i32, y: i32) -> Option<u8> {
    SOURCES.with_borrow(|s| Some(s.sprites.get(&sprite)?.index(x, y).unwrap_or(0xFF)))
}

/// Only the startup oldinterface probe retains a sprite-derived indexed
/// answer. Keep the immutable source and placement, never a sprite pointer
/// or an evictable global drawing history.
#[derive(Clone)]
struct Presence {
    file: Arc<crate::ui_opacity::GameFile>,
    rect: [i32; 4],
    at: (i32, i32),
    clip: Option<[i32; 4]>,
}

thread_local! {
    static PRESENCE: RefCell<HashMap<u32, Presence>> = RefCell::new(HashMap::new());
}

pub(crate) fn note_presence(b: &civ3_ui::blit::GameBlit, source: impl FnOnce() -> Option<Source>) {
    let Some(file) = SOURCES.with_borrow(|s| s.sprites.get(&b.sprite).cloned()) else { return };
    let Some(Source::File { path, rect, mask: None, opacity: 255, .. }) = source() else { return };
    if !path.replace('\\', "/").to_ascii_lowercase().ends_with("art/oldinterface.pcx") { return; }
    if b.wrapper != civ3_ui::blit::Wrapper::Draw { return; }
    PRESENCE.with_borrow_mut(|p| { p.insert(b.canvas, Presence { file, rect, at: b.at, clip: b.clip }); });
}

pub(crate) fn clear_presence(image: u32) {
    PRESENCE.with_borrow_mut(|p| { p.remove(&image); });
}

pub(crate) fn presence_pixel(image: u32, x: i32, y: i32) -> Option<u32> {
    PRESENCE.with_borrow(|p| {
        let p = p.get(&image)?;
        let (sx, sy) = (x - p.at.0, y - p.at.1);
        if p.clip.is_some_and(|[l,t,r,b]| x < l || y < t || x >= r || y >= b)
            || sx < 0 || sy < 0 || sx >= p.rect[2] || sy >= p.rect[3] { return None; }
        p.file.index(p.rect[0] + sx, p.rect[1] + sy).map(u32::from)
    })
}

/// 0x57C8A0 starts each rebuild with the full 0xFF fill. With no upgrade
/// credit it does not clear, so the previous selection must survive.
pub(crate) fn fill(call: &jgl::FillCall) {
    let jgl::FillCall::Fill { target, rect, color } = *call else { return };
    if civ3_sys::ui_hooks::palace_mask_form(target.owner).is_none() { return; }
    let full = rect.is_none_or(|r| r == target.bounds) && target.clip == target.bounds;
    let error = (!full || color as u8 != 0xFF).then(|| "unexpected Palace selection clear".to_owned());
    MAPS.with_borrow_mut(|maps| { maps.insert(target.owner, Arc::new(Map { selection: Selection::new(target.size()), error, anchors: OnceLock::new(), files: HashMap::new() })); });
}

/// The game names each eligible piece and its ordinal in its +0x74 call.
pub(crate) fn piece(b: &civ3_ui::blit::GameBlit, source: Option<Source>) {
    if b.wrapper != civ3_ui::blit::Wrapper::A4 || civ3_sys::ui_hooks::palace_mask_form(b.canvas).is_none() { return; }
    MAPS.with_borrow_mut(|maps| {
        let Some(map) = maps.get_mut(&b.canvas) else { return };
        let map = Arc::make_mut(map);
        let result = (|| {
            let source = source.ok_or("Palace sprite has no interface source")?;
            if let Source::File { path, .. } = &source {
                let file = SOURCES.with_borrow(|s| s.sprites.get(&b.sprite).cloned()).ok_or("Palace sprite has no retained decoded source")?;
                map.files.insert(path.clone(), file);
            }
            map.selection.piece(&source, b.extra as u8, b.at, b.clip).map_err(str::to_owned)?;
            Ok::<_, String>(())
        })();
        if let Err(e) = result { map.error = Some(e); }
        map.anchors = OnceLock::new();
    });
}

pub(crate) fn cleared(image: u32) {
    MAPS.with_borrow_mut(|maps| { maps.remove(&image); });
    SOURCES.with_borrow_mut(|s| { s.images.remove(&image); });
}

pub(crate) fn pixel(image: u32, x: i32, y: i32) -> Option<u32> {
    let result = MAPS.with_borrow(|maps| {
        let Some(map) = maps.get(&image) else {
            return civ3_sys::ui_hooks::palace_mask_form(image).map(|_| Err("Palace selection was read before its geometry was recorded".to_owned()));
        };
        Some(match &map.error {
            Some(error) => Err(error.clone()),
            None => map.selection.hit(x, y, &|p, x, y, k| map.pixel(p, x, y, k)).map(u32::from).ok_or_else(|| "Palace selection file unavailable".to_owned()),
        })
    })?;
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            #[cfg(windows)]
            if jgl_ffi::own::on() { crate::ui_trace::replacement_failed(&error); }
            log::warn!("{error}");
            None
        }
    }
}

/// Replays the control walk from the same Present's captured fields. This
/// preserves tree order, clipping and opaque-form exclusion when Palace
/// anchors are computed on the publisher, without reading live game memory.
#[derive(Debug)]
pub(crate) struct DeferredControls {
    sites: civ3_sys::controls::Sites,
    words: HashMap<u32, Option<u32>>,
    bytes: HashMap<u32, Option<u8>>,
    blocks: HashMap<(u32, usize), Option<Vec<u8>>>,
    maps: HashMap<u32, Arc<Map>>,
    controls: OnceLock<Vec<civ3_sys::controls::Control>>,
}

impl PartialEq for DeferredControls {
    fn eq(&self, other: &Self) -> bool {
        self.sites == other.sites && self.words == other.words && self.bytes == other.bytes && self.blocks == other.blocks
            && self.maps.len() == other.maps.len() && self.maps.iter().all(|(id, map)| other.maps.get(id).is_some_and(|other| Arc::ptr_eq(map, other)))
    }
}

impl DeferredControls {
    pub fn controls(&self) -> Vec<civ3_sys::controls::Control> {
        self.controls.get_or_init(|| civ3_sys::controls::controls(self, &self.sites)).clone()
    }
}

impl civ3_sys::controls::Mem for DeferredControls {
    fn u32(&self, at: u32) -> Option<u32> { self.words.get(&at).copied().flatten() }
    fn u8(&self, at: u32) -> Option<u8> { self.bytes.get(&at).copied().flatten() }
    fn read_bytes(&self, at: u32, len: usize) -> Option<Vec<u8>> { self.blocks.get(&(at, len)).cloned().flatten() }
    fn index_regions(&self, pcx: u32) -> Option<Vec<(u8, [i32; 4])>> {
        let map = self.maps.get(&pcx)?;
        map.anchors.get_or_init(|| {
            if map.error.is_some() { return None; }
            map.selection.anchors(&|p, x, y, k| map.pixel(p, x, y, k))
        }).clone()
    }
}

/// On the game thread, retain fields already read by the control walk and
/// immutable source handles. The initial walk defers Palace cells only.
#[derive(Default)]
pub(crate) struct Memory {
    words: RefCell<HashMap<u32, Option<u32>>>,
    bytes: RefCell<HashMap<u32, Option<u8>>>,
    blocks: RefCell<HashMap<(u32, usize), Option<Vec<u8>>>>,
    maps: RefCell<HashMap<u32, Arc<Map>>>,
    recording: bool,
}

impl Memory {
    pub fn new() -> Self { Self { recording: MAPS.with_borrow(|maps| !maps.is_empty()), ..Self::default() } }

    pub fn finish(self, sites: civ3_sys::controls::Sites) -> Option<Arc<DeferredControls>> {
        let maps = self.maps.into_inner();
        if maps.is_empty() { return None; }
        Some(Arc::new(DeferredControls { sites, words: self.words.into_inner(), bytes: self.bytes.into_inner(), blocks: self.blocks.into_inner(), maps, controls: OnceLock::new() }))
    }
}

impl civ3_sys::controls::Mem for Memory {
    fn u32(&self, at: u32) -> Option<u32> {
        if self.recording {
            // Repeated reads in one capture use one value, so replay sees
            // precisely the same branches and native form placement.
            *self.words.borrow_mut().entry(at).or_insert_with(|| civ3_sys::controls::Live.u32(at))
        } else { civ3_sys::controls::Live.u32(at) }
    }
    fn u8(&self, at: u32) -> Option<u8> {
        if self.recording { *self.bytes.borrow_mut().entry(at).or_insert_with(|| civ3_sys::controls::Live.u8(at)) }
        else { civ3_sys::controls::Live.u8(at) }
    }
    fn read_bytes(&self, at: u32, len: usize) -> Option<Vec<u8>> {
        if self.recording { self.blocks.borrow_mut().entry((at, len)).or_insert_with(|| civ3_sys::controls::Live.read_bytes(at, len)).clone() }
        else { civ3_sys::controls::Live.read_bytes(at, len) }
    }
    fn index_regions(&self, pcx: u32) -> Option<Vec<(u8, [i32; 4])>> {
        MAPS.with_borrow(|maps| {
            let Some(map) = maps.get(&pcx) else {
                return civ3_sys::ui_hooks::palace_mask_form(pcx).map(|_| Vec::new());
            };
            self.maps.borrow_mut().insert(pcx, map.clone());
            Some(Vec::new())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> Arc<crate::ui_opacity::GameFile> {
        Arc::new(crate::ui_opacity::GameFile { size: (3, 2), row: 4,
            indices: vec![255, 7, 254, 91, 8, 9, 255, 92], palette: [[12, 34, 56]; 256] })
    }

    #[test]
    fn art_presence_owns_its_source_after_sprite_reset_and_image_clear() {
        let f = file();
        PRESENCE.with_borrow_mut(|p| { p.insert(99, Presence { file: f.clone(), rect: [0,0,3,2], at: (4,5), clip: Some([4,5,7,7]) }); });
        SOURCES.with_borrow_mut(|s| { s.images.insert(1,f.clone()); s.sprites.insert(2,f); });
        reset(2); cleared(1);
        assert_eq!(presence_pixel(99,5,5),Some(7));
        assert_eq!(presence_pixel(99,3,5),None);
        clear_presence(99);
        assert_eq!(presence_pixel(99,5,5),None);
    }

    #[test]
    fn first_hit_and_publisher_targets_survive_source_clear_reset_and_later_rebuild() {
        let file = file();
        SOURCES.with_borrow_mut(|s| { s.images.insert(10, file.clone()); });
        sliced(20, 10);
        copied(21, 20);
        cleared(10);
        reset(20);
        assert_eq!(sprite_index(21, 1, 0), Some(7));
        assert_eq!(sprite_index(21, -1, 0), Some(0xFF));
        let source = Source::File { path: "not on disk".into(), rect: [0, 0, 3, 2], mask: None, palette: None, opacity: 255 };
        let mut map = Arc::new(Map { selection: Selection::new((20, 20)), error: None, anchors: OnceLock::new(), files: HashMap::from([("not on disk".into(), file)]) });
        Arc::make_mut(&mut map).selection.piece(&source, 3, (2, 3), None).unwrap();
        reset(21);
        // No opacity worker, filename, cache, or source object remains.
        assert_eq!(map.selection.hit(3, 3, &|p, x, y, k| map.pixel(p, x, y, k)), Some(3));
        assert_eq!(map.selection.hit(2, 3, &|p, x, y, k| map.pixel(p, x, y, k)), Some(0xFF));
        assert!(map.anchors.get().is_none(), "capture and first hit never scan for anchors");
        let snapshot = map.clone();
        Arc::make_mut(&mut map).selection = Selection::new((20, 20));
        assert!(!Arc::ptr_eq(&snapshot, &map));
        // A publication owns both its source bytes and its old eligibility.
        let anchors = std::thread::spawn(move || snapshot.selection.anchors(&|p, x, y, k| snapshot.pixel(p, x, y, k))).join().unwrap().unwrap();
        assert_eq!(anchors.len(), 1);
        let (index, [x, y, w, h]) = anchors[0];
        assert_eq!(index, 3);
        assert_eq!((w, h), (1, 1));
        assert!((x == 3 && y == 3) || ((x == 2 || x == 3) && y == 4));
    }

    #[test]
    fn decoded_source_stride_key_indices_and_palette_match_the_shared_pixel_query() {
        let source = file();
        crate::ui_opacity::cache_game_file("selection test", source.clone());
        for y in -1..3 {
            for x in -1..5 {
                for keyed in [false, true] {
                    assert_eq!(crate::ui_opacity::pixel("selection test", x, y, keyed), Some(source.pixel(x, y, keyed)));
                }
            }
        }
        assert_eq!(source.pixel(1, 0, true), Some([12, 34, 56]));
        assert_eq!(source.pixel(2, 0, true), None);
        assert_eq!(source.index(0, 1), Some(8), "padding is not a file pixel");
    }

    #[test]
    fn publisher_replays_control_order_placement_and_opaque_root_exclusion_from_captured_fields() {
        use civ3_sys::controls::{Sites, screens};
        let sites = Sites { roots: 0x100, root_count: 0x200, modal: 0x300, companion: 0x304, main_gui: 0x900000, advisor: 0xA00000 };
        let form = 0x10000;
        let mut map = Map { selection: Selection::new((20, 20)), error: None, anchors: OnceLock::new(), files: HashMap::from([("art".into(), file())]) };
        let source = Source::File { path: "art".into(), rect: [0, 0, 3, 2], mask: None, palette: None, opacity: 255 };
        map.selection.piece(&source, 3, (2, 3), None).unwrap();
        let mut capture = DeferredControls { sites, words: HashMap::new(), bytes: HashMap::new(), blocks: HashMap::new(), maps: HashMap::from([(form + 0x3154, Arc::new(map))]), controls: OnceLock::new() };
        let put = |words: &mut HashMap<_, _>, at, value| { words.insert(at, Some(value)); };
        for (at, value) in [(sites.modal, 0), (sites.root_count, 1), (sites.roots, form), (0x9C72F8, 20), (0x9C72FC, 20), (form + 0x312C, 20), (form + 0x3130, 20)] { put(&mut capture.words, at, value); }
        for (addr, vtable) in [(form, screens::PALACE), (0x20000, 0x671218)] {
            for (offset, value) in [(0, vtable), (0x9C, 0), (0xA0, 1), (0x130, 0), (0x224, 0), (0x22C, 0), (0x380, 0)] { put(&mut capture.words, addr + offset, value); }
            for (offset, rect) in [(0x1AC, [10, 20, 30, 40]), (0x1BC, [0, 0, 0, 0]), (0x1DC, [0, 0, 1024, 768])] {
                for (i, value) in rect.into_iter().enumerate() { put(&mut capture.words, addr + offset + 4 * i as u32, value); }
            }
        }
        let controls = capture.controls();
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0].id, civ3_sys::controls::advisors::cell_id(form, screens::CELL_MASK, 3));
        assert!((12..=13).contains(&controls[0].rect[0]) && (23..=24).contains(&controls[0].rect[1]));
        // A different packet puts an opaque root above Palace. Replaying
        // the complete captured walk must hide that anchor too.
        capture.controls = OnceLock::new();
        put(&mut capture.words, sites.root_count, 2);
        put(&mut capture.words, sites.roots, 0x20000);
        put(&mut capture.words, sites.roots + 4, form);
        let controls = std::thread::spawn(move || capture.controls()).join().unwrap();
        assert!(controls.is_empty());
    }
}

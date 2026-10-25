//! Palace selection uses the sprite source known by the interface model.
//! The recorded eligible pieces are geometry, not a second rasterised map.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use civ3_ui::selection::Selection;
use crate::ui::Source;

struct Map {
    selection: Selection,
    error: Option<String>,
    anchors: Option<Vec<(u8, [i32; 4])>>,
    files: HashMap<String, Arc<civ3_assets::pcx::Indexed>>,
}

impl Map {
    fn pixel(&self, path: &str, x: i32, y: i32, keyed: bool) -> Option<Option<[u8; 3]>> {
        Some(crate::ui_opacity::file_pixel(self.files.get(path)?, x, y, keyed))
    }
}

thread_local! { static MAPS: RefCell<HashMap<u32, Map>> = RefCell::new(HashMap::new()); }

/// 0x57C8A0 starts each rebuild with the full 0xFF fill. With no upgrade
/// credit it does not clear, so the previous selection must survive.
pub(crate) fn fill(call: &jgl::FillCall) {
    let jgl::FillCall::Fill { target, rect, color } = *call else { return };
    if civ3_sys::ui_hooks::palace_mask_form(target.owner).is_none() { return; }
    let full = rect.is_none_or(|r| r == target.bounds) && target.clip == target.bounds;
    let error = (!full || color as u8 != 0xFF).then(|| "unexpected Palace selection clear".to_owned());
    MAPS.with_borrow_mut(|maps| { maps.insert(target.owner, Map { selection: Selection::new(target.size()), error, anchors: None, files: HashMap::new() }); });
}

/// The game names each eligible piece and its ordinal in its +0x74 call.
pub(crate) fn piece(b: &civ3_ui::blit::GameBlit, source: Option<Source>) {
    if b.wrapper != civ3_ui::blit::Wrapper::A4 || civ3_sys::ui_hooks::palace_mask_form(b.canvas).is_none() { return; }
    MAPS.with_borrow_mut(|maps| {
        let Some(map) = maps.get_mut(&b.canvas) else { return };
        let result = (|| {
            let source = source.ok_or("Palace sprite has no interface source")?;
            if let Source::File { path, .. } = &source { map.files.insert(path.clone(), crate::ui_opacity::require_file(path)?); }
            map.selection.piece(&source, b.extra as u8, b.at, b.clip).map_err(str::to_owned)?;
            Ok::<_, String>(())
        })();
        if let Err(e) = result { map.error = Some(e); }
        map.anchors = None;
    });
}

pub(crate) fn cleared(image: u32) {
    MAPS.with_borrow_mut(|maps| { maps.remove(&image); });
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
            if jgl_ffi::own::on() { crate::ui_trace::replacement_failed(&error); }
            log::warn!("{error}");
            None
        }
    }
}

/// On the game thread, while NativeUi captures the frame's targets.
pub(crate) struct Memory;

impl civ3_sys::controls::Mem for Memory {
    fn u32(&self, at: u32) -> Option<u32> { civ3_sys::controls::Live.u32(at) }
    fn u8(&self, at: u32) -> Option<u8> { civ3_sys::controls::Live.u8(at) }
    fn read_bytes(&self, at: u32, len: usize) -> Option<Vec<u8>> { civ3_sys::controls::Live.read_bytes(at, len) }
    fn index_regions(&self, pcx: u32) -> Option<Vec<(u8, [i32; 4])>> {
        MAPS.with_borrow_mut(|maps| {
            let Some(map) = maps.get_mut(&pcx) else {
                return civ3_sys::ui_hooks::palace_mask_form(pcx).map(|_| Vec::new());
            };
            if map.error.is_some() { return Some(Vec::new()); }
            if map.anchors.is_none() { map.anchors = map.selection.anchors(&|p, x, y, k| map.pixel(p, x, y, k)); }
            map.anchors.clone()
        })
    }
}

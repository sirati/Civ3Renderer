//! The short, localized notices above map tiles, such as an exhausted unit's
//! message. The animator's separate map-overlay image is not an ordinary UI
//! form. Read the semantic records so independent views show them too.

use inproc::Ptr32;
use crate::{Build, Civ3, Civ3Error};

pub const FIRST: u32 = 0x590;
pub const STRIDE: usize = 0x170;
pub const COUNT: usize = 50;

inproc::fastcall_hook! {
    /// `MapMessage::draw(this, PCX_Image *canvas, PCX_Image *background)`.
    pub mod draw(message, edx; canvas, background) -> ();
}

/// PCGames 1.22's complete position-independent entry instructions.
pub fn draw_site(build: Build) -> Option<crate::ui_hooks::Site> {
    match build {
        Build::PcGames => Some((0x4D_7E00, &[0x53, 0x8B, 0x5C, 0x24, 0x08])),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub slot: usize,
    pub tile: (i32, i32),
    pub text: String,
    pub font: u32,
    pub color: u32,
    pub shadow: u32,
    /// A second overlapping message is displaced down by half a tile.
    pub below: bool,
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// The lifetime flag at +0x26 stays set until animator removal. The byte
/// at +4 is only the input pause and clears before the notice disappears.
pub fn decode(slot: usize, bytes: &[u8; STRIDE]) -> Option<Message> {
    if bytes[0x26] == 0 { return None; }
    let text = &bytes[0x68..0x168];
    let end = text.iter().position(|&c| c == 0)?;
    if end == 0 { return None; }
    Some(Message {
        slot,
        tile: (word(bytes, 0x18) as i32, word(bytes, 0x1C) as i32),
        text: text[..end].iter().map(|&c| gdi_text::windows_1252_char(char::from(c))).collect(),
        font: word(bytes, 8),
        color: word(bytes, 0x14),
        shadow: word(bytes, 0x10),
        below: bytes[0x24] != 0,
    })
}

/// A JGL font's GDI handle and the game's own vertical text metrics.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Font {
    pub handle: u32,
    pub baseline: i32,
    pub box_height: i32,
}

pub fn font(at: u32) -> Option<Font> {
    if at == 0 { return None; }
    let p = Ptr32::<u32>::new(at);
    let jgl = p.offset::<u32>(4).ok()?.read().ok()?;
    let handle = Ptr32::<u32>::new(jgl).offset::<u32>(0x18).ok()?.read().ok()?;
    let ascender = p.offset::<i32>(8).ok()?.read().ok()?;
    let baseline = p.offset::<i32>(0x10).ok()?.read().ok()?;
    let box_height = if ascender >= 0 { baseline.checked_add(ascender)? } else { p.offset::<i32>(0xC).ok()?.read().ok()? };
    (handle != 0 && (1..=1024).contains(&baseline) && (1..=1024).contains(&box_height))
        .then_some(Font { handle, baseline, box_height })
}

impl Civ3 {
    pub fn map_messages(&self) -> Result<Vec<Message>, Civ3Error> {
        let base = self.addrs.main_screen_form.checked_add(FIRST).ok_or(Civ3Error::NoGame)?;
        let records = Ptr32::<[u8; STRIDE]>::new(base).read_array(COUNT)?;
        Ok(records.iter().enumerate().filter_map(|(i, b)| decode(i, b)).collect())
    }

    /// The global shade colour consumed by MapMessage::draw, shared with
    /// the rest of the game's interface theme.
    pub fn map_message_background(&self) -> Result<u32, Civ3Error> {
        Ok(Ptr32::<u32>::new(self.addrs.bic_data + 0x3E3C).read()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record() -> [u8; STRIDE] {
        let mut b = [0; STRIDE];
        b[0x26] = 1;
        b[0x18..0x1C].copy_from_slice(&42u32.to_le_bytes());
        b[0x1C..0x20].copy_from_slice(&24u32.to_le_bytes());
        b[0x68..0x7A].copy_from_slice(b"Unit already moved");
        b
    }
    #[test]
    fn notice_outlives_input_pause_and_is_not_limited_to_camera_visibility() {
        let b = record();
        assert_eq!(b[4], 0);
        assert_eq!(b[0x25], 0, "the engine can have an active notice outside A");
        let m = decode(7, &b).unwrap();
        assert_eq!((m.slot, m.tile, m.text.as_str()), (7, (42, 24), "Unit already moved"));
    }
    #[test]
    fn removal_and_slot_reuse_never_keep_old_text() {
        let mut b = record();
        b[0x26] = 0;
        assert!(decode(0, &b).is_none());
        b[0x26] = 1;
        b[0x68] = 0;
        assert!(decode(0, &b).is_none());
        b[0x68..0x6B].copy_from_slice(&[0xC9, 0x92, 0]);
        assert_eq!(decode(0, &b).unwrap().text, "É’");
    }
    #[test]
    fn localized_text_decodes_accents_punctuation_and_currency() {
        let mut b = record();
        let localized = b"D\xE9j\xE0 \x93d\xE9plac\xE9\x94 \x96 l\x92unit\xE9 \x80\0";
        b[0x68..0x68 + localized.len()].copy_from_slice(localized);
        assert_eq!(decode(0, &b).unwrap().text, "Déjà “déplacé” – l’unité €");
    }

    #[test]
    fn truncated_record_text_is_not_rendered() {
        let mut b = record();
        b[0x68..0x168].fill(b'x');
        assert!(decode(0, &b).is_none());
    }
}

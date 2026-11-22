//! The two animation files of each leader head, so that both are made
//! while it shows either.
//!
//! A leader head object loads its era's forward file at +0x2BC and its
//! reverse file at +0x384 (`Flic_Anim_Info`s, 0xC8 apart; init 0x4C8240,
//! `docs/civ3-map-rendering.md` §16.13), the forward one first. It plays
//! one sweep forward, the next backward from the other file. Only the
//! file shown was requested, so each change of direction asked for the
//! other file then, and the head was black until it was made.

use std::collections::HashMap;
use std::sync::Mutex;

/// The reverse file's `Flic_Anim_Info` past the forward one's.
const REVERSE: u32 = 0x384 - 0x2BC;

/// At most this many files remembered (heads of every civ and era).
const MAX: usize = 512;

#[derive(Default)]
struct Pairs {
    /// The last file loaded, and where.
    last: Option<(u32, String)>,
    /// Each head file's other one, both ways.
    partner: HashMap<String, String>,
}

static PAIRS: Mutex<Option<Pairs>> = Mutex::new(None);

/// The folder of `path` (either separator, any case).
fn folder(path: &str) -> String {
    path.rsplit_once(['/', '\\']).map_or("", |(dir, _)| dir).replace('\\', "/").to_ascii_lowercase()
}

/// The game loaded animation file `path` into `flic`: a head's reverse
/// file pairs with its forward file, loaded just before it into the same
/// head from the same folder.
pub(crate) fn loaded(flic: u32, path: &str) {
    let mut guard = PAIRS.lock().unwrap_or_else(|e| e.into_inner());
    let pairs = guard.get_or_insert_with(Pairs::default);
    if pairs.partner.len() >= MAX {
        pairs.partner.clear();
    }
    if let Some((before, forward)) = pairs.last.take()
        && before.checked_add(REVERSE) == Some(flic)
        && forward != path
        && folder(&forward) == folder(path)
    {
        pairs.partner.insert(forward.clone(), path.to_owned());
        pairs.partner.insert(path.to_owned(), forward);
    }
    pairs.last = Some((flic, path.to_owned()));
}

/// `heads` (files as `name` gives them) with the other file of each.
pub(crate) fn with_partners(heads: Vec<String>, name: impl Fn(&str) -> String) -> Vec<String> {
    let guard = PAIRS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(pairs) = guard.as_ref() else { return heads };
    let mut out = heads.clone();
    for (file, other) in &pairs.partner {
        if heads.contains(&name(file)) {
            out.push(name(other));
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shown_head_brings_its_other_file_and_nothing_else() {
        let head = 0x0A35_0000 + 0x2BC;
        // Another object's file just below, then the head: forward, reverse.
        loaded(head - REVERSE, "Art/Units/Warrior/WarriorRun.flc");
        loaded(head, "Art/Flics/Al_01.FLC");
        loaded(head + REVERSE, "Art/Flics/Al_02.FLC");
        let lower = |p: &str| p.to_ascii_lowercase();
        assert_eq!(with_partners(vec!["art/flics/al_01.flc".into()], lower), ["art/flics/al_01.flc", "art/flics/al_02.flc"]);
        assert_eq!(with_partners(vec!["art/flics/al_02.flc".into()], lower), ["art/flics/al_01.flc", "art/flics/al_02.flc"]);
        // The warrior is no head's: alone, and paired with nothing.
        assert_eq!(with_partners(vec!["art/units/warrior/warriorrun.flc".into()], lower), ["art/units/warrior/warriorrun.flc"]);
        // The next leader in the same head: its own pair.
        loaded(head, "Art/Flics/Ab_a01.flc");
        loaded(head + REVERSE, "Art/Flics/Ab_a02.flc");
        assert_eq!(with_partners(vec!["art/flics/ab_a02.flc".into()], lower), ["art/flics/ab_a01.flc", "art/flics/ab_a02.flc"]);
    }
}

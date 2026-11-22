//! The two animation files of each leader head, so that both are made
//! while it shows either.
//!
//! A leader head object loads its era's forward file at +0x2BC and its
//! reverse file at +0x384 (`Flic_Anim_Info`s, 0xC8 apart; init 0x4C8240,
//! `docs/civ3-map-rendering.md` §16.13), the forward one first. It plays
//! one sweep forward, the next backward from the other file. Only the
//! file shown was requested, so each change of direction asked for the
//! other file then, and the head was black until it was made.

use std::cell::RefCell;
use std::collections::HashMap;

/// The reverse file's `Flic_Anim_Info` past the forward one's.
const REVERSE: u32 = 0x384 - 0x2BC;

/// At most this many files remembered (heads of every civ and era).
const MAX: usize = 512;

thread_local! {
    /// The last file the game's thread loaded, and where.
    static LAST: RefCell<Option<(u32, String)>> = const { RefCell::new(None) };
}

/// Each head file's other one, both ways: from the game's thread to the
/// publisher's.
static PARTNERS: handoff::Latest<HashMap<String, String>> = handoff::Latest::new();

/// The folder of `path` (either separator, any case).
fn folder(path: &str) -> String {
    path.rsplit_once(['/', '\\']).map_or("", |(dir, _)| dir).replace('\\', "/").to_ascii_lowercase()
}

/// The game loaded animation file `path` into `flic` (on its thread): a
/// head's reverse file pairs with its forward file, loaded just before it
/// into the same head from the same folder.
pub(crate) fn loaded(flic: u32, path: &str) {
    let before = LAST.with_borrow_mut(|last| last.replace((flic, path.to_owned())));
    let Some((at, forward)) = before else { return };
    if at.checked_add(REVERSE) != Some(flic) || forward == path || folder(&forward) != folder(path) {
        return;
    }
    PARTNERS.update(|old| {
        let mut partners = old.filter(|o| o.len() < MAX).cloned().unwrap_or_default();
        partners.insert(forward.clone(), path.to_owned());
        partners.insert(path.to_owned(), forward.clone());
        Some(partners)
    });
}

/// `heads` (files as `name` gives them) with the other file of each.
pub(crate) fn with_partners(heads: Vec<String>, name: impl Fn(&str) -> String) -> Vec<String> {
    let Some(partners) = PARTNERS.load() else { return heads };
    let mut out = heads.clone();
    for (file, other) in partners.iter() {
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

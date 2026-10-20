//! Read only the native dialog's staged values. The native Accept callback
//! and Escape still own application and rollback; this snapshot only decides
//! whether our discard confirmation is needed.

use civ3_sys::{controls::Mem, ui_hooks::PreferencesForm};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Settings {
    checkboxes: [u32; 33],
    choice: u32,
    audio: [[u32; 2]; 3],
}

/// PCGames Parameters_Form 0x588040 applies its 33 checkbox values and
/// choice only on Accept. The three staged doubles also support audio
/// preview; Escape restores them through the native sound callbacks.
/// CheckBox uses the same data-offset table lookup as 0x58809B..0x5880A7.
/// These offsets are verified against the original executable, not writes.
pub(super) fn read(mem: &dyn Mem, form: PreferencesForm) -> Option<Settings> {
    let at = |offset| form.form.checked_add(offset);
    if mem.u32(form.form)? != form.vtable { return None; }
    let mut checkboxes = [0; 33];
    for (index, value) in checkboxes.iter_mut().enumerate() {
        let object = at(0x1348 + index as u32 * 0x694)?;
        let table = mem.u32(object)?;
        let data = mem.u32(table.checked_add(8)?)?;
        if data > 0x694 - 0xF4 { return None; }
        *value = mem.u32(object.checked_add(data)?.checked_add(0xF0)?)?;
    }
    let choice = mem.u32(at(0x15C88)?)?;
    let mut audio = [[0; 2]; 3];
    for (index, value) in audio.iter_mut().enumerate() {
        let value_at = at(0x15C98 + index as u32 * 8)?;
        *value = [mem.u32(value_at)?, mem.u32(value_at.checked_add(4)?)?];
    }
    Some(Settings { checkboxes, choice, audio })
}

impl Settings {
    pub(super) fn changes(&self, draft: &Self) -> Vec<String> {
        let mut changes = Vec::new();
        let count = self.checkboxes.iter().zip(draft.checkboxes).filter(|(before, after)| **before != *after).count();
        if count != 0 { changes.push(format!("Civ III options: {count} changed")); }
        if self.choice != draft.choice { changes.push("Civ III dropdown choice changed".into()); }
        for (index, name) in ["All sound", "Sound effects", "Music"].into_iter().enumerate() {
            if self.audio[index] != draft.audio[index] { changes.push(format!("{name} volume changed")); }
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fake(BTreeMap<u32, u32>);
    impl Mem for Fake {
        fn u32(&self, at: u32) -> Option<u32> { self.0.get(&at).copied() }
        fn u8(&self, at: u32) -> Option<u8> { self.u32(at).map(|value| value as u8) }
    }

    fn fixture() -> (Fake, PreferencesForm) {
        let form = PreferencesForm { form: 0x10_0000, vtable: 0x66_E628, background: "test" };
        let mut mem = Fake(BTreeMap::new());
        mem.0.insert(form.form, form.vtable);
        mem.0.insert(0x8008, 0x598);
        for index in 0..33 {
            let object = form.form + 0x1348 + index * 0x694;
            mem.0.insert(object, 0x8000);
            mem.0.insert(object + 0x688, index % 2);
        }
        mem.0.insert(form.form + 0x15C88, 2);
        for offset in (0x15C98..0x15CB0).step_by(4) { mem.0.insert(form.form + offset, 0); }
        (mem, form)
    }

    #[test]
    fn native_snapshot_ignores_hover_and_reports_staged_checkbox_choice_and_audio_changes() {
        let (mut mem, form) = fixture();
        let original = read(&mem, form).unwrap();
        mem.0.insert(form.form + 0x1348 + 0x68C, 42); // hovered box
        assert_eq!(read(&mem, form).unwrap(), original);
        mem.0.insert(form.form + 0x1348 + 0x688, 1);
        mem.0.insert(form.form + 0x15C88, 3);
        mem.0.insert(form.form + 0x15CA0, 99);
        assert_eq!(original.changes(&read(&mem, form).unwrap()), [
            "Civ III options: 1 changed", "Civ III dropdown choice changed", "Sound effects volume changed",
        ]);
    }

    #[test]
    fn native_snapshot_rejects_wrong_class_unreadable_fields_and_bad_data_offsets() {
        let (mut mem, form) = fixture();
        mem.0.insert(form.form, 0);
        assert!(read(&mem, form).is_none());
        mem.0.insert(form.form, form.vtable);
        mem.0.insert(0x8008, u32::MAX);
        assert!(read(&mem, form).is_none());
        mem.0.insert(0x8008, 0x598);
        mem.0.remove(&(form.form + 0x15CA0));
        assert!(read(&mem, form).is_none());
    }
}

#[cfg(test)]
pub(super) fn test_values(changed: bool) -> Settings {
    let mut checkboxes = [0; 33];
    checkboxes[0] = u32::from(changed);
    Settings { checkboxes, choice: 0, audio: [[0; 2]; 3] }
}

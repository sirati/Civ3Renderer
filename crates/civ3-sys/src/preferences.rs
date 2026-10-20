//! Preferences-only audio transactions. The original controls still own
//! mouse/keyboard input, drawing and checked state. Their three notification
//! callbacks stage values; the native Accept path applies audio before it
//! saves options or destroys controls. Cancel leaves the sound engine alone.

use std::cell::Cell;
use inproc::{Ptr32, hook::HookError};
use crate::Build;

const FORM: u32 = 0xC7_2888;
const VTABLE: u32 = 0x66_E628;
const EFFECTS: u32 = 0x588AD0;
const MUSIC: u32 = 0x588C20;
const AUDIO: u32 = 0x588DB0;
const CLOSE: u32 = 0x588040;
const CHECKED: u32 = 0x61E290;
const VISIBLE: u32 = 0x609AC0;
const VISIBLE_PROLOGUE: &[u8] = &[0xF6, 0x81, 0xA0, 0, 0, 0, 1];
const CHECK_PROLOGUE: &[u8] = &[0x8B, 0x01, 0x8B, 0x50, 0x08];
const ENABLE_PROLOGUE: &[u8] = &[0x53, 0x8A, 0x5C, 0x24, 0x0C];
const AUDIO_PROLOGUE: &[u8] = &[0x8B, 0x44, 0x24, 0x04, 0x83, 0xEC, 0x08];
const CLOSE_PROLOGUE: &[u8] = &[0x51, 0x53, 0x56, 0x8B, 0xF1];
/// Parameters_Form button callback (vtable +0xD4). Id 0, the round Accept,
/// sets the result byte +0xEC61 to 1 and then closes through vtable +8
/// (0x588040), which commits. Native Return does not: its close runs with
/// the byte still 0, and 0x587EA0 sets it only after the form is hidden.
const BUTTON: u32 = 0x587E70;
const BUTTON_PROLOGUE: &[u8] = &[0x8B, 0x44, 0x24, 0x04, 0x83, 0xF8, 0xFF];
const ACCEPT_BUTTON_ID: u32 = 0;
// Every direct sound-engine writer in native Cancel, before it restores
// checkbox visuals through the staged callbacks. Each has a separate ABI.
const CANCEL_EFFECTS_VOLUME: (u32, [u8; 5], u32) = (0x58820A, [0xE8, 0x21, 0x10, 0x07, 0x00], 0x5F9230);
const CANCEL_MUSIC_VOLUME: (u32, [u8; 5], u32) = (0x588246, [0xE8, 0xE5, 0x0F, 0x07, 0x00], 0x5F9230);
const CANCEL_EFFECTS_ENABLE: (u32, [u8; 5], u32) = (0x588270, [0xE8, 0x3B, 0x10, 0x07, 0x00], 0x5F92B0);
const CANCEL_EFFECTS_DISABLE: (u32, [u8; 5], u32) = (0x588296, [0xE8, 0x15, 0x11, 0x07, 0x00], 0x5F93B0);

inproc::fastcall_hook! { mod effects(form, edx; enabled, update_master) -> (); }
inproc::fastcall_hook! { mod music(form, edx; enabled, update_master) -> (); }
inproc::fastcall_hook! { mod audio(form, edx; which, value) -> (); }
inproc::fastcall_hook! { mod close(form, edx;) -> (); }
// Native Cancel also bypasses the form callbacks for these sound writes.
inproc::fastcall_hook! { mod cancel_effects(sound, edx; channel, value) -> (); }
inproc::fastcall_hook! { mod cancel_music(sound, edx; channel, value) -> (); }
inproc::fastcall_hook! { mod cancel_enable(sound, edx; channel) -> (); }
inproc::fastcall_hook! { mod cancel_disable(sound, edx; channel) -> (); }
inproc::thiscall_fn! { fn set_checked(checkbox; checked) -> (); }
inproc::thiscall_fn! { fn visible(form;) -> u32; }
inproc::thiscall_fn! { fn apply_effects(form; enabled, update_master) -> (); }
inproc::thiscall_fn! { fn apply_music(form; enabled, update_master) -> (); }
inproc::thiscall_fn! { fn apply_audio(form; which, value) -> (); }
inproc::thiscall_fn! { fn button(form; id) -> (); }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase { #[default] Edit, Commit, Cancel }
thread_local! { static PHASE: Cell<Phase> = const { Cell::new(Phase::Edit) }; }

fn phase<T>(next: Phase, f: impl FnOnce() -> T) -> T {
    struct Restore(Phase);
    impl Drop for Restore { fn drop(&mut self) { PHASE.set(self.0); } }
    let _restore = Restore(PHASE.replace(next));
    f()
}

/// A discarded draft never changed active audio, so the original rollback
/// must not stop playing nodes, clear channel flags, or reset volume either.
fn cancel_native_audio(write: impl FnOnce()) {
    if PHASE.get() != Phase::Cancel { write(); }
}

fn real_form(form: u32) -> bool { form == FORM && Ptr32::<u32>::new(form).read().ok() == Some(VTABLE) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Apply { Effects(bool), Music(bool), Volumes(u32) }

fn plan(accepted: bool, was_enabled: [bool; 2], enabled: [bool; 2], was_volume: [u32; 3], volume: [u32; 3]) -> Vec<Apply> {
    if !accepted { return Vec::new(); }
    let mut out = Vec::new();
    if enabled[0] != was_enabled[0] { out.push(Apply::Effects(enabled[0])); }
    if enabled[1] != was_enabled[1] { out.push(Apply::Music(enabled[1])); }
    if volume != was_volume { out.push(Apply::Volumes(volume[0])); }
    out
}

fn finish(actions: Vec<Apply>, mut apply: impl FnMut(Apply), close: impl FnOnce()) {
    for action in actions { apply(action); }
    close();
}

fn commit_plan(form: u32, accepted: bool) -> Option<Vec<Apply>> {
    // Cancel needs no staged values. Its rollback guards must stay active
    // even if a draft field is unreadable or contains a malformed value.
    if !accepted { return Some(Vec::new()); }
    let byte = |offset| Ptr32::<u8>::new(form + offset).read().ok().map(|n| n != 0);
    let was_enabled = [byte(0xEC5F)?, byte(0xEC60)?];
    let enabled = [byte(0xEC5D)?, byte(0xEC5E)?];
    let mut was_volume = [0; 3];
    let mut volume = [0; 3];
    for (index, offset) in [0x15C84, 0x15C7C, 0x15C80].into_iter().enumerate() {
        was_volume[index] = Ptr32::<u32>::new(form + offset).read().ok()?;
        let staged = Ptr32::<f64>::new(form + 0x15C98 + index as u32 * 8).read().ok()?;
        if !staged.is_finite() || !(0.0..=127.0).contains(&staged) { return None; }
        volume[index] = staged.round() as u32;
    }
    Some(plan(accepted, was_enabled, enabled, was_volume, volume))
}

/// Only staged scalar values change here. The Scroll_Bar's existing
/// notification code redraws its thumb after this callback returns.
fn stage_audio(form: u32, which: u32, value: u32) -> Result<(), inproc::MemError> {
    if which == 3 { return Ptr32::<u32>::new(form + 0x15C88).write(&value); }
    let value = value.min(127) as f64;
    match which {
        0 => Ptr32::<f64>::new(form + 0x15CA0).write(&value),
        1 => Ptr32::<f64>::new(form + 0x15CA8).write(&value),
        2 => {
            let maximum = Ptr32::<u32>::new(form + 0x14E40).read()?;
            if maximum == 0 { return Ok(()); }
            Ptr32::<f64>::new(form + 0x15C98).write(&value)?;
            Ptr32::<f64>::new(form + 0x15C90).write(&(value / maximum as f64))
        }
        _ => Ok(()),
    }
}

fn stage_enabled(form: u32, enabled: bool, update_master: bool, effects: bool, checked: set_checked::Function) -> Result<(), inproc::MemError> {
    let (checkbox, own, other) = if effects { (0xE5C8, 0xEC5D, 0xEC5E) } else { (0xDF34, 0xEC5E, 0xEC5D) };
    checked.call(form + checkbox, u32::from(enabled));
    Ptr32::<u8>::new(form + own).write(&u8::from(enabled))?;
    if update_master {
        let master = enabled && Ptr32::<u8>::new(form + other).read()? != 0;
        checked.call(form + 0xD8A0, u32::from(master));
        Ptr32::<u8>::new(form + 0xEC5C).write(&u8::from(master))?;
    }
    Ok(())
}

pub struct Installed {
    _effects: effects::Installed,
    _music: music::Installed,
    _audio: audio::Installed,
    _close: close::Installed,
    _cancel_effects: cancel_effects::Installed,
    _cancel_music: cancel_music::Installed,
    _cancel_enable: cancel_enable::Installed,
    _cancel_disable: cancel_disable::Installed,
}

/// Install once, before Preferences can open. All patches are byte checked
/// and rollback together if any contract is unavailable. No global sound
/// method or sound shutdown path is patched.
pub fn install(build: Build) -> Result<Installed, HookError> {
    if build != Build::PcGames { return Err(HookError::Unsupported); }
    let checked = set_checked::Function::at(CHECKED, CHECK_PROLOGUE)?;
    let visible = visible::Function::at(VISIBLE, VISIBLE_PROLOGUE)?;
    let effects_fn = apply_effects::Function::at(EFFECTS, ENABLE_PROLOGUE)?;
    let music_fn = apply_music::Function::at(MUSIC, ENABLE_PROLOGUE)?;
    let audio_fn = apply_audio::Function::at(AUDIO, AUDIO_PROLOGUE)?;
    let effects = effects::install_inline(EFFECTS, ENABLE_PROLOGUE, move |call| {
        if !real_form(call.form) || PHASE.get() == Phase::Commit { return call.original(); }
        if let Err(error) = stage_enabled(call.form, call.enabled != 0, call.update_master != 0, true, checked) { log::error!("staging Preferences sound enable: {error}"); }
    })?;
    let music = music::install_inline(MUSIC, ENABLE_PROLOGUE, move |call| {
        if !real_form(call.form) || PHASE.get() == Phase::Commit { return call.original(); }
        if let Err(error) = stage_enabled(call.form, call.enabled != 0, call.update_master != 0, false, checked) { log::error!("staging Preferences music enable: {error}"); }
    })?;
    let audio = audio::install_inline(AUDIO, AUDIO_PROLOGUE, |call| {
        if !real_form(call.form) || PHASE.get() == Phase::Commit { return call.original(); }
        if let Err(error) = stage_audio(call.form, call.which, call.value) { log::error!("staging Preferences audio value: {error}"); }
    })?;
    let cancel_effects = cancel_effects::install_call_site(CANCEL_EFFECTS_VOLUME.0, &CANCEL_EFFECTS_VOLUME.1, |call| cancel_native_audio(|| call.original()))?;
    let cancel_music = cancel_music::install_call_site(CANCEL_MUSIC_VOLUME.0, &CANCEL_MUSIC_VOLUME.1, |call| cancel_native_audio(|| call.original()))?;
    let cancel_enable = cancel_enable::install_call_site(CANCEL_EFFECTS_ENABLE.0, &CANCEL_EFFECTS_ENABLE.1, |call| cancel_native_audio(|| call.original()))?;
    let cancel_disable = cancel_disable::install_call_site(CANCEL_EFFECTS_DISABLE.0, &CANCEL_EFFECTS_DISABLE.1, |call| cancel_native_audio(|| call.original()))?;
    let close = close::install_inline(CLOSE, CLOSE_PROLOGUE, move |call| {
        // The same native visibility gate as the original close routine.
        // Static destruction or a duplicate close must never apply audio.
        if !real_form(call.form) || visible.call(call.form) == 0 { return call.original(); }
        let accepted = Ptr32::<u8>::new(call.form + 0xEC61).read().ok() == Some(1);
        let Some(actions) = commit_plan(call.form, accepted) else {
            log::error!("Preferences audio draft was unreadable; retaining native closure");
            return call.original();
        };
        log::debug!("Preferences native {}: audio callbacks {:?} before original closure", if accepted { "Accept" } else { "Cancel" }, actions);
        phase(if accepted { Phase::Commit } else { Phase::Cancel }, || {
            finish(actions, |action| match action {
                Apply::Effects(enabled) => effects_fn.call(call.form, u32::from(enabled), 0),
                Apply::Music(enabled) => music_fn.call(call.form, u32::from(enabled), 0),
                Apply::Volumes(master) => audio_fn.call(call.form, 2, master),
            }, || call.original());
        });
    })?;
    log::info!("Preferences audio edits stage until native Accept; Cancel leaves active audio unchanged");
    Ok(Installed { _effects: effects, _music: music, _audio: audio, _close: close, _cancel_effects: cancel_effects, _cancel_music: cancel_music, _cancel_enable: cancel_enable, _cancel_disable: cancel_disable })
}

/// Accept the open Preferences form exactly as a click on its round button
/// does: callback 0x587E70 with id 0. The close hook then sees the result
/// byte set, applies staged audio once, and the original closure saves
/// conquests.ini. Call only on the game's window thread from its message
/// dispatch. Returns false when the form is not the open Preferences form.
pub fn accept(build: Build) -> Result<bool, HookError> {
    if build != Build::PcGames { return Err(HookError::Unsupported); }
    let callback = button::Function::at(BUTTON, BUTTON_PROLOGUE)?;
    let visible = visible::Function::at(VISIBLE, VISIBLE_PROLOGUE)?;
    if !real_form(FORM) || visible.call(FORM) == 0 { return Ok(false); }
    callback.call(FORM, ACCEPT_BUTTON_ID);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accept_applies_each_changed_native_callback_once_before_persist_and_close() {
        let actions = plan(true, [false, true], [true, false], [100, 80, 70], [50, 90, 60]);
        let events = std::cell::RefCell::new(Vec::new());
        finish(actions, |action| events.borrow_mut().push(format!("{action:?}")), || events.borrow_mut().push("native persist and close".into()));
        assert_eq!(*events.borrow(), ["Effects(true)", "Music(false)", "Volumes(50)", "native persist and close"]);
    }
    #[test]
    fn cancel_and_clean_accept_invoke_no_audio_callback() {
        for accepted in [false, true] {
            let actions = plan(accepted, [true, true], [true, true], [100, 80, 70], [100, 80, 70]);
            finish(actions, |_| panic!("unchanged or cancelled audio must not apply"), || {});
        }
        let actions = plan(false, [false, false], [true, true], [100, 80, 70], [0, 0, 0]);
        finish(actions, |_| panic!("cancelled changes must not apply"), || {});
    }
    #[test]
    fn phase_scope_restores_the_previous_native_callback_policy() {
        assert_eq!(PHASE.get(), Phase::Edit);
        phase(Phase::Commit, || {
            assert_eq!(PHASE.get(), Phase::Commit);
            phase(Phase::Cancel, || assert_eq!(PHASE.get(), Phase::Cancel));
            assert_eq!(PHASE.get(), Phase::Commit);
        });
        assert_eq!(PHASE.get(), Phase::Edit);
    }
    #[test]
    fn native_accept_is_the_round_button_callback_that_sets_the_result_before_closing() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let game = std::path::Path::new(&install).join("Conquests/Civ3Conquests.exe");
        let Ok(bytes) = std::fs::read(&game) else { return eprintln!("native accept contract check skipped: {} unavailable", game.display()); };
        let code = |at: u32, length: usize| &bytes[(at - 0x401000 + 0x400) as usize..][..length];
        // .rdata VA 0x665000 is at raw offset 0x263A00 in this PE.
        let slot = |offset: u32| u32::from_le_bytes(bytes[(VTABLE + offset - 0x665000 + 0x263A00) as usize..][..4].try_into().unwrap());
        assert_eq!(slot(0xD4), BUTTON, "Button notifications reach the form's callback");
        assert_eq!(slot(8), CLOSE, "the callback's vtable +8 call is the hooked closure");
        assert_eq!(code(BUTTON, BUTTON_PROLOGUE.len()), BUTTON_PROLOGUE);
        // id 0: mov eax,[ecx]; mov byte [ecx+0xEC61],1; call [eax+8].
        assert_eq!(ACCEPT_BUTTON_ID, 0);
        assert_eq!(code(BUTTON + 0x0B, 15), [0x75, 0x1B, 0x8B, 0x01, 0xC6, 0x81, 0x61, 0xEC, 0, 0, 0x01, 0xFF, 0x50, 0x08, 0xC2]);
    }
    #[test]
    fn preferences_callback_and_cancel_callsite_contracts_match_the_installed_game() {
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let game = std::path::Path::new(&install).join("Conquests/Civ3Conquests.exe");
        let Ok(bytes) = std::fs::read(&game) else { return eprintln!("native binary contract check skipped: {} unavailable", game.display()); };
        // This PCGames PE maps .text RVA0x1000 at raw offset0x400.
        let code = |at: u32, length: usize| &bytes[(at - 0x401000 + 0x400) as usize..][..length];
        for (at, expected) in [(EFFECTS, ENABLE_PROLOGUE), (MUSIC, ENABLE_PROLOGUE), (AUDIO, AUDIO_PROLOGUE), (CLOSE, CLOSE_PROLOGUE), (CHECKED, CHECK_PROLOGUE), (VISIBLE, VISIBLE_PROLOGUE)] {
            assert_eq!(code(at, expected.len()), expected, "{at:#x}");
        }
        let guarded = [CANCEL_EFFECTS_VOLUME, CANCEL_MUSIC_VOLUME, CANCEL_EFFECTS_ENABLE, CANCEL_EFFECTS_DISABLE];
        for (at, expected, target) in guarded {
            assert_eq!(code(at, 5), expected, "{at:#x}");
            let relative = i32::from_le_bytes(expected[1..5].try_into().unwrap());
            assert_eq!(at.wrapping_add(5).wrapping_add_signed(relative), target);
        }
        // Check the complete native Cancel branch for any other direct
        // calls into this executable's sound engine, not just known sites.
        let writers: Vec<_> = (0x5881A9..0x5882F2).filter(|&at| {
            let instruction = code(at, 5);
            if instruction[0] != 0xE8 { return false; }
            let target = at.wrapping_add(5).wrapping_add_signed(i32::from_le_bytes(instruction[1..5].try_into().unwrap()));
            (0x5F9000..0x5FB000).contains(&target)
        }).collect();
        assert_eq!(writers, guarded.map(|(site, _, _)| site));
    }

    #[test]
    fn native_cancel_direct_writers_preserve_channel_nodes_flags_and_volume() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct Sound { enabled: bool, nodes: Vec<u32>, volume: u32 }
        let mut sound = Sound { enabled: true, nodes: vec![11, 22], volume: 80 };
        let before = sound.clone();
        phase(Phase::Cancel, || {
            cancel_native_audio(|| sound.volume = 0);
            cancel_native_audio(|| sound.volume = 127);
            cancel_native_audio(|| sound.enabled = true);
            cancel_native_audio(|| { sound.nodes.clear(); sound.enabled = false; });
        });
        assert_eq!(sound, before, "discard must preserve already playing sounds and channel state");
        for context in [Phase::Edit, Phase::Commit] {
            let mut calls = 0;
            phase(context, || cancel_native_audio(|| calls += 1));
            assert_eq!(calls, 1, "ordinary native behavior must be preserved outside Cancel");
        }
    }

    #[test]
    fn cancelled_close_needs_no_foreign_draft_reads_to_disable_all_audio_writers() {
        assert_eq!(commit_plan(0, false), Some(Vec::new()));
    }

}

//! Keep the native MIDI sibling alive only until failed Wave initialization
//! has unwound it. The game normally closes MIDI before unloading sound.dll,
//! but its failed-Wave branch omits that close and leaves a stale owner.
//!
//! Install synchronously from imported IFC23's DllMain, before game startup.
//! A late injection with active or stale audio ownership is declined. Only
//! the audited failed-Wave callsite is redirected; ordinary audio startup
//! and the normal shutdown callsites remain native.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::Build;
use inproc::{Ptr32, hook::HookError};

const FAILURE_SITE: u32 = 0x5F9B7D;
const FAILURE_CALL: [u8; 5] = [0xE8, 0x7E, 0x01, 0x00, 0x00];
const MIDI_WRAPPER: u32 = 0xCAAAD8;
const INITIALIZED: u32 = 0xCAAD40;
const MODULE: u32 = 0xCAAD3C;
const MIDI_BACKEND: u32 = MIDI_WRAPPER + 0x14;
const MASTER_BACKEND: u32 = 0xCAAB14;
const WAVE_IN_BACKEND: u32 = 0xCAAD2C;
const FACTORIES: u32 = 0xCAACEC;

// Supported 66d PCGames contracts include the full native close/delete and
// unload bodies, plus the caller's failure branch and error restoration.
const FAILED_CALLER: (u32, &[u8]) = (0x5F9B6B, &[0x53, 0x57, 0xB9, 0x00, 0xAB, 0xCA, 0x00, 0xE8, 0x39, 0xF3, 0xFF, 0xFF, 0x8B, 0xF0, 0x85, 0xF6, 0x74, 0x0B, 0xE8, 0x7E, 0x01, 0x00, 0x00, 0x8B, 0xC6, 0x5F, 0x5E, 0x5B, 0xC3]);
const MIDI_CLOSE: (u32, &[u8]) = (0x5F9770, &[0x56, 0x8B, 0xF1, 0x8B, 0x4E, 0x14, 0x85, 0xC9, 0x74, 0x13, 0x8B, 0x01, 0xFF, 0x50, 0x14, 0x8B, 0xCE, 0xE8, 0x1A, 0xFF, 0xFF, 0xFF, 0xC7, 0x46, 0x14, 0x00, 0x00, 0x00, 0x00, 0x33, 0xC0, 0x5E, 0xC3]);
const MIDI_DELETE: (u32, &[u8]) = (0x5F96A0, &[0x56, 0x8B, 0xF1, 0x8B, 0x46, 0x14, 0x85, 0xC0, 0x74, 0x16, 0xA1, 0x08, 0xAD, 0xCA, 0x00, 0x85, 0xC0, 0x74, 0x0D, 0xFF, 0xD0, 0xC7, 0x46, 0x14, 0x00, 0x00, 0x00, 0x00, 0x33, 0xC0, 0x5E, 0xC3, 0xB8, 0x14, 0x00, 0x00, 0x00, 0x5E, 0xC3]);
const UNLOAD: (u32, &[u8]) = (0x5F9D00, &[0xA1, 0x3C, 0xAD, 0xCA, 0x00, 0xC7, 0x05, 0x40, 0xAD, 0xCA, 0x00, 0x00, 0x00, 0x00, 0x00, 0x85, 0xC0, 0x74, 0x11, 0x50, 0xFF, 0x15, 0x60, 0x51, 0x66, 0x00, 0xC7, 0x05, 0x3C, 0xAD, 0xCA, 0x00, 0x00, 0x00, 0x00, 0x00, 0xE8, 0x87, 0x00, 0x00, 0x00, 0x33, 0xC0, 0xC3]);

// Native MIDI close is thiscall, ECX=this, no stack arguments, returning u32.
// It performs backend shutdown, original plugin delete and owner clearing.
inproc::thiscall_fn! { fn close_midi(this;) -> u32; }
// Native unload has no arguments and returns u32. The failure caller keeps
// its original Wave error in nonvolatile ESI and restores it after this call.
inproc::cdecl_hook! { mod failed_wave() -> u32; }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Installation {
    Pending = 0,
    Installed = 1,
    UnsupportedImage = 2,
    AudioAlreadyStarted = 3,
    ContractMismatch = 4,
    HookFailed = 5,
}

static STATUS: AtomicU32 = AtomicU32::new(Installation::Pending as u32);

/// Bounded installation outcome for logging after DllMain has returned.
pub fn status() -> Installation {
    match STATUS.load(Ordering::Acquire) {
        1 => Installation::Installed,
        2 => Installation::UnsupportedImage,
        3 => Installation::AudioAlreadyStarted,
        4 => Installation::ContractMismatch,
        5 => Installation::HookFailed,
        _ => Installation::Pending,
    }
}

fn supported_image(base: u32, build: Option<Build>) -> bool {
    base == crate::build::IMAGE_BASE && build == Some(Build::PcGames)
}

fn clean_preinit(mut read: impl FnMut(u32) -> Option<u32>) -> bool {
    [INITIALIZED, MODULE, MIDI_BACKEND, MASTER_BACKEND, WAVE_IN_BACKEND]
        .into_iter()
        .chain((0..11).map(|index| FACTORIES + index * 4))
        .all(|at| read(at) == Some(0))
}

fn validate_contracts(mut read: impl FnMut(u32, &mut [u8]) -> Result<(), inproc::MemError>) -> Result<(), HookError> {
    for (at, expected) in [FAILED_CALLER, MIDI_CLOSE, MIDI_DELETE, UNLOAD] {
        let mut actual = [0u8; 128];
        let actual = &mut actual[..expected.len()];
        read(at, actual)?;
        if actual != expected {
            return Err(HookError::UnexpectedCode { addr: at, found: actual.to_vec(), expected: expected.to_vec() });
        }
    }
    Ok(())
}

struct UnwindEvidence {
    completed: AtomicU32,
    first_claimed: AtomicBool,
    first_ready: AtomicBool,
    midi_close_result: AtomicU32,
    cleanup_result: AtomicU32,
}

impl UnwindEvidence {
    const fn new() -> Self {
        Self { completed: AtomicU32::new(0), first_claimed: AtomicBool::new(false), first_ready: AtomicBool::new(false),
            midi_close_result: AtomicU32::new(0), cleanup_result: AtomicU32::new(0) }
    }

    fn snapshot(&self) -> (u32, Option<u32>, Option<u32>) {
        // The first completed record is immutable once ready. Later
        // unwinds increase only the total count and never overwrite it.
        let first_ready = self.first_ready.load(Ordering::Acquire);
        let count = self.completed.load(Ordering::Acquire);
        (count, first_ready.then(|| self.midi_close_result.load(Ordering::Relaxed)),
            first_ready.then(|| self.cleanup_result.load(Ordering::Relaxed)))
    }
}

static UNWINDS: UnwindEvidence = UnwindEvidence::new();

fn unwind_failed_wave(close: impl FnOnce() -> u32, unload: impl FnOnce() -> u32, evidence: &UnwindEvidence) -> u32 {
    let closed = close();
    let result = unload();
    // Claim once after both calls complete. No spin, waiting or overwrite:
    // readers see None while that first record is still being published.
    let first = evidence.first_claimed.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed).is_ok();
    if first {
        evidence.midi_close_result.store(closed, Ordering::Relaxed);
        evidence.cleanup_result.store(result, Ordering::Relaxed);
    }
    evidence.completed.fetch_add(1, Ordering::Release);
    if first { evidence.first_ready.store(true, Ordering::Release); }
    result
}

/// Read only when an existing trace request is being formatted. Failed reads
/// remain explicit; unsupported/declined installation does not read addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnostics {
    pub installation: Installation,
    pub unwind_count: u32,
    pub first_midi_close_result: Option<u32>,
    pub first_cleanup_result: Option<u32>,
    pub initialized: Option<u32>,
    pub module: Option<u32>,
    pub midi_backend: Option<u32>,
    pub master_backend: Option<u32>,
    pub wave_in_backend: Option<u32>,
    pub factories_cleared: Option<bool>,
}

fn diagnostics_with(installation: Installation, evidence: &UnwindEvidence, mut read: impl FnMut(u32) -> Option<u32>) -> Diagnostics {
    let (unwind_count, first_midi_close_result, first_cleanup_result) = evidence.snapshot();
    let mut out = Diagnostics { installation, unwind_count, first_midi_close_result, first_cleanup_result,
        initialized: None, module: None, midi_backend: None, master_backend: None,
        wave_in_backend: None, factories_cleared: None };
    if installation != Installation::Installed { return out; }
    out.initialized = read(INITIALIZED);
    out.module = read(MODULE);
    out.midi_backend = read(MIDI_BACKEND);
    out.master_backend = read(MASTER_BACKEND);
    out.wave_in_backend = read(WAVE_IN_BACKEND);
    out.factories_cleared = (0..11).try_fold(true, |clear, index| read(FACTORIES + index * 4).map(|value| clear && value == 0));
    out
}

pub fn diagnostics() -> Diagnostics {
    diagnostics_with(status(), &UNWINDS, |at| Ptr32::<u32>::new(at).read().ok())
}

#[cfg(windows)]
fn install() -> Result<failed_wave::Installed, Installation> {
    let base = inproc::pe::main_module();
    if !supported_image(base, Build::detect(base).ok()) {
        return Err(Installation::UnsupportedImage);
    }
    // Startup imports have zeroed owners/factories. Late injection must not
    // retroactively touch a MIDI child left stale by an earlier native unload.
    if !clean_preinit(|at| Ptr32::<u32>::new(at).read().ok()) {
        return Err(Installation::AudioAlreadyStarted);
    }
    validate_contracts(|at, out| inproc::mem::read_bytes(u64::from(at), out))
        .map_err(|_| Installation::ContractMismatch)?;
    let close = close_midi::Function::at(MIDI_CLOSE.0, MIDI_CLOSE.1)
        .map_err(|_| Installation::ContractMismatch)?;
    failed_wave::install_call_site(FAILURE_SITE, &FAILURE_CALL, move |call| {
        // Native close handles no-MIDI and already-cleared MIDI itself. It
        // runs on the original game's failure path with its module live.
        unwind_failed_wave(|| close.call(MIDI_WRAPPER), || call.original(), &UNWINDS)
    }).map_err(|_| Installation::HookFailed)
}

/// Synchronous, bounded installation under DllMain. No sound method, library
/// load, file I/O, GUI call, new thread, waiting lock or retry runs here.
#[cfg(windows)]
pub fn early() {
    let outcome = match install() {
        Ok(hook) => {
            // Preserve ownership through native process teardown, as with
            // the other early bridge hooks. The guard is intentionally bounded.
            std::mem::forget(hook);
            Installation::Installed
        }
        Err(reason) => reason,
    };
    STATUS.store(outcome as u32, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct NativeAudio {
        module_live: bool,
        midi_owned: bool,
        shutdowns: u32,
        deletes: u32,
        events: Vec<&'static str>,
    }

    impl NativeAudio {
        fn close_midi(&mut self) -> u32 {
            if self.midi_owned {
                assert!(self.module_live, "a stale MIDI backend must never be called after unload");
                self.events.push("MIDI shutdown");
                self.shutdowns += 1;
                self.events.push("MIDI plugin delete");
                self.deletes += 1;
                self.midi_owned = false;
            }
            0
        }
        fn unload(&mut self) -> u32 {
            self.events.push("module unload");
            self.module_live = false;
            0
        }
    }

    #[test]
    fn failed_wave_unwinds_live_midi_before_module_unload() {
        let state = RefCell::new(NativeAudio { module_live: true, midi_owned: true, ..Default::default() });
        let result = unwind_failed_wave(|| state.borrow_mut().close_midi(), || state.borrow_mut().unload(), &UnwindEvidence::new());
        let state = state.into_inner();
        assert_eq!(result, 0);
        assert!(!state.module_live && !state.midi_owned);
        assert_eq!((state.shutdowns, state.deletes), (1, 1));
        assert_eq!(state.events, ["MIDI shutdown", "MIDI plugin delete", "module unload"]);
    }

    #[test]
    fn native_missing_close_leaves_the_reproduced_stale_owner() {
        let mut state = NativeAudio { module_live: true, midi_owned: true, ..Default::default() };
        state.unload();
        assert!(!state.module_live && state.midi_owned);
    }

    #[test]
    fn no_midi_uses_native_noop_then_unloads_once() {
        let state = RefCell::new(NativeAudio { module_live: true, ..Default::default() });
        unwind_failed_wave(|| state.borrow_mut().close_midi(), || state.borrow_mut().unload(), &UnwindEvidence::new());
        let state = state.into_inner();
        assert_eq!((state.shutdowns, state.deletes), (0, 0));
        assert_eq!(state.events, ["module unload"]);
    }

    #[test]
    fn original_cleanup_result_is_never_replaced() {
        for result in [0, 1, 3, u32::MAX] {
            assert_eq!(unwind_failed_wave(|| 0, || result, &UnwindEvidence::new()), result);
        }
    }

    #[test]
    fn supported_build_requires_the_audited_absolute_image() {
        assert!(supported_image(0x400000, Some(Build::PcGames)));
        assert!(!supported_image(0x500000, Some(Build::PcGames)));
        assert!(!supported_image(0x400000, Some(Build::Gog)));
        assert!(!supported_image(0x400000, Some(Build::Steam)));
        assert!(!supported_image(0x400000, None));
    }

    #[test]
    fn active_stale_and_unreadable_late_injection_are_declined() {
        assert!(clean_preinit(|_| Some(0)));
        for nonzero in [INITIALIZED, MODULE, MIDI_BACKEND, MASTER_BACKEND, WAVE_IN_BACKEND, FACTORIES, FACTORIES + 40] {
            assert!(!clean_preinit(|at| Some(u32::from(at == nonzero))));
        }
        // This is the observed stale state: module/init/factories zero,
        // but MIDI still owns the child that was invalidated by the unload.
        assert!(!clean_preinit(|at| Some(if at == MIDI_BACKEND { 0x2652920 } else { 0 })));
        assert!(!clean_preinit(|at| if at == MODULE { None } else { Some(0) }));
    }

    #[test]
    fn any_changed_native_contract_is_declined_before_installation() {
        for (changed, _) in [FAILED_CALLER, MIDI_CLOSE, MIDI_DELETE, UNLOAD] {
            let result = validate_contracts(|at, out| {
                let expected = [FAILED_CALLER, MIDI_CLOSE, MIDI_DELETE, UNLOAD].into_iter().find(|(address, _)| *address == at).unwrap().1;
                out.copy_from_slice(expected);
                if at == changed { out[0] ^= 1; }
                Ok(())
            });
            assert!(matches!(result, Err(HookError::UnexpectedCode { addr, .. }) if addr == changed));
        }
        assert!(validate_contracts(|at, out| {
            let expected = [FAILED_CALLER, MIDI_CLOSE, MIDI_DELETE, UNLOAD].into_iter().find(|(address, _)| *address == at).unwrap().1;
            out.copy_from_slice(expected);
            Ok(())
        }).is_ok());
    }

    #[test]
    fn failure_call_and_native_error_restore_contract_are_exact() {
        assert_eq!(&FAILED_CALLER.1[18..23], &FAILURE_CALL);
        let displacement = i32::from_le_bytes(FAILURE_CALL[1..].try_into().unwrap());
        assert_eq!(i64::from(FAILURE_SITE) + 5 + i64::from(displacement), i64::from(UNLOAD.0));
        assert_eq!(&FAILED_CALLER.1[12..18], &[0x8B, 0xF0, 0x85, 0xF6, 0x74, 0x0B]);
        assert_eq!(&FAILED_CALLER.1[23..], &[0x8B, 0xC6, 0x5F, 0x5E, 0x5B, 0xC3]);
        assert_eq!(&MIDI_CLOSE.1[MIDI_CLOSE.1.len() - 2..], &[0x5E, 0xC3]);
        assert_eq!(&UNLOAD.1[UNLOAD.1.len() - 3..], &[0x33, 0xC0, 0xC3]);
    }

    #[test]
    fn unwind_evidence_publishes_only_after_both_original_calls() {
        let evidence = UnwindEvidence::new();
        assert_eq!(evidence.snapshot(), (0, None, None));
        let result = unwind_failed_wave(|| {
            assert_eq!(evidence.snapshot(), (0, None, None));
            17
        }, || {
            assert_eq!(evidence.snapshot(), (0, None, None));
            42
        }, &evidence);
        assert_eq!(result, 42);
        assert_eq!(evidence.snapshot(), (1, Some(17), Some(42)));
    }

    #[test]
    fn requested_diagnostics_distinguish_cleared_unreadable_and_active_audio() {
        let evidence = UnwindEvidence::new();
        unwind_failed_wave(|| 0, || 0, &evidence);
        let cleared = diagnostics_with(Installation::Installed, &evidence, |_| Some(0));
        assert_eq!(cleared.unwind_count, 1);
        assert_eq!(cleared.first_midi_close_result, Some(0));
        assert_eq!(cleared.first_cleanup_result, Some(0));
        assert_eq!(cleared.module, Some(0));
        assert_eq!(cleared.midi_backend, Some(0));
        assert_eq!(cleared.factories_cleared, Some(true));
        let unreadable = diagnostics_with(Installation::Installed, &evidence, |at| if at == MIDI_BACKEND { None } else { Some(0) });
        assert_eq!(unreadable.midi_backend, None);
        let active = diagnostics_with(Installation::Installed, &UnwindEvidence::new(), |_| Some(1));
        assert_eq!(active.unwind_count, 0);
        assert_eq!(active.first_midi_close_result, None);
        assert_eq!(active.module, Some(1));
        assert_eq!(active.factories_cleared, Some(false));
    }

    #[test]
    fn declined_installation_diagnostics_do_not_read_native_addresses() {
        let mut reads = 0;
        let out = diagnostics_with(Installation::UnsupportedImage, &UnwindEvidence::new(), |_| { reads += 1; Some(7) });
        assert_eq!(reads, 0);
        assert_eq!(out.module, None);
        assert_eq!(out.midi_backend, None);
        assert_eq!(out.factories_cleared, None);
    }


    #[test]
    fn later_completions_never_overwrite_the_first_completed_record() {
        let evidence = UnwindEvidence::new();
        // A nested completion finishes before the outer invocation. The
        // retained record belongs to that first completion, not first entry.
        assert_eq!(unwind_failed_wave(|| {
            assert_eq!(unwind_failed_wave(|| 11, || 22, &evidence), 22);
            33
        }, || 44, &evidence), 44);
        assert_eq!(evidence.snapshot(), (2, Some(11), Some(22)));
        assert_eq!(unwind_failed_wave(|| 55, || 66, &evidence), 66);
        assert_eq!(evidence.snapshot(), (3, Some(11), Some(22)));
        let publishing = UnwindEvidence::new();
        publishing.first_claimed.store(true, Ordering::Relaxed);
        publishing.completed.store(1, Ordering::Release);
        assert_eq!(publishing.snapshot(), (1, None, None));
    }

}

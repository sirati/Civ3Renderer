//! The checked C3X mill producer's draw-only block, after selection and
//! its count/sheet/JGL/TransparentBlt availability gates. No game logic or
//! GDI result is substituted. Owned mode returns through the producer's
//! existing epilogue; original mode continues through the stolen bytes.

use inproc::{
    Ptr32,
    hook::{self, HandlerSlot, HookError, InlinePatch},
};
use std::sync::atomic::{AtomicBool, Ordering};

const BEGIN: u32 = 0xdeee34;
const BYTES: [u8; 6] = [0x8b, 0x45, 0x08, 0x83, 0xc0, 0x04];
const RETURN: u32 = 0xdef018;
const EPILOGUE: [u8; 7] = [0x8b, 0x45, 0xf4, 0xc9, 0xc2, 0x10, 0x00];
static ACTIVE: AtomicBool = AtomicBool::new(false);
static HANDLER: HandlerSlot<Box<dyn Fn() + Send + Sync>> = HandlerSlot::new();
static STAT: inproc::hook_time::Stat = inproc::hook_time::Stat::new("mill_draw_boundary");

pub(super) fn check() -> Result<(), HookError> {
    InlinePatch::check(BEGIN, &BYTES)?;
    let found = Ptr32::<u8>::new(RETURN).read_array(EPILOGUE.len())?;
    if found != EPILOGUE {
        return Err(HookError::UnexpectedCode {
            addr: RETURN,
            found,
            expected: EPILOGUE.to_vec(),
        });
    }
    Ok(())
}

extern "C" fn ready() {
    let _timed = inproc::hook_time::Hook::enter(&STAT);
    if let Some(handler) = HANDLER.get() {
        // Never unwind through a foreign stack, and never resume legacy
        // raster work after a callback failure in owned mode.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(handler)).is_err() {
            log::error!("C3X mill draw-boundary callback panicked");
        }
    }
}

/// Restores the exact original block on drop. Used only after Producer::find
/// has bound this body, call sites, state pointer and helper ABIs to R28.
pub struct Installed(Option<InlinePatch>);
impl Drop for Installed {
    fn drop(&mut self) {
        drop(self.0.take());
        HANDLER.set(None);
        ACTIVE.store(false, Ordering::Release);
    }
}

impl super::Producer {
    pub fn intercept_drawing(
        self,
        owned: bool,
        notify: impl Fn() + Send + Sync + 'static,
    ) -> Result<Installed, HookError> {
        if !cfg!(all(windows, target_arch = "x86")) {
            return Err(HookError::Unsupported);
        }
        if ACTIVE.swap(true, Ordering::AcqRel) {
            return Err(HookError::AlreadyInstalled);
        }
        let result = (|| {
            check()?;
            // pushfd/pushad; save ESP in callee-saved EBX and align for the
            // C callback; restore ESP/registers/flags before continuing.
            // No borrow or lock spans the callback, including nested draws.
            let mut code = vec![0x9c, 0x60, 0x89, 0xe3, 0x83, 0xe4, 0xf0, 0xb8];
            code.extend((ready as *const () as usize as u32).to_le_bytes());
            code.extend([0xff, 0xd0, 0x89, 0xdc, 0x61, 0x9d]);
            if !owned {
                code.extend(BYTES);
            }
            let at = hook::alloc_code(code.len() + 5)?;
            let destination = if owned {
                RETURN
            } else {
                BEGIN + BYTES.len() as u32
            };
            let after_jump = at + code.len() as u32 + 5;
            code.push(0xe9);
            code.extend(destination.wrapping_sub(after_jump).to_le_bytes());
            hook::write_code(at, &code)?;
            HANDLER.set(Some(Box::new(notify)));
            InlinePatch::install(BEGIN, &BYTES, at, |_| {})
        })();
        result.map(|patch| Installed(Some(patch))).inspect_err(|_| {
            HANDLER.set(None);
            ACTIVE.store(false, Ordering::Release);
        })
    }
}

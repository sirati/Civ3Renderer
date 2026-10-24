//! Per-call cost of a text run on the old path (jgl's `text_out` with the
//! `TextOutA` import hook the bridge had, making the run from GDI's DC
//! state) against the new one (jgl's `text_out` slot taken over by
//! `jgl-ffi`, the record consumed by `jgl` into `civ3-ui`'s `TextCapture`).
//!
//! Both end in a sink that only keeps the run, so the interface mirror's
//! own work (the same on both paths) is left out. A 32-bit Windows program:
//!
//! ```text
//! cargo build --release -p civ3-bridge --example jgl_text_bench --target i686-pc-windows-gnu
//! wine target/i686-pc-windows-gnu/release/examples/jgl_text_bench.exe 'Z:\path\to\jgl.dll' [N]
//! ```

#[cfg(all(windows, target_arch = "x86"))]
fn main() {
    bench::run();
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn main() {
    eprintln!("jgl_text_bench runs in a 32-bit Windows process (i686-pc-windows-gnu)");
}

#[cfg(all(windows, target_arch = "x86"))]
#[allow(unsafe_code)]
mod bench {
    use std::cell::Cell;
    use std::hint::black_box;
    use std::time::Instant;

    use civ3_ui::text::{CanvasText, TextCapture, TextRun};
    use windows_sys::Win32::Graphics::Gdi::{CreateCompatibleDC, CreateFontA, SelectObject, SetTextColor};
    use windows_sys::Win32::System::LibraryLoader::LoadLibraryA;

    inproc::stdcall_hook! {
        /// GDI `TextOutA` as jgl.dll imports it (as `civ3_sys::hooks::text_out`).
        pub mod text_out(dc, x, y, text, len) -> i32;
    }

    thread_local! {
        static RUNS: Cell<u64> = const { Cell::new(0) };
    }

    /// The old hook's run, as `trace.rs` `record_text` made it (before the
    /// takeover replaced it).
    fn old_record(call: &text_out::Call) {
        let len = (call.len as i32).clamp(0, 1024) as usize;
        let Ok(bytes) = inproc::Ptr32::<u8>::new(call.text).read_array(len) else { return };
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let Some(font) = inproc::gdi::dc_font(call.dc) else { return };
        let (r, g, b) = inproc::gdi::dc_text_color(call.dc);
        let run = TextRun { x: call.x as i32, y: call.y as i32, color: [r, g, b], face: font.face, height: font.height, weight: font.weight, italic: font.italic, underline: font.underline, text, backdrop: None };
        black_box(&run);
        RUNS.with(|n| n.set(n.get() + 1));
    }

    fn sink(t: CanvasText) {
        black_box(&t);
        RUNS.with(|n| n.set(n.get() + 1));
    }

    struct Skip;
    impl jgl::PixelPolicy for Skip {
        fn original_pixels(&self, _: jgl::ImageId, _: u32) -> bool {
            false
        }
    }

    type TextOutSlot = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;

    fn time(label: &str, n: u32, image: u32, text: &[u8]) -> f64 {
        // SAFETY: `image`'s first word is jgl's image vtable; slot +0xB8 is
        // `text_out(this, x, y, text, len)`, thiscall, as `jgl::slots` has it.
        let f: TextOutSlot = unsafe {
            let vtable = *(image as *const u32);
            std::mem::transmute::<usize, TextOutSlot>(*((vtable + 0xB8) as *const u32) as usize)
        };
        RUNS.with(|r| r.set(0));
        for _ in 0..n / 10 {
            f(image, 10, 20, text.as_ptr() as u32, text.len() as u32);
        }
        RUNS.with(|r| r.set(0));
        let start = Instant::now();
        for i in 0..n {
            f(image, 10 + (i & 7), 20, text.as_ptr() as u32, text.len() as u32);
        }
        let ns = start.elapsed().as_nanos() as f64 / n as f64;
        let runs = RUNS.with(Cell::get);
        println!("{label}: {ns:.0} ns per call ({runs} runs of {n} calls)");
        ns
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().collect();
        let path = std::ffi::CString::new(args.get(1).cloned().unwrap_or_else(|| "jgl.dll".into())).unwrap();
        let n: u32 = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(200_000);
        // SAFETY: loading the game's jgl.dll; its DllMain only initialises its CRT.
        let base = unsafe { LoadLibraryA(path.as_ptr().cast()) } as usize as u32;
        assert!(base != 0, "cannot load {path:?}");
        let text = b"Rome (12) - Settler 3 turns";

        // A JGL_Image as jgl's dispatch sees it: jgl's image vtable, an
        // owner, a memory DC with a font and a text colour.
        let mut image = vec![0u32; 0x4D4 / 4].into_boxed_slice();
        image[0] = base + jgl::slots::Class::Image.vtable_rva();
        image[0x80 / 4] = 0x00C8_87FC;
        image[0x44 / 4..0x54 / 4].copy_from_slice(&[0, 0, 1024, 768]);
        // SAFETY: plain GDI object creation on a memory DC.
        let dc = unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            let font = CreateFontA(-14, 0, 0, 0, 700, 0, 0, 0, 0, 7, 0, 0, 0, c"Times New Roman".as_ptr().cast());
            SelectObject(dc, font);
            SetTextColor(dc, 0x0040_80C0);
            dc
        };
        image[0x4BC / 4] = dc as usize as u32;
        let image = image.as_ptr() as u32;

        // jgl's own text_out, drawing (GDI TextOutA into a 1x1 bitmap).
        let plain = time("jgl text_out, no hook (TextOutA draws)", n, image, text);

        // Old: the import hook, as installed by trace.rs before the takeover.
        let slot = inproc::pe::import_slot(base, "GDI32.dll", "TextOutA").unwrap().expect("jgl imports TextOutA");
        let hook = text_out::install_slot(slot, |call| {
            old_record(call);
            1
        })
        .unwrap();
        let old = time("old: jgl text_out + TextOutA import hook (run from the DC)", n, image, text);
        drop(hook);

        // New: the text group taken over; the run from the record.
        let group = jgl_ffi::text::install(base, Box::new(|| jgl::Hooks { text: Box::new(TextCapture::new(sink)), policy: Box::new(Skip) })).unwrap();
        let new = time("new: text_out taken over (record, jgl state, TextCapture)", n, image, text);
        drop(group);
        println!("old / new: {:.1}x; plain jgl {plain:.0} ns", old / new);
    }
}

//! Original-binary checks for caller-written navigator data. No game process,
//! shared game DLL replacement, or viewer-pixel read-back is involved.
#![cfg(all(windows, target_arch = "x86"))]

use jgl::ImageId;
use jgl_ffi::layout::Rect32;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

struct Data;
impl jgl::PixelPolicy for Data {
    fn preserve_image_data(&self, _: ImageId, owner: u32) -> bool { owner != 0 }
}

unsafe fn call(object: u32, slot: u32, args: &[u32]) -> u32 {
    unsafe {
        let table = *(object as usize as *const u32);
        let function = *((table + slot) as usize as *const u32) as usize;
        match *args {
            [] => std::mem::transmute::<usize, extern "thiscall" fn(u32) -> u32>(function)(object),
            [a] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32) -> u32>(function)(object, a),
            [a,b] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32, u32) -> u32>(function)(object, a, b),
            [a,b,c] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32, u32, u32) -> u32>(function)(object, a, b, c),
            [a,b,c,d] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32>(function)(object, a, b, c, d),
            _ => panic!("test arity"),
        }
    }
}

fn addr<T>(value: &T) -> u32 { value as *const T as usize as u32 }
fn rect(l: i32, t: i32, r: i32, b: i32) -> Rect32 { Rect32 { left: l, top: t, right: r, bottom: b } }

unsafe fn field(object: u32, offset: u32) -> u32 {
    unsafe { *((object.wrapping_add(offset)) as usize as *const u32) }
}

unsafe fn bytes(image: u32, height: u32) -> Vec<u8> {
    unsafe {
        let stride = call(image, 0xE0, &[]) * 2;
        let at = call(image, 0x10, &[]);
        let result = std::slice::from_raw_parts(at as *const u8, (stride * height) as usize).to_vec();
        call(image, 0x24, &[1]);
        result
    }
}

unsafe fn seed(image: u32, height: u32, salt: usize) {
    unsafe {
        let stride = call(image, 0xE0, &[]) * 2;
        let at = call(image, 0x10, &[]);
        for (i, value) in std::slice::from_raw_parts_mut(at as *mut u8, (stride * height) as usize).iter_mut().enumerate() {
            *value = (i.wrapping_mul(29) + salt) as u8;
        }
        call(image, 0x24, &[1]);
    }
}

#[test]
fn generated_data_copies_and_clears_match_original_jgl() {
    let Ok(path) = std::env::var("CIV3_JGL_DLL") else {
        eprintln!("skipped: CIV3_JGL_DLL is required for the original-binary check");
        return;
    };
    let mut path = path.into_bytes(); path.push(0);
    // SAFETY: this process owns every image and owner field below. The calls
    // use the checked x86 thiscall signatures and pointers to live storage.
    unsafe {
        let module = LoadLibraryA(path.as_ptr());
        assert!(!module.is_null());
        let export = GetProcAddress(module, c"get_graphsy_object_ptr".as_ptr().cast()).unwrap();
        let original = std::mem::transmute::<unsafe extern "system" fn() -> isize, extern "C" fn() -> u32>(export)();
        jgl_ffi::own::install(Box::new(|| jgl::Hooks { policy: Box::new(Data), ..Default::default() }), Box::new(jgl::hooks::NoWindowHooks)).unwrap();
        let owned = jgl_ffi::own::new_graphics();
        let (mut calls, mut checked_bytes) = (0, 0);
        for (w, h) in [(3, 5), (12, 9), (230, 108), (300, 145)] {
            let mut owners = [[0u32; 8]; 4];
            let images: Vec<u32> = [original, original, owned, owned].into_iter().enumerate().map(|(i, g)| {
                let image = call(g, 0x7C, &[addr(&owners[i]), 0]);
                owners[i][1] = image;
                assert_ne!(image, 0);
                assert_eq!(call(image, 4, &[w, h, 16, 1]), 0);
                image
            }).collect();
            if w == 3 && h == 5 {
                assert_ne!(field(images[2], 0x4BC), 0, "owned image keeps the direct C3X DC contract");
                assert_eq!(field(images[2], 0x4C0), 0, "ordinary owned canvas pixels are lazy");
                assert_ne!(call(images[2], 0x10, &[]), 0, "first lock allocates plain pixels");
                call(images[2], 0x24, &[1]);
                call(images[2], 0x08, &[]);
                assert_eq!(field(images[2], 0x4C0), 0, "clear releases lazy canvas pixels");
                assert_eq!(field(images[2], 0x4BC), 0, "clear releases the scratch DC");
                assert_eq!(call(images[2], 4, &[w, h, 16, 1]), 0);
            }
            let whole = rect(0, 0, w as i32, h as i32);
            for clip in [whole, rect(1, 1, w as i32 - 1, h as i32 - 1)] {
                for (src, dst) in [(whole, whole), (rect(0,0,w as i32 - 1,h as i32 - 1), rect(1,1,w as i32,h as i32))] {
                    for (i, &image) in images.iter().enumerate() {
                        seed(image, h, if i % 2 == 0 { 17 } else { 91 });
                        assert_eq!(call(image, 0x34, &[addr(&clip)]), 0);
                    }
                    let before_source = bytes(images[0], h);
                    assert_eq!(call(images[0], 0x40, &[images[1], addr(&src), addr(&dst)]), 0);
                    assert_eq!(call(images[2], 0x40, &[images[3], addr(&src), addr(&dst)]), 0);
                    windows_sys::Win32::Graphics::Gdi::GdiFlush();
                    let expected = bytes(images[1], h);
                    assert_eq!(bytes(images[3], h), expected, "copy {w}x{h}, {clip:?}, {src:?} -> {dst:?}");
                    assert_eq!(bytes(images[2], h), before_source, "copy does not modify its source");
                    checked_bytes += expected.len(); calls += 1;
                }
                for color in [0x8000_0000, 0x8000_1234, 0x8000_7FFF] {
                    for &image in &[images[1], images[3]] { seed(image, h, 91); }
                    assert_eq!(call(images[1], 0x44, &[addr(&whole), color]), 0);
                    assert_eq!(call(images[3], 0x44, &[addr(&whole), color]), 0);
                    windows_sys::Win32::Graphics::Gdi::GdiFlush();
                    let expected = bytes(images[1], h);
                    assert_eq!(bytes(images[3], h), expected, "fill {w}x{h}, {clip:?}, {color:#x}");
                    checked_bytes += expected.len(); calls += 1;
                }
            }
            // A normal canvas with no data owner stays record-only.
            let canvas = call(owned, 0x7C, &[0, 0]);
            assert_eq!(call(canvas, 4, &[w, h, 16, 1]), 0);
            seed(canvas, h, 91);
            let before = bytes(canvas, h);
            assert_eq!(call(images[2], 0x40, &[canvas, addr(&whole), addr(&whole)]), 0);
            assert_eq!(call(canvas, 0x44, &[0, 0x8000_1234]), 0);
            assert_eq!(bytes(canvas, h), before);
            call(canvas, 0, &[1]);
            for image in images { call(image, 0, &[1]); }
            assert!(owners.iter().all(|o| o[1] == 0));
        }
        call(original, 0x0C, &[]); call(owned, 0x0C, &[]);
        println!("original/owned data equivalence: {calls} calls, {checked_bytes} bytes, zero differences; ordinary canvases unchanged");
    }
}

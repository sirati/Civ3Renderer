//! Real x86 ABI and GDI checks; no game process or shared installation.
#![cfg(all(windows, target_arch = "x86"))]

use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

unsafe fn call(object: u32, slot: u32, args: &[u32]) -> u32 {
    unsafe {
        let table = *(object as usize as *const u32);
        let function = *((table + slot) as usize as *const u32) as usize;
        match *args {
            [] => std::mem::transmute::<usize, extern "thiscall" fn(u32) -> u32>(function)(object),
            [a] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32) -> u32>(function)(object, a),
            [a, b] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32, u32) -> u32>(function)(object, a, b),
            [a, b, c, d] => std::mem::transmute::<usize, extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32>(function)(object, a, b, c, d),
            _ => panic!("test arity"),
        }
    }
}

fn address<T>(p: &mut T) -> u32 { p as *mut T as usize as u32 }

#[test]
fn original_cursor_order_owned_embedded_font_and_gdi_resource_release() {
    let (Ok(dll), Ok(source_font)) = (std::env::var("CIV3_JGL_DLL"), std::env::var("CIV3_JGL_FONT")) else {
        eprintln!("skipped: CIV3_JGL_DLL and CIV3_JGL_FONT are required for the original-binary/GDI check");
        return;
    };
    let mut dll = dll.into_bytes(); dll.push(0);
    // SAFETY: standalone test owns both graphics objects and all their
    // owner storage. Each method signature is the recovered x86 ABI.
    unsafe {
        let module = LoadLibraryA(dll.as_ptr());
        assert!(!module.is_null());
        let export = GetProcAddress(module, c"get_graphsy_object_ptr".as_ptr().cast()).unwrap();
        let original = std::mem::transmute::<unsafe extern "system" fn() -> isize, extern "C" fn() -> u32>(export)();
        let base = module as usize as u32;
        let current = || *((base + 0x70D38) as usize as *const u32);
        let mut expected = jgl::registry::Registry::default();
        let mut owners = [[0u32; 8]; 5];
        let mut objects = Vec::new();
        for (index, slot) in [0x7C, 0x78, 0x74, 0x80, 0x7C].into_iter().enumerate() {
            let owner = address(&mut owners[index]);
            let object = match slot {
                0x74 | 0x78 => call(original, slot, &[owner]),
                _ => call(original, slot, &[owner, 0]),
            };
            let registered = object + if slot == 0x74 { 8 } else { 0 };
            objects.push((object, registered));
            expected.made(registered);
            assert_eq!(current(), expected.current().unwrap());
        }
        let (object, registered) = objects[1];
        call(object, 0, &[1]); expected.gone(registered);
        assert_eq!(current(), expected.current().unwrap());
        let mut deleted = 1;
        while let Some(object) = expected.current() {
            assert_eq!(current(), object);
            call(object, 0, &[1]); expected.gone(object); deleted += 1;
        }
        assert_eq!(deleted, 5);
        assert_eq!(*((base + 0x70D40) as usize as *const u32), 0);
        call(original, 0x0C, &[]);

        jgl_ffi::own::install(Box::new(jgl::Hooks::default), Box::new(jgl::hooks::NoWindowHooks)).unwrap();
        let graphics = jgl_ffi::own::new_graphics();
        assert_eq!(call(graphics, 0xB8, &[]), 0);
        let mut font_owner = [0u32; 8];
        let font = call(graphics, 0x74, &[address(&mut font_owner)]);
        font_owner[0] = font;
        // Wine reuses Windows PIDs across separate runs. The copied game
        // font may be read-only, so each run needs a fresh destination.
        let serial = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("jgl-lifetime-{}-{serial}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::copy(source_font, dir.join("LSANS.TTF")).unwrap();
        std::env::set_current_dir(&dir).unwrap();
        for _ in 0..8 {
            assert_eq!(call(font, 4, &[c"LSANS.TTF".as_ptr() as u32, c"Lucida Sans".as_ptr() as u32, 14, 0]), 0);
            assert_ne!(*((font + 0x20) as usize as *const u32), 0);
            let handle = *((font + 0x18) as usize as *const u32);
            assert_ne!(handle, 0);
            assert!(call(font, 0x10, &[c"Test width".as_ptr() as u32, 10]) > 0);
            call(font, 0x0C, &[]);
            assert_eq!(*((font + 0x20) as usize as *const u32), 0);
            assert_eq!(windows_sys::Win32::Graphics::Gdi::GetObjectType(handle as usize as _), 0);
        }
        // Release must call the embedded +8 base destructor, adjust this,
        // and clear the exe's owner pointer, without touching freed memory.
        call(graphics, 0xBC, &[]);
        call(graphics, 0x0C, &[]);
        assert_eq!(font_owner[0], 0);
    }
}

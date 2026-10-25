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
            [a,b,c,d,e,f,g,h] => std::mem::transmute::<usize, extern "thiscall" fn(u32,u32,u32,u32,u32,u32,u32,u32,u32)->u32>(function)(object,a,b,c,d,e,f,g,h),
            _ => panic!("test arity"),
        }
    }
}

fn addr<T>(value: &T) -> u32 { value as *const T as usize as u32 }
fn rect(l: i32, t: i32, r: i32, b: i32) -> Rect32 { Rect32 { left: l, top: t, right: r, bottom: b } }

unsafe fn field(object: u32, offset: u32) -> u32 {
    unsafe { *((object.wrapping_add(offset)) as usize as *const u32) }
}

unsafe fn bytes(image: u32) -> Vec<u8> {
    unsafe {
        let len = call(image, 0xE0, &[]) * call(image, 0xDC, &[]) * field(image, 0x24) / 8;
        let p = call(image, 0x10, &[]);
        assert_ne!(p, 0);
        let data = std::slice::from_raw_parts(p as *const u8, len as usize).to_vec();
        call(image, 0x24, &[1]);
        data
    }
}

unsafe fn seed(image: u32, salt: usize) {
    unsafe {
        let len = call(image, 0xE0, &[]) * call(image, 0xDC, &[]) * field(image, 0x24) / 8;
        let p = call(image, 0x10, &[]);
        for (i, b) in std::slice::from_raw_parts_mut(p as *mut u8, len as usize).iter_mut().enumerate() {
            *b = ((i + salt) % 7) as u8;
        }
        call(image, 0x24, &[1]);
    }
}

unsafe fn state_and_errors(original:u32, owned:u32) {
    unsafe {
        let fields=[0x24,0x28,0x2C,0x30,0x34,0x38,0x3C,0x40,0x44,0x48,0x4C,0x50,0x54,0x58,0x5C,0x60,0x84,0x88,0x8C,0x90,0x94,0x98,0x9C,0xA0,0xA4,0xA8,0x4C4,0x4C8,0x4D0];
        let mut comparisons=0;
        for bits in [8,16] {
            let a=call(original,0x7C,&[0,0]); let b=call(owned,0x7C,&[0,0]);
            for args in [[7,5,bits,1],[7,5,bits,1],[7,5,0,1],[9,3,bits,1]] {
                assert_eq!(call(a,4,&args),call(b,4,&args)); comparisons+=1;
                for at in fields { assert_eq!(field(a,at),field(b,at),"state depth {bits} field {at:#x}, create {args:?}"); comparisons+=1; }
                for clip in [rect(-3,-2,5,3),rect(90,90,100,100),rect(0,0,args[0] as i32,args[1] as i32)] {
                    assert_eq!(call(a,0x34,&[addr(&clip)]),call(b,0x34,&[addr(&clip)])); comparisons+=1;
                    for at in [0x44,0x48,0x4C,0x50] { assert_eq!(field(a,at),field(b,at)); comparisons+=1; }
                }
                for slot in [0x10,0x18,0x20] {
                    assert_ne!(call(a,slot,&[]),0); assert_ne!(call(b,slot,&[]),0);
                    assert_eq!(field(a,0x4C8),field(b,0x4C8)); comparisons+=1;
                }
                for n in [1,0,5] { assert_eq!(call(a,0x24,&[n]),call(b,0x24,&[n])); assert_eq!(field(a,0x4C8),field(b,0x4C8)); comparisons+=2; }
                for slot in [0xD8,0xDC,0xE0,0xC0] { assert_eq!(call(a,slot,&[]),call(b,slot,&[])); comparisons+=1; }
                assert_eq!(call(a,0x34,&[0]),call(b,0x34,&[0])); comparisons+=1;
                assert_eq!(call(a,0x58,&[0,0,1]),call(b,0x58,&[0,0,1])); comparisons+=1;
            }
            // Unsupported-depth errors precede data policy, even on ordinary
            // canvases. Restore the ABI field before destruction.
            let whole=rect(0,0,9,3);
            *((a+0x24) as *mut u32)=24; *((b+0x24) as *mut u32)=24;
            assert_eq!(call(a,0x58,&[addr(&whole),0,1]),call(b,0x58,&[addr(&whole),0,1])); comparisons+=1;
            assert_eq!(call(a,0x38,&[a,0,0,0,0,1,1,0]),call(b,0x38,&[b,0,0,0,0,1,1,0])); comparisons+=1;
            *((a+0x24) as *mut u32)=bits; *((b+0x24) as *mut u32)=bits;
            call(a,8,&[]); call(b,8,&[]);
            assert_eq!(field(a,0x4BC),field(b,0x4BC)); comparisons+=1;
            assert_eq!(call(a,0x40,&[a,addr(&whole),addr(&whole)]),call(b,0x40,&[b,addr(&whole),addr(&whole)])); comparisons+=1;
            assert_eq!(call(a,4,&[9,3,bits,1]),call(b,4,&[9,3,bits,1])); comparisons+=1;
            assert_eq!(field(a,0x4BC),field(b,0x4BC)); comparisons+=1;
            call(a,0,&[1]); call(b,0,&[1]);
        }
        println!("image defined state/error comparisons: {comparisons} pairs, zero differences; handles and freed-pointer values excluded");
    }
}

#[test]
fn image_indexed_copies_replacements_and_state_match_original_binary() {
    let path = std::env::var("CIV3_JGL_DLL").expect("original jgl oracle is required");
    let path = std::ffi::CString::new(path).unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let export = GetProcAddress(module, c"get_graphsy_object_ptr".as_ptr().cast()).unwrap();
        let original = std::mem::transmute::<unsafe extern "system" fn() -> isize, extern "C" fn() -> u32>(export)();
        jgl_ffi::own::install(Box::new(|| jgl::Hooks { policy: Box::new(Data), ..Default::default() }), Box::new(jgl::hooks::NoWindowHooks)).unwrap();
        let owned = jgl_ffi::own::new_graphics();
        let mut calls = 0;
        let mut checked_bytes = 0;
        let mut differences = Vec::new();
        for bits in [8, 16] {
            for (w,h) in [(3,5),(8,6),(13,9)] {
                let mut owners = [[0u32; 8]; 4];
                let images: Vec<_> = [original,original,owned,owned].into_iter().enumerate().map(|(i,g)| {
                    let image = call(g, 0x7C, &[addr(&owners[i]),0]);
                    owners[i][1] = image;
                    assert_eq!(call(image,4,&[w,h,if i % 2 == 0 {8} else {bits},1]),0);
                    image
                }).collect();
                // First operation after create, before any pixel lock.
                let whole = rect(0,0,w as i32,h as i32);
                for image in [images[1],images[3]] { assert_eq!(call(image,0x58,&[addr(&whole),0,3]),0); }
                let expected=bytes(images[1]);
                checked_bytes+=expected.len(); calls+=1;
                if bytes(images[3])!=expected { differences.push(format!("first replace {bits} {w}x{h}")); }
                let table_orig=call(original,0x78,&[0]);
                let table_own=call(owned,0x78,&[0]);
                call(table_orig,4,&[]); call(table_own,4,&[]);
                call(images[0],0xEC,&[table_orig]); call(images[2],0xEC,&[table_own]);
                for clip in [whole,rect(1,1,w as i32-1,h as i32-1),rect(-2,-3,w as i32+1,h as i32+1)] {
                    for (sx,sy,dx,dy,cw,ch) in [(0,0,0,0,w as i32,h as i32),(-1,-1,0,0,w as i32,h as i32),(0,0,-1,-1,w as i32,h as i32),(1,1,1,2,w as i32,h as i32),(0,0,0,0,0,0)] {
                        for (i,&image) in images.iter().enumerate() { seed(image,i%2); call(image,0x34,&[addr(&clip)]); }
                        let a=[images[1],sx as u32,sy as u32,dx as u32,dy as u32,cw as u32,ch as u32,3];
                        let b=[images[3],sx as u32,sy as u32,dx as u32,dy as u32,cw as u32,ch as u32,3];
                        let expected_status=call(images[0],0x38,&a);
                        let actual_status=call(images[2],0x38,&b);
                        let expected=bytes(images[1]); checked_bytes+=expected.len(); calls+=1;
                        if expected_status!=actual_status || bytes(images[3])!=expected { differences.push(format!("copy {bits} {w}x{h} clip {clip:?} {sx},{sy}->{dx},{dy} size {cw},{ch}: status {expected_status}/{actual_status}")); }
                    }
                    for (from,to) in [(0,255),(1,255),(0xFD,0xFF),(0x0100,0x1234),(0x0302,0x5678)] {
                        for &image in &[images[1],images[3]] { seed(image,0); call(image,0x34,&[addr(&clip)]); }
                        let a=call(images[1],0x58,&[addr(&whole),from,to]);
                        let b=call(images[3],0x58,&[addr(&whole),from,to]);
                        let expected=bytes(images[1]); checked_bytes+=expected.len(); calls+=1;
                        if a!=b || bytes(images[3])!=expected { differences.push(format!("replace {bits} {w}x{h} {from:#x}->{to:#x} clip {clip:?}: status {a}/{b}")); }
                    }
                }
                if bits==8 {
                    for (sx,sy,dx,dy) in [(0,0,1,0),(1,0,0,0),(0,0,0,1),(0,1,0,0),(0,0,1,1)] {
                        for &image in &[images[0],images[2]] {
                            let len=bytes(image).len();
                            let p=call(image,0x10,&[]);
                            let data=std::slice::from_raw_parts_mut(p as *mut u8,len);
                            data.fill(3);
                            data[0]=1;
                            call(image,0x24,&[1]); call(image,0xC4,&[]);
                        }
                        let a=call(images[0],0x38,&[images[0],sx,sy,dx,dy,w-1,h-1,3]);
                        let b=call(images[2],0x38,&[images[2],sx,sy,dx,dy,w-1,h-1,3]);
                        let expected=bytes(images[0]); checked_bytes+=expected.len(); calls+=1;
                        if a!=b || bytes(images[2])!=expected { differences.push(format!("overlap {w}x{h} {sx},{sy}->{dx},{dy}: status {a}/{b}")); }
                    }
                }
                for &image in &images { call(image,0,&[1]); }
                call(table_orig,0,&[1]); call(table_own,0,&[1]);
            }
        }
        state_and_errors(original,owned);
        call(original,0x0C,&[]); call(owned,0x0C,&[]);
        println!("image comparisons: {calls} calls, {checked_bytes} bytes, {} differences",differences.len());
        for d in &differences { println!("DIFFERENCE {d}"); }
        assert!(differences.is_empty());
    }
}

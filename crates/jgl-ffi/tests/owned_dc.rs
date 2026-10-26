//! Original-binary checks for caller-written navigator data. No game process,
//! shared game DLL replacement, or viewer-pixel read-back is involved.
#![cfg(all(windows, target_arch = "x86"))]

use jgl_ffi::layout::Rect32;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};


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


#[repr(C)]
struct Startup { version:u32, callback:u32, suppress_thread:u32, suppress_codecs:u32 }

#[test]
fn c3x_gdiplus_bind_pen_draw_and_retained_lifetimes_match_original() {
    let path=std::ffi::CString::new(std::env::var("CIV3_JGL_DLL").expect("original oracle required")).unwrap();
    unsafe {
        let module=LoadLibraryA(path.as_ptr().cast()); assert!(!module.is_null());
        let export=GetProcAddress(module,c"get_graphsy_object_ptr".as_ptr().cast()).unwrap();
        let original=std::mem::transmute::<unsafe extern "system" fn()->isize,extern "C" fn()->u32>(export)();
        jgl_ffi::own::install(Box::new(jgl::Hooks::default),Box::new(jgl::hooks::NoWindowHooks)).unwrap();
        let owned=jgl_ffi::own::new_graphics();
        let gp=LoadLibraryA(c"gdiplus.dll".as_ptr().cast()); assert!(!gp.is_null());
        macro_rules! load { ($name:literal,$ty:ty) => { std::mem::transmute::<_, $ty>(GetProcAddress(gp,concat!($name,"\0").as_ptr()).unwrap()) }; }
        let startup=load!("GdiplusStartup",unsafe extern "system" fn(*mut usize,*const Startup,*mut u32)->i32);
        let shutdown=load!("GdiplusShutdown",unsafe extern "system" fn(usize));
        let create=load!("GdipCreateFromHDC",unsafe extern "system" fn(usize,*mut usize)->i32);
        let delete=load!("GdipDeleteGraphics",unsafe extern "system" fn(usize)->i32);
        let smoothing=load!("GdipSetSmoothingMode",unsafe extern "system" fn(usize,i32)->i32);
        let pen=load!("GdipCreatePen1",unsafe extern "system" fn(u32,f32,i32,*mut usize)->i32);
        let dash=load!("GdipSetPenDashStyle",unsafe extern "system" fn(usize,i32)->i32);
        let draw=load!("GdipDrawLineI",unsafe extern "system" fn(usize,usize,i32,i32,i32,i32)->i32);
        let delete_pen=load!("GdipDeletePen",unsafe extern "system" fn(usize)->i32);
        let mut token=0;
        assert_eq!(startup(&mut token,&Startup {version:1,callback:0,suppress_thread:0,suppress_codecs:0},std::ptr::null_mut()),0);
        let mut comparisons=0;
        for life in 0..16 {
            let mut owners=[[0u32;8];2];
            let images=[original,owned].into_iter().enumerate().map(|(i,g)| {
                let image=call(g,0x7C,&[addr(&owners[i]),0]); owners[i][1]=image;
                assert_eq!(call(image,4,&[64,32,16,1]),0); image
            }).collect::<Vec<_>>();
            for clip in [rect(0,0,64,32),rect(4,3,60,28),rect(-10,-4,16,8),rect(90,90,100,100)] {
                let statuses:Vec<_>=images.iter().map(|&im| call(im,0x34,&[addr(&clip)])).collect();
                assert_eq!(statuses[0],statuses[1]); comparisons+=1;
                // The logical image state is independent of its scratch bitmap.
                for at in [0x44,0x48,0x4C,0x50,0x4C4,0x4C8] { assert_eq!(field(images[0],at),field(images[1],at)); comparisons+=1; }
                let mut graphics=[0usize;2];
                for (i,&im) in images.iter().enumerate() {
                    let hdc=field(im,0x4BC); assert_ne!(hdc,0);
                    assert_eq!(call(im,0x28,&[]),hdc);
                    assert_eq!(create(hdc as usize,&mut graphics[i]),0);
                    assert_eq!(smoothing(graphics[i],4),0);
                    assert_eq!(call(im,0x2C,&[1]),0);
                    assert_ne!(windows_sys::Win32::Graphics::Gdi::GetObjectType(hdc as usize as _),0);
                }
                comparisons+=3;
                for color in [0xFF000000,0x80F800F8,0x0000F800] {
                    for width in [1.0,3.0,9.0] {
                        for dashed in [false,true] {
                            let mut pens=[0usize;2];
                            for i in 0..2 { assert_eq!(pen(color,width,0,&mut pens[i]),0); if dashed { assert_eq!(dash(pens[i],1),0); } }
                            comparisons+=1+usize::from(dashed);
                            for (x1,y1,x2,y2) in [(0,0,63,31),(-10,5,90,10),(4,3,5,3),(80,40,90,60)] {
                                let statuses=[draw(graphics[0],pens[0],x1,y1,x2,y2),draw(graphics[1],pens[1],x1,y1,x2,y2)];
                                assert_eq!(statuses[0],statuses[1],"life {life}, clip {clip:?}, line {x1},{y1}->{x2},{y2}");
                                assert_eq!(statuses[0],0); comparisons+=1;
                            }
                            assert_eq!(delete_pen(pens[0]),delete_pen(pens[1])); comparisons+=1;
                        }
                    }
                }
                assert_eq!(delete(graphics[0]),delete(graphics[1])); comparisons+=1;
            }
            for (i,&im) in images.iter().enumerate() {
                let dc=field(im,0x4BC);
                call(im,0,&[1]);
                assert_eq!(owners[i][1],0);
                assert_eq!(windows_sys::Win32::Graphics::Gdi::GetObjectType(dc as usize as _),0);
            }
            comparisons+=2;
        }
        shutdown(token);
        call(original,0x0C,&[]); call(owned,0x0C,&[]);
        println!("C3X GDI+ original/owned: {comparisons} status/state pairs, 16 lifetimes, zero differences; no pixels copied or compared");
    }
}

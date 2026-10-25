//! Deterministic original-binary window/pump contract checks. Both implementations
//! call the same substituted Win32 imports. No game, desktop or installed DLLs.
#![cfg(all(windows, target_arch = "x86"))]

use std::cell::RefCell;
use std::collections::VecDeque;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi as gdi;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows_sys::Win32::UI::WindowsAndMessaging as wm;
#[path = "support/window_abi.rs"] mod abi;

type Event = (&'static str, Vec<u32>);
#[derive(Default)]
struct Fixture {
    events: Vec<Event>,
    expected_hooks: Vec<Event>,
    actual_hooks: Vec<Event>,
    queue: VecDeque<wm::MSG>,
    procedure: wm::WNDPROC,
    register_fails: bool,
    create_fails: bool,
    no_paint_dc: bool,
    icon_fails: bool,
    metrics: u32,
    vary_dimensions: bool,
    no_window_dc: bool,
    mode_count: usize,
    virtual_list: u32,
}
thread_local! { static FIXTURE: RefCell<Fixture> = RefCell::default(); }
struct Hooks;
impl jgl::WindowHooks for Hooks {
    fn peeking(&self) { FIXTURE.with_borrow_mut(|s| s.actual_hooks.push(("peeking",vec![]))); }
    fn peeked(&self,flags: u32,id: u32,lp: u32) { FIXTURE.with_borrow_mut(|s| s.actual_hooks.push(("peeked",vec![flags,id,lp]))); }
    fn waiting(&self,on: bool) { FIXTURE.with_borrow_mut(|s| s.actual_hooks.push(("waiting",vec![on as u32]))); }
}
fn record(name: &'static str, args: &[u32]) { FIXTURE.with_borrow_mut(|s| s.events.push((name, args.to_vec()))); }
fn addr<T>(p: &T) -> u32 { p as *const T as usize as u32 }
fn addr_mut<T>(p: &mut T) -> u32 { p as *mut T as usize as u32 }
fn handle(v: u32) -> HWND { v as usize as HWND }
fn reset() { FIXTURE.with_borrow_mut(|s| *s = Fixture::default()); }

unsafe fn call(object: u32, slot: u32, args: &[u32]) -> u32 {
    unsafe {
        let table = *(object as usize as *const u32);
        let f = *((table + slot) as usize as *const u32) as usize;
        abi::function(f as u32,object,args)
    }
}

// Only the two test modules' IAT entries change. Restored on drop, including
// assertion unwind. The originals remain mapped until process exit.
struct Import { at: inproc::mem::Ptr32<u32>, old: u32 }
fn write_import(at: inproc::mem::Ptr32<u32>, value: u32) {
    use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_READWRITE};
    unsafe {
        let p = at.raw() as usize as *const std::ffi::c_void;
        let mut old = 0;
        assert_ne!(VirtualProtect(p,4,PAGE_READWRITE,&mut old),0);
        at.write(&value).unwrap();
        let mut ignored = 0;
        assert_ne!(VirtualProtect(p,4,old,&mut ignored),0);
    }
}
impl Import {
    fn replace(module: u32, dll: &str, name: &str, f: usize) -> Option<Self> {
        let at = inproc::pe::import_slot(module, dll, name).unwrap()?;
        let old = at.read().unwrap();
        write_import(at,f as u32);
        Some(Self { at, old })
    }
}
impl Drop for Import { fn drop(&mut self) { write_import(self.at,self.old); } }

unsafe extern "system" fn register(c: *const wm::WNDCLASSA) -> u16 {
    let c = unsafe { &*c };
    let name = unsafe { std::ffi::CStr::from_ptr(c.lpszClassName.cast()) };
    assert_eq!(name.to_bytes(), b"JackalClass");
    record("RegisterClass", &[c.style, c.cbClsExtra as u32, c.cbWndExtra as u32, c.hInstance as usize as u32,
        c.hIcon as usize as u32, c.hCursor as usize as u32, c.hbrBackground as usize as u32, c.lpszMenuName as usize as u32]);
    FIXTURE.with_borrow_mut(|s| { s.procedure = c.lpfnWndProc; (!s.register_fails) as u16 })
}
unsafe extern "system" fn create(ex: u32, class: *const u8, title: *const u8, style: u32, x: i32, y: i32,
    w: i32, h: i32, parent: HWND, menu: wm::HMENU, instance: windows_sys::Win32::Foundation::HINSTANCE, param: *const std::ffi::c_void) -> HWND {
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(class.cast()) }.to_bytes(), b"JackalClass");
    record("CreateWindow", &[ex, title as usize as u32, style, x as u32,y as u32,w as u32,h as u32,
        parent as usize as u32,menu as usize as u32,instance as usize as u32,param as usize as u32]);
    FIXTURE.with_borrow(|s| if s.create_fails { std::ptr::null_mut() } else { handle(0x777) })
}
unsafe extern "system" fn metrics(which: i32) -> i32 {
    record("GetSystemMetrics", &[which as u32]);
    FIXTURE.with_borrow_mut(|s| { s.metrics += 1;
        if s.vary_dimensions && s.metrics>2 { if which==0 {1600} else {900} }
        else if which == 0 { 1024 } else { 768 } })
}
unsafe extern "system" fn icon(instance: windows_sys::Win32::Foundation::HINSTANCE, id: *const u8) -> wm::HICON {
    record("LoadIcon", &[instance as usize as u32,id as usize as u32]);
    FIXTURE.with_borrow(|s| if s.icon_fails && !instance.is_null() { std::ptr::null_mut() } else { handle(0x123) })
}
unsafe extern "system" fn stock(n: i32) -> gdi::HGDIOBJ { record("GetStockObject", &[n as u32]); handle(0x456) }
unsafe extern "system" fn get_dc(h: HWND) -> gdi::HDC {
    record("GetDC", &[h as usize as u32]); FIXTURE.with_borrow(|s| if s.no_window_dc {std::ptr::null_mut()} else {handle(0x987)})
}
unsafe extern "system" fn bk(dc: gdi::HDC, mode: i32) -> i32 { record("SetBkMode", &[dc as usize as u32,mode as u32]); 2 }
unsafe extern "system" fn destroy(h: HWND) -> i32 { record("DestroyWindow", &[h as usize as u32]); 1 }
unsafe extern "system" fn unregister(_: *const u8, i: windows_sys::Win32::Foundation::HINSTANCE) -> i32 {
    record("UnregisterClass", &[i as usize as u32]); 0x31
}
unsafe extern "system" fn show(h: HWND, n: i32) -> i32 { record("ShowWindow", &[h as usize as u32,n as u32]); 0x42 }
unsafe extern "system" fn pos(h: HWND, after: HWND, x: i32,y: i32,w: i32,hgt: i32,flags: u32) -> i32 {
    record("SetWindowPos", &[h as usize as u32, after as usize as u32,x as u32,y as u32,w as u32,hgt as u32,flags]); 0x52
}
unsafe extern "system" fn cursor(p: *mut POINT) -> i32 { record("GetCursorPos", &[]); unsafe { *p = POINT{x:81,y:-14}; } 1 }
unsafe extern "system" fn to_client(h: HWND,p: *mut POINT) -> i32 {
    record("ScreenToClient", &[h as usize as u32,unsafe { (*p).x } as u32,unsafe { (*p).y } as u32]);
    unsafe { (*p).x -= 12; (*p).y += 9; } 1
}
unsafe extern "system" fn rect(h: HWND,r: *mut RECT) -> i32 {
    record("GetWindowRect", &[h as usize as u32]); unsafe { *r = RECT{left:11,top:12,right:1035,bottom:780}; } 0x61
}
unsafe extern "system" fn invalidate(h: HWND,r: *const RECT,erase: i32) -> i32 {
    let mut args = vec![h as usize as u32,erase as u32,r.is_null() as u32];
    if !r.is_null() { let r = unsafe { &*r }; args.extend([r.left as u32,r.top as u32,r.right as u32,r.bottom as u32]); }
    record("InvalidateRect", &args); 0x71
}
unsafe extern "system" fn visible(_: HWND) -> i32 { 1 }
unsafe extern "system" fn begin(h: HWND,p: *mut gdi::PAINTSTRUCT) -> gdi::HDC {
    record("BeginPaint", &[h as usize as u32]);
    unsafe { (*p).rcPaint = RECT{left:1,top:2,right:34,bottom:56}; }
    FIXTURE.with_borrow(|s| if s.no_paint_dc { std::ptr::null_mut() } else { handle(0x987) })
}
unsafe extern "system" fn end(h: HWND,p: *const gdi::PAINTSTRUCT) -> i32 {
    let r=unsafe { (*p).rcPaint };
    record("EndPaint", &[h as usize as u32,r.left as u32,r.top as u32,r.right as u32,r.bottom as u32]); 1
}
unsafe extern "system" fn default(h: HWND,m: u32,w: WPARAM,l: LPARAM) -> LRESULT {
    record("DefWindowProc", &[h as usize as u32,m,w as u32,l as u32]); 0x5678_9ABC_u32 as i32 as isize
}
unsafe extern "system" fn select(dc: gdi::HDC,p: gdi::HPALETTE,b: i32) -> gdi::HPALETTE {
    record("SelectPalette", &[dc as usize as u32,p as usize as u32,b as u32]); std::ptr::null_mut()
}
unsafe extern "system" fn realize(dc: gdi::HDC) -> u32 { record("RealizePalette", &[dc as usize as u32]); 3 }
unsafe extern "system" fn release(h: HWND,dc: gdi::HDC) -> i32 { record("ReleaseDC", &[h as usize as u32,dc as usize as u32]); 1 }
unsafe extern "system" fn change(_: *const gdi::DEVMODEA,_: u32) -> i32 { 0 } // Accepted display suppression.
const DISPLAY_MODES: [[u32;4];5] = [[1024,768,16,60],[800,600,8,60],[1024,768,16,75],[1024,768,16,75],[1920,1080,32,60]];
unsafe extern "system" fn enumerate(_: *const u8,index: u32,out: *mut gdi::DEVMODEA) -> i32 {
    record("EnumDisplaySettings", &[index]);
    let mode = if index==gdi::ENUM_CURRENT_SETTINGS {Some([1280,1024,32,60])}
        else { FIXTURE.with_borrow(|s| ((index as usize)<s.mode_count).then(|| DISPLAY_MODES[index as usize])) };
    let Some([width,height,bits,frequency])=mode else {return 0};
    unsafe {
        if index!=gdi::ENUM_CURRENT_SETTINGS { assert_eq!((*out).dmSize,148); assert_eq!((*out).dmDriverExtra,0); }
        let mut native: gdi::DEVMODEA=std::mem::zeroed(); native.dmSize=148;
        native.dmPelsWidth=width; native.dmPelsHeight=height; native.dmBitsPerPel=bits; native.dmDisplayFrequency=frequency;
        std::ptr::copy_nonoverlapping((&native as *const gdi::DEVMODEA).cast::<u8>(),out.cast::<u8>(),148);
    }
    1
}
unsafe extern "system" fn peek(p: *mut wm::MSG,h: HWND,min: u32,max: u32,flags: u32) -> i32 {
    record("PeekMessage", &[h as usize as u32,min,max,flags]);
    FIXTURE.with_borrow_mut(|s| {
        s.expected_hooks.push(("peeking",vec![]));
        let found = s.queue.iter().position(|m| (h.is_null() || h == m.hwnd) &&
            (m.message == wm::WM_QUIT || (min == 0 && max == 0) || (min..=max).contains(&m.message)));
        let Some(i) = found else { return 0 };
        let m=s.queue[i];
        unsafe { *p = m; }
        if flags & 1 != 0 {
            s.queue.remove(i);
            s.expected_hooks.push(("peeked",vec![flags,m.message,m.lParam as u32]));
        }
        1
    })
}
unsafe extern "system" fn translate(m: *const wm::MSG) -> i32 {
    let m = unsafe { &*m }; record("TranslateMessage", &[m.message,m.wParam as u32,m.lParam as u32]); 1
}
unsafe extern "system" fn dispatch(m: *const wm::MSG) -> LRESULT {
    let m = unsafe { &*m }; record("DispatchMessage", &[m.hwnd as usize as u32,m.message,m.wParam as u32,m.lParam as u32]); 0
}
unsafe extern "system" fn wait() -> i32 {
    record("WaitMessage", &[]);
    FIXTURE.with_borrow_mut(|s| s.expected_hooks.extend([("waiting",vec![1]),("waiting",vec![0])]));
    0x81
}

macro_rules! controller {
    ($name:ident,$slot:expr,$($a:ident),*) => {
        extern "thiscall" fn $name(_this: u32,$($a: u32),*) -> u32 {
            record("Controller", &[$slot,$($a),*]); 0xABCD_0000 | $slot
        }
    };
}
controller!(size,0x00,x,y);
extern "thiscall" fn paint(_: u32,r: u32) -> u32 {
    let p=r as usize as *mut RECT;
    let r=unsafe { *p };
    record("Controller", &[4,r.left as u32,r.top as u32,r.right as u32,r.bottom as u32]);
    // Original jgl passes a writable copy, independent of PAINTSTRUCT.
    unsafe { (*p).left=0x1234; }
    0
}
controller!(hit,0x08,x,y); controller!(key,0x0C,vk,down); controller!(character,0x10,c);
controller!(motion,0x14,x,y); controller!(wheel,0x18,delta,x,y);
controller!(left_down,0x1C,double,x,y); controller!(left_up,0x20,x,y);
controller!(right_down,0x24,double,x,y); controller!(right_up,0x28,x,y);
controller!(activate,0x2C,state,minimized); controller!(timer,0x30,id); controller!(message402,0x34,);
controller!(owner_create,0x100,w,h,bits,flag); controller!(owner_clear,0x108,);
extern "C" fn copy_data(w: u32,l: u32) -> u32 { record("CopyData", &[w,l]); 0x8765_ABCD }
extern "thiscall" fn non_boolean(_: u32) -> u32 { record("Virtual128", &[]); 0xCAFE }
extern "thiscall" fn virtual_mode(_: u32,w: u32,h: u32,bits: u32) -> u32 { record("Virtual58", &[w,h,bits]);0xBEEF }
extern "thiscall" fn virtual_list(_: u32) -> u32 { record("Virtual68", &[]); FIXTURE.with_borrow(|s| s.virtual_list) }
extern "thiscall" fn virtual_index(_: u32,index: u32) -> u32 { record("Virtual64", &[index]); 0xF00D }

#[derive(Debug, PartialEq, Eq)]
struct Observation { answer: u32, events: Vec<Event>, queue: Vec<[u32;4]>, fields: Vec<u32>, out: Vec<u32> }
unsafe fn observe(g: u32, run: impl FnOnce(u32) -> (u32, Vec<u32>)) -> Observation {
    let (answer,out) = run(g);
    // Handles are identical fixture tokens. The rest of the object includes
    // saved display state, deliberately outside this window/pump comparison.
    let fields = unsafe { std::slice::from_raw_parts((g+0x12C) as usize as *const u32,8) }.iter().enumerate()
        .filter_map(|(i,&v)| (i!=4).then_some(v)).collect(); // +13C is uninitialized and unused in jgl.
    if g==jgl_ffi::own::graphics::object() {
        FIXTURE.with_borrow(|s| assert_eq!(s.actual_hooks,s.expected_hooks,"every owned peek/wait must report its hooks"));
    }
    FIXTURE.with_borrow_mut(|s| Observation { answer,events: std::mem::take(&mut s.events),
        queue: s.queue.iter().map(|m| [m.hwnd as usize as u32,m.message,m.wParam as u32,m.lParam as u32]).collect(),fields,out })
}
fn message(id: u32,window: u32,serial: u32) -> wm::MSG {
    wm::MSG { hwnd:handle(window),message:id,wParam:serial as usize,lParam:(serial*7) as isize,time:serial,
        pt:POINT{x:0,y:0} }
}

#[test]
fn window_and_pump_contracts_match_original_binary() {
    let Ok(path) = std::env::var("CIV3_JGL_DLL") else { eprintln!("skipped: CIV3_JGL_DLL required"); return };
    let path = std::ffi::CString::new(path).unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast()); assert!(!module.is_null());
        let f = GetProcAddress(module,c"get_graphsy_object_ptr".as_ptr().cast()).unwrap();
        let original = std::mem::transmute::<_,extern "C" fn()->u32>(f)();
        jgl_ffi::own::install(Box::new(jgl::Hooks::default),Box::new(Hooks)).unwrap();
        let owned = jgl_ffi::own::new_graphics();
        let modules = [module as usize as u32,inproc::pe::main_module()];
        let mut imports = Vec::new();
        macro_rules! patch { ($dll:literal,$name:literal,$f:ident) => {
            for m in modules {
                imports.push(Import::replace(m,$dll,$name,$f as *const () as usize)
                    .unwrap_or_else(|| panic!("missing {m:x}: {}!{}",$dll,$name)));
            }
        }; }
        patch!("USER32.dll","RegisterClassA",register); patch!("USER32.dll","CreateWindowExA",create);
        patch!("USER32.dll","GetSystemMetrics",metrics); patch!("USER32.dll","LoadIconA",icon);
        patch!("GDI32.dll","GetStockObject",stock); patch!("USER32.dll","GetDC",get_dc);
        patch!("GDI32.dll","SetBkMode",bk); patch!("USER32.dll","DestroyWindow",destroy);
        patch!("USER32.dll","UnregisterClassA",unregister); patch!("USER32.dll","ShowWindow",show);
        patch!("USER32.dll","SetWindowPos",pos); patch!("USER32.dll","GetCursorPos",cursor);
        patch!("USER32.dll","ScreenToClient",to_client); patch!("USER32.dll","GetWindowRect",rect);
        patch!("USER32.dll","InvalidateRect",invalidate);
        imports.push(Import::replace(modules[1],"USER32.dll","IsWindowVisible",visible as *const () as usize).unwrap());
        patch!("USER32.dll","BeginPaint",begin); patch!("USER32.dll","EndPaint",end);
        patch!("USER32.dll","DefWindowProcA",default); patch!("GDI32.dll","SelectPalette",select);
        patch!("GDI32.dll","RealizePalette",realize); patch!("USER32.dll","ReleaseDC",release);
        imports.push(Import::replace(modules[0],"USER32.dll","ChangeDisplaySettingsA",change as *const () as usize).unwrap());
        patch!("USER32.dll","EnumDisplaySettingsA",enumerate);
        patch!("USER32.dll","PeekMessageA",peek); patch!("USER32.dll","TranslateMessage",translate);
        patch!("USER32.dll","DispatchMessageA",dispatch); patch!("USER32.dll","WaitMessage",wait);
        let table = [size as usize,paint as usize,hit as usize,key as usize,character as usize,motion as usize,
            wheel as usize,left_down as usize,left_up as usize,right_down as usize,right_up as usize,
            activate as usize,timer as usize,message402 as usize];
        let controller = [addr(&table)];
        let owner_table = [owner_create as usize,0,owner_clear as usize];
        let owner = [addr(&owner_table)];
        inproc::hidden::set_hiding(false);
        let mut differences = 0;
        let mut checked = 0;
        let mut compare = |label: &str,a: Observation,b: Observation| {
            checked += 1;
            if a != b { differences += 1; println!("DIFFERENCE {label}\noriginal={a:?}\nowned={b:?}"); }
            else { println!("MATCH {label} {a:?}"); }
        };
        for (label,register_fails,create_fails,icon_fails,vary_dimensions,no_window_dc) in [("create-register-failure",true,false,false,false,false),
            ("create-window-failure",false,true,false,false,false),("create-icon-fallback",false,false,true,false,false),
            ("create-changing-screen",false,false,false,true,false),("create-no-dc",false,false,false,false,true),
            ("create",false,false,false,false,false)] {
            let run = |g| {
                reset(); FIXTURE.with_borrow_mut(|s| { s.register_fails=register_fails;s.create_fails=create_fails;s.icon_fails=icon_fails;
                    s.vary_dimensions=vary_dimensions;s.no_window_dc=no_window_dc; });
                // Use the same valid owner/controller on both paths.
                let r = call(g,4,&[addr(&controller),c"contract".as_ptr() as usize as u32,addr(&owner),0x12345]);
                observe(g, |_| (r,vec![]))
            };
            let a=run(original); let b=run(owned); compare(label,a,b);
        }
        // Captured by the final RegisterClass stub; both routines remain callable.
        let owned_proc = FIXTURE.with_borrow(|s| s.procedure.unwrap());
        let original_proc: unsafe extern "system" fn(HWND,u32,WPARAM,LPARAM)->LRESULT =
            std::mem::transmute((module as usize)+0x3C730);
        for slot in [0x10,0x14,0x18,0x1C,0x20,0x24,0x28,0x2C,0x30,0x34,0x38,0x3C,0x48,0xA8,0xAC,0xB0,0xB4,0x154] {
            let run = |g| { reset(); observe(g,|g| {
                let mut out=[0u32;4];
                let args = match slot { 0x28=>vec![3,4,320,240],0x2C|0x34=>vec![addr_mut(&mut out[0]),addr_mut(&mut out[1])],
                    0x30=>vec![addr(&out)],0x38=>vec![addr_mut(&mut out)],0x154=>vec![copy_data as *const () as usize as u32],_=>vec![] };
                let r=call(g,slot,&args); (r,std::mem::take(&mut out).to_vec())
            }) };
            compare(&format!("slot-{slot:03x}"),run(original),run(owned));
        }
        let packed = |x: i16,y: i16| (x as u16 as u32 | ((y as u16 as u32)<<16)) as i32 as isize;
        for (id,wp,lp) in [(5,0,packed(-3,30000)),(6,1,0),(6,0x1_0002,0),(0xF,0,0),(0x4A,0x123,0x234),
            (0x84,0,packed(-22,35)),(0x100,0x41,0),(0x101,0x41,0),(0x104,0x12,0),(0x105,0x12,0),
            (0x102,0xE9,0),(0x112,0xF140,0),(0x112,0xF141,0),(0x200,0,packed(-20,-8)),
            (0x201,0,packed(5,6)),(0x202,0,packed(5,6)),(0x203,0,packed(5,6)),(0x204,0,packed(5,6)),
            (0x205,0,packed(5,6)),(0x206,0,packed(5,6)),(0x207,0,0),(0x20A,0xFF88_0004u32,packed(-3,4)),
            (0x218,0,0),(0x218,7,0),(0x30F,0,0),(0x311,0x777,0),(0x311,0x888,0),(0x401,0x55,0),(0x10,0,0),
            (0x100,0x61,0),(0x100,0x6F,0),(0x100,0x70,0)] {
            for no_dc in [false,true].into_iter().take(if id==0xF {2} else {1}) {
                let run = |g,proc: unsafe extern "system" fn(HWND,u32,WPARAM,LPARAM)->LRESULT| {
                    reset(); FIXTURE.with_borrow_mut(|s| { s.no_paint_dc=no_dc;
                        if id==0x100 { s.queue.extend([message(0x102,0x777,1),message(0x102,0x888,2)]); } });
                    observe(g, |_| (abi::function(proc as usize as u32,0,&[0x777,id,wp as u32,lp as u32]),vec![]))
                };
                compare(&format!("message-{id:03x}-{wp:x}-no-dc-{no_dc}"),run(original,original_proc),run(owned,owned_proc));
            }
        }
        // Every branch of both switch tables, every default message below
        // WM_USER+0x100, and signed/unsigned word boundary arguments.
        for id in 0..=0x500 {
            for (wp,lp) in [(0,0),(u32::MAX,0x8000_FFFFu32)] {
                let run = |g,proc: unsafe extern "system" fn(HWND,u32,WPARAM,LPARAM)->LRESULT| {
                    reset(); observe(g, |_| (abi::function(proc as usize as u32,0,&[0x777,id,wp,lp]),vec![]))
                };
                compare(&format!("sweep-{id:03x}-{wp:x}"),run(original,original_proc),run(owned,owned_proc));
            }
        }
        for slot in (0x120..=0x150).step_by(4) {
            for (case,ids) in [vec![],vec![0x401],vec![0x402],vec![0x100,0x102,0x200,0x401,0x400,0xF],
                vec![0x204,0x205,0x201,0x202,0x206],vec![0x201,0x200,0x202],vec![0x12],vec![0x109,0x401],
                vec![0xFF,0x100,0x108,0x109,0x1FF,0x200,0x20A,0x20B,0xFFFF,0x10000],
                vec![0x201,0x202,0x202,0x401,0x401,0x402]].into_iter().enumerate() {
                let run = |g| { reset(); FIXTURE.with_borrow_mut(|s| { for (i,&id) in ids.iter().enumerate() {
                    s.queue.push_back(message(id,if case==9 && i==1 {0x888} else {0x777},i as u32+1));
                    // Priority ranges, not chronological order, decide +12C.
                    s.queue.back_mut().unwrap().time=100-i as u32;
                } }); observe(g,|g| (call(g,slot,&[]),vec![])) };
                compare(&format!("pump-{slot:03x}-{case}"),run(original),run(owned));
            }
        }
        // +124 must preserve a nonzero result of its virtual +128 call.
        let mut returns = Vec::new();
        for g in [original,owned] {
            let mut vt=std::slice::from_raw_parts(*(g as *const u32) as *const u32,86).to_vec();
            vt[0x128/4]=non_boolean as *const () as usize as u32;
            *(g as *mut u32)=vt.as_ptr() as usize as u32;
            reset(); let o=observe(g,|g| (call(g,0x124,&[]),vec![]));
            // Restore before dropping the copied table. Its original pointer is
            // recovered from the module or the installed owned tables.
            *(g as *mut u32)=if g==original { module as usize as u32+0x685F8 } else { jgl_ffi::own::tables().unwrap()[0] };
            returns.push(o);
        }
        let b=returns.pop().unwrap(); let a=returns.pop().unwrap();
        compare("virtual-pump-return",a,b);
        for slot in jgl::slots::Class::Graphics.slots().iter().filter(|s| s.group==jgl::slots::Group::Stub) {
            let args=vec![0x1234;slot.stack as usize/4];
            let run=|g| { reset(); observe(g,|g| (call(g,slot.offset as u32,&args),vec![])) };
            compare(&format!("stub-{:03x}",slot.offset),run(original),run(owned));
        }
        for (slot,args) in [(0x4C,vec![]),(0x98,vec![0x1234])] {
            let run=|g| { reset(); observe(g,|g| { call(g,slot,&args); (0,vec![]) }) };
            compare(&format!("undefined-eax-no-effect-{slot:03x}"),run(original),run(owned));
        }
        for count in [0,1,5,2,0,5] {
            let run=|g| {
                reset(); FIXTURE.with_borrow_mut(|s| s.mode_count=count);
                observe(g,|g| {
                    let list=call(g,0x68,&[]); assert_ne!(list,0);
                    let fields=std::slice::from_raw_parts(list as *const u32,5);
                    let mut out=fields[2..].to_vec();
                    if fields[3]!=0 { out.extend_from_slice(std::slice::from_raw_parts(fields[1] as *const u32,fields[3] as usize*5)); }
                    (0,out) // The two stable list addresses designate different allocations.
                })
            };
            compare(&format!("display-list-{count}"),run(original),run(owned));
        }
        for (slot,args) in [(0x50,vec![1024,768,16]),(0x64,vec![0]),(0x64,vec![2]),
            (0x64,vec![5]),(0x60,vec![1200,800,16,1]),(0x6C,vec![640,480]),(0x70,vec![])] {
            let run=|g| { reset(); FIXTURE.with_borrow_mut(|s| s.mode_count=5); observe(g,|g| (call(g,slot,&args),vec![])) };
            compare(&format!("mode-{slot:03x}-{args:?}"),run(original),run(owned));
        }
        let mut returns=Vec::new();
        for g in [original,owned] {
            let old=*(g as *const u32);
            let mut vt=std::slice::from_raw_parts(old as *const u32,86).to_vec();
            vt[0x58/4]=virtual_mode as *const () as usize as u32;
            *(g as *mut u32)=vt.as_ptr() as usize as u32;
            reset(); returns.push(observe(g,|g| (call(g,0x54,&[16]),vec![])));
            *(g as *mut u32)=old;
        }
        let b=returns.pop().unwrap(); let a=returns.pop().unwrap(); compare("set-depth-virtual58",a,b);
        for args in [[1024,768,16],[320,200,8]] {
            let entries=[[1024u32,768,1024,16,60],[1024,768,1024,16,75],[1024,768,1024,16,75]];
            let header=[0,addr(&entries),3,3,1];
            let mut returns=Vec::new();
            for g in [original,owned] {
                let old=*(g as *const u32);
                let mut vt=std::slice::from_raw_parts(old as *const u32,86).to_vec();
                vt[0x68/4]=virtual_list as *const () as usize as u32;
                vt[0x64/4]=virtual_index as *const () as usize as u32;
                *(g as *mut u32)=vt.as_ptr() as usize as u32;
                reset(); FIXTURE.with_borrow_mut(|s| s.virtual_list=addr(&header));
                returns.push(observe(g,|g| (call(g,0x50,&args),vec![])));
                *(g as *mut u32)=old;
            }
            let b=returns.pop().unwrap(); let a=returns.pop().unwrap(); compare(&format!("fastest-virtual-list-{args:?}"),a,b);
        }
        // The already accepted display wrapper handles valid game requests.
        // Check its raw fallback bodies separately, including OS-order quirks
        // and invalid-dimension keep requests that still delegate to +60.
        for frequencies in [[60,85,75],[85,60,75],[60,75,75],[u32::MAX,60,75]] {
            for width in [1024,320] {
                for slot in [0x58,0x5C] {
                    for refresh in [60,75,85,u32::MAX].into_iter().take(if slot==0x58 {1} else {4}) {
                        let entries=frequencies.map(|f| [1024u32,768,0xBEEF,16,f]);
                        let header=[0,addr(&entries),3,3,1]; let mut returns=Vec::new();
                        for g in [original,owned] {
                            let old=*(g as *const u32);
                            let mut vt=std::slice::from_raw_parts(old as *const u32,86).to_vec();
                            vt[0x68/4]=virtual_list as *const () as usize as u32;
                            vt[0x64/4]=virtual_index as *const () as usize as u32;
                            *(g as *mut u32)=vt.as_ptr() as usize as u32;
                            reset(); FIXTURE.with_borrow_mut(|s| s.virtual_list=addr(&header));
                            let args=if slot==0x58 {vec![width,768,16]} else {vec![width,768,16,0,refresh]};
                            returns.push(observe(g,|g| {
                                let result=if g==original {call(g,slot as u32,&args)}
                                    else {abi::function(jgl_ffi::own::graphics::slot(slot),g,&args)};
                                (result,vec![])
                            }));
                            *(g as *mut u32)=old;
                        }
                        let b=returns.pop().unwrap(); let a=returns.pop().unwrap();
                        compare(&format!("raw-mode-{slot:x}-{width}-{refresh}-{frequencies:?}"),a,b);
                    }
                }
            }
        }
        for keep in [1,0x100,0x101] {
            for width in [1024,u32::MAX] {
                let args=[width,768,16,keep,60];
                let run=|g| { reset(); FIXTURE.with_borrow_mut(|s| s.mode_count=5); observe(g,|g| {
                    let result=if g==original {call(g,0x5C,&args)}
                        else {abi::function(jgl_ffi::own::graphics::slot(0x5C),g,&args)};
                    (result,vec![])
                }) };
                compare(&format!("raw-refresh-keep-{width}-{keep}"),run(original),run(owned));
            }
        }
        // Shutdown clears the owner and window but retains the DC and dimensions.
        let run = |g| { reset(); observe(g,|g| (call(g,8,&[]),vec![])) };
        compare("destroy",run(original),run(owned));
        // Constructor 3C720 resets the process-global COPYDATA callback.
        call(original,0x154,&[copy_data as *const () as usize as u32]);
        call(owned,0x154,&[copy_data as *const () as usize as u32]);
        let original_new=std::mem::transmute::<_,extern "C" fn()->u32>(f)();
        let owned_new=jgl_ffi::own::new_graphics();
        let run = |g,proc: unsafe extern "system" fn(HWND,u32,WPARAM,LPARAM)->LRESULT| {
            reset(); observe(g, |_| (abi::function(proc as usize as u32,0,&[0x777,0x4A,0x123,0x234]),vec![]))
        };
        compare("constructor-resets-copydata",run(original_new,original_proc),run(owned_new,owned_proc));
        drop(imports);
        println!("window/pump comparison: {checked} observations, {differences} differences");
        assert_eq!(differences,0);
    }
}

//! Real Win32 messages and hidden painting, on tools/test-jgl-window's private
//! Xvfb/Wine prefix. This is not the integrated game's input acceptance test.
#![cfg(all(windows, target_arch = "x86"))]
use std::cell::RefCell;
use std::sync::atomic::{AtomicU32,Ordering};
use windows_sys::Win32::Foundation::{HWND,RECT};
use windows_sys::Win32::System::LibraryLoader::{LoadLibraryA,GetProcAddress};
use windows_sys::Win32::UI::WindowsAndMessaging as wm;
#[path = "support/window_abi.rs"] mod abi;

thread_local! { static EVENTS: RefCell<Vec<Vec<u32>>> = RefCell::default(); }
static WINDOW: AtomicU32 = AtomicU32::new(0);
static WAKE_ON_WAIT: AtomicU32 = AtomicU32::new(0);
static NESTED_GRAPHICS: AtomicU32 = AtomicU32::new(0);
fn addr<T>(value: &T) -> u32 { value as *const T as usize as u32 }
fn addr_mut<T>(value: &mut T) -> u32 { value as *mut T as usize as u32 }
fn handle(value: u32) -> HWND { value as usize as HWND }
fn record(event: &[u32]) { EVENTS.with_borrow_mut(|s| s.push(event.to_vec())); }
fn take() -> Vec<Vec<u32>> { EVENTS.with_borrow_mut(std::mem::take) }

macro_rules! method {
    ($name:ident,$slot:expr,$($a:ident),*) => {
        extern "thiscall" fn $name(_: u32,$($a: u32),*) -> u32 { record(&[$slot,$($a),*]); 0 }
    };
}
method!(size,0,x,y); method!(hit,8,x,y); method!(key,12,vk,down); method!(character,16,c);
method!(motion,20,x,y); method!(wheel,24,delta,x,y); method!(left_down,28,double,x,y);
method!(left_up,32,x,y); method!(right_down,36,double,x,y); method!(right_up,40,x,y);
method!(activate,44,state,minimized); method!(message402,52,);
extern "thiscall" fn timer(_: u32,id: u32) -> u32 {
    record(&[48,id]);
    if id==0xBB {
        // Deliberately re-enter the real queue from a controller callback.
        // The outer ABI probe spans this whole stack. Do not recursively use
        // its single observation buffer for the inner zero-argument call.
        let g=NESTED_GRAPHICS.load(Ordering::SeqCst);
        let answer=unsafe {
            let table=*(g as *const u32);
            let f=*((table+0x128) as *const usize);
            std::mem::transmute::<usize,extern "thiscall" fn(u32)->u32>(f)(g)
        };
        record(&[0x2000,answer]);
    }
    0
}
method!(owner_create,256,w,h,bits,flag); method!(owner_clear,264,);
#[repr(C)]
struct CopyData { data: usize, length: u32, bytes: *const u8 }
extern "thiscall" fn paint(_: u32,r: u32) -> u32 {
    let r=unsafe { &*(r as *const RECT) }; record(&[4,r.left as u32,r.top as u32,r.right as u32,r.bottom as u32]); 0
}
extern "C" fn copy_data(w: u32,l: u32) -> u32 {
    // A real WM_COPYDATA carries a validated COPYDATASTRUCT, not an arbitrary
    // integer lParam. Normalize its address by recording the payload instead.
    let data=unsafe { &*(l as *const CopyData) };
    let bytes=unsafe { std::slice::from_raw_parts(data.bytes,data.length as usize) };
    let mut event=vec![0x154,w,data.data as u32,data.length];
    event.extend(bytes.iter().map(|&v|v as u32)); record(&event); 0xABCD_1234
}

unsafe fn call(object: u32,slot: u32,args: &[u32]) -> u32 {
    unsafe { let table=*(object as *const u32); let f=*((table+slot) as *const u32); abi::function(f,object,args) }
}

struct Hooks;
impl jgl::WindowHooks for Hooks {
    fn waiting(&self,on: bool) {
        record(&[0x1000,on as u32]);
        if on { wake(); }
    }
}
fn wake() {
    if WAKE_ON_WAIT.swap(0,Ordering::SeqCst)!=0 {
        unsafe { assert_ne!(wm::PostMessageA(handle(WINDOW.load(Ordering::SeqCst)),0x401,0xAA,0),0); }
    }
}
inproc::stdcall_hook! { mod original_wait() -> i32; }
extern "system" fn suppress_display_change(_: u32,_: u32) -> i32 { 0 }

// Only drains messages that this standalone test owns. No shared thread queue.
unsafe fn drain(g: u32) {
    for _ in 0..500 {
        if unsafe { call(g,0x128,&[]) }==0 { return; }
    }
    panic!("standalone queue did not become idle");
}

#[derive(Debug,PartialEq,Eq)]
struct Observation { label: String, answer: u32, out: Vec<u32>, callbacks: Vec<Vec<u32>> }
fn observed(label: &str,answer: u32,out: Vec<u32>) -> Observation {
    Observation { label:label.into(),answer,out,callbacks:take() }
}

unsafe fn exercise(g: u32,controller: u32,owner: u32,close: bool) -> Vec<Observation> {
    let mut results=Vec::new();
    unsafe {
        take();
        NESTED_GRAPHICS.store(g,Ordering::SeqCst);
        let r=call(g,4,&[controller,c"jgl standalone contracts".as_ptr() as usize as u32,owner,0]);
        assert_eq!(r,0,"create graphics {g:x}, Win32 error {}",windows_sys::Win32::Foundation::GetLastError());
        let window=call(g,0x20,&[]); assert_ne!(window,0);
        WINDOW.store(window,Ordering::SeqCst);
        let windows=inproc::window::Window::all_of_this_process();
        let own_window=*windows.iter().find(|w| w.title()=="jgl standalone contracts").expect("test window");
        let _interceptor=inproc::message::intercept(own_window,|_|None).unwrap();
        // Creation callbacks contain no native handles and can be compared.
        results.push(observed("create",r,vec![wm::IsWindowVisible(handle(window)) as u32]));
        drain(g); take();
        let r=call(g,0x28,&[11,12,1024,768]);
        results.push(observed("position",r,vec![]));
        drain(g); take();
        for slot in [0x10,0x18,0x1C,0x14] {
            let r=call(g,slot,&[]);
            assert_eq!(wm::IsWindowVisible(handle(window)),0);
            results.push(observed(&format!("hidden-show-{slot:x}"),r,vec![]));
        }
        let list=call(g,0x68,&[]);
        let fields=std::slice::from_raw_parts(list as *const u32,5);
        let mut modes=fields[2..].to_vec();
        if fields[3]>0 { modes.extend_from_slice(std::slice::from_raw_parts(fields[1] as *const u32,fields[3] as usize*5)); }
        results.push(observed("os-display-list",0,modes));
        let r=call(g,0x64,&[0]);
        results.push(observed("mode-index",r,vec![call(g,0xA8,&[]),call(g,0xAC,&[]),call(g,0xB0,&[])]));
        let r=call(g,0x60,&[1024,768,16,1]);
        results.push(observed("keep-mode",r,vec![call(g,0xA8,&[]),call(g,0xAC,&[]),call(g,0xB0,&[])]));
        for (label,rects) in [("dirty-union",vec![RECT{left:4,top:5,right:100,bottom:200},RECT{left:0,top:20,right:400,bottom:300}]),
            ("dirty-next",vec![RECT{left:9,top:10,right:20,bottom:30}])] {
            drain(g); take();
            for rect in &rects { assert_ne!(call(g,0x30,&[addr(rect)]),0); }
            drain(g);
            let observation=observed(label,0,vec![]);
            let expected=if label=="dirty-union" {vec![4,0,5,400,300]} else {vec![4,9,10,20,30]};
            assert!(observation.callbacks.contains(&expected),"{observation:?}");
            results.push(observation);
        }
        call(g,0x30,&[0]); drain(g);
        results.push(observed("dirty-whole",0,vec![]));
        inproc::pointer::set(Some(inproc::pointer::Override { window:own_window,x:-7,y:42 }));
        let mut out=[0u32;2]; let r=call(g,0x2C,&[addr_mut(&mut out[0]),addr_mut(&mut out[1])]);
        assert_eq!(out,[(-7i32) as u32,42]);
        results.push(observed("pointer-override",r,out.to_vec()));
        inproc::pointer::set(None);
        for (id,wp,lp) in [(0x200,0,0xFFFD_FFFEu32),(0x201,0,0x0006_0005),(0x202,0,0x0006_0005),
            (0x203,0,0x0006_0005),(0x204,0,0x0006_0005),(0x205,0,0x0006_0005),
            (0x206,0,0x0006_0005),(0x20A,0xFF88_0004u32,0x0006_0005),(0x401,0x543,0),
            (0x100,0x70,0),(0x101,0x70,0),(0x104,0x12,0),
            (0x105,0x12,0),(0x102,0xE9,0),(0x402,0,0)] {
            drain(g); take();
            assert_ne!(wm::PostMessageA(handle(window),id,wp as usize,lp as i32 as isize),0);
            let r=call(g,0x128,&[]); drain(g);
            results.push(observed(&format!("posted-{id:x}"),r,vec![]));
        }
        // +402 is special only in +128, not when sent to the WndProc.
        assert_ne!(wm::PostMessageA(handle(window),0x401,0xBB,0),0);
        assert_ne!(wm::PostMessageA(handle(window),0x402,0,0),0);
        let r=call(g,0x128,&[]); drain(g);
        let reentered=observed("reentrant-timer-pump",r,vec![]);
        assert_eq!(reentered.callbacks,[vec![48,0xBB],vec![52],vec![0x2000,1]]);
        results.push(reentered);
        let r=wm::SendMessageA(handle(window),0x402,0,0);
        results.push(observed("sent-402-default",r as u32,vec![]));
        call(g,0x154,&[copy_data as *const () as usize as u32]);
        let payload=[1u8,2,3,4];
        let data=CopyData{data:0x567,length:payload.len() as u32,bytes:payload.as_ptr()};
        let r=wm::SendMessageA(handle(window),0x4A,0,addr(&data) as isize);
        assert_eq!(r as u32,0xABCD_1234);
        results.push(observed("copydata",r as u32,vec![]));
        call(g,0x154,&[0]);
        let r=wm::SendMessageA(handle(window),0x4A,0,addr(&data) as isize);
        assert_eq!(r,0);
        results.push(observed("copydata-unset",r as u32,vec![]));
        drain(g); take();
        WAKE_ON_WAIT.store(1,Ordering::SeqCst);
        let r=call(g,0x124,&[]);
        results.push(observed("wait-wakes-without-consuming",r,vec![]));
        let r=call(g,0x120,&[]);
        results.push(observed("take-wakeup-timer",r,vec![]));
        drain(g); take();
        wm::PostQuitMessage(37);
        let r=call(g,0x128,&[]);
        results.push(observed("quit-message-consumed",r,vec![wm::IsWindow(handle(window)) as u32]));
        if close {
            let r=wm::SendMessageA(handle(window),wm::WM_CLOSE,0,0);
            results.push(observed("close",r as u32,vec![wm::IsWindow(handle(window)) as u32]));
        }
        take(); let r=call(g,8,&[]);
        results.push(observed("destroy",r,vec![call(g,0x20,&[]),call(g,0x24,&[])]));
        WINDOW.store(0,Ordering::SeqCst);
        drain(g); take();
    }
    results
}

#[test]
fn real_messages_and_hidden_paints_match_original() {
    let (Ok(path),Ok(_))=(std::env::var("CIV3_JGL_DLL"),std::env::var("CIV3R_JGL_WINDOW_PRIVATE")) else {
        eprintln!("skipped: use tools/test-jgl-window with a private display/prefix");return;
    };
    unsafe {
        let path=std::ffi::CString::new(path).unwrap();
        let module=LoadLibraryA(path.as_ptr().cast()); assert!(!module.is_null());
        let f=GetProcAddress(module,c"get_graphsy_object_ptr".as_ptr().cast()).unwrap();
        let original=std::mem::transmute::<_,extern "C" fn()->u32>(f)();
        jgl_ffi::own::install(Box::new(jgl::Hooks::default),Box::new(Hooks)).unwrap();
        let owned=jgl_ffi::own::new_graphics();
        let base=module as usize as u32;
        let _hidden=inproc::hidden::hook_hidden_windows(base).unwrap().unwrap();
        let _paint=inproc::paint::hook_hidden_painting(base).unwrap().unwrap();
        let _pointer=inproc::pointer::hook_cursor_pos(base).unwrap().unwrap();
        let change=inproc::pe::import_slot(base,"USER32.dll","ChangeDisplaySettingsA").unwrap().unwrap();
        let _changes=inproc::hook::SlotPatch::replace(change,suppress_display_change as *const () as usize as u32).unwrap();
        let at=inproc::pe::import_slot(base,"USER32.dll","WaitMessage").unwrap().unwrap();
        let _wait=original_wait::install_slot(at,|c| { record(&[0x1000,1]);wake();let r=c.original();record(&[0x1000,0]);r }).unwrap();
        let controller_table=[size as usize,paint as usize,hit as usize,key as usize,character as usize,motion as usize,
            wheel as usize,left_down as usize,left_up as usize,right_down as usize,right_up as usize,
            activate as usize,timer as usize,message402 as usize];
        let controller=[addr(&controller_table)];
        let owner_table=[owner_create as usize,0,owner_clear as usize]; let owner=[addr(&owner_table)];
        inproc::hidden::set_hiding(true);
        let mut checked=0;
        // More than the paint adapter's fixed table size, including handles
        // recreated after WM_CLOSE and explicit graphics shutdown.
        for cycle in 0..12 {
            let a=exercise(original,addr(&controller),addr(&owner),cycle%2!=0);
            let b=exercise(owned,addr(&controller),addr(&owner),cycle%2!=0);
            assert_eq!(a.len(),b.len());
            for (a,b) in a.into_iter().zip(b) {
                assert_eq!(a,b,"cycle {cycle}");
                checked+=1; println!("MATCH cycle-{cycle} {a:?}");
            }
        }
        println!("real Win32 comparison: {checked} per-call observations, zero differences; 24 hidden window lifetimes");
    }
}

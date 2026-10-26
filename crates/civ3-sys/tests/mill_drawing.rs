//! Runs the supported R28 machine code at its audited addresses. Only the
//! Sprite/query/GDI dependencies are replaced by command/call recorders.
//! This is a private process, not a game session or an installed-DLL test.
#![cfg(all(windows, target_arch = "x86"))]

use civ3_sys::{Build, c3x_layout as X, layout as L, mill_icons as native};
use inproc::{
    Ptr32,
    hook::{HookError, InlinePatch},
};
use std::{cell::RefCell, ffi::c_void};
use windows_sys::Win32::System::Memory::PAGE_EXECUTE_READWRITE;

#[path = "support/mill_abi.rs"]
mod abi;

const STATE: u32 = 0xcd6000;
const CITY: u32 = 0x900000;
const MILLS: u32 = 0x910000;
const RESOURCES: u32 = 0x920000;
const SHEET: u32 = 0x930000;
const CANVAS: u32 = 0x940000;
const JGL_SHEET: u32 = 0x950000;
const JGL_CANVAS: u32 = 0x960000;
const TABLE: u32 = 0x970000;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Sprite(Vec<u32>),
    Generate(u32, u32),
    Active(u32, u32),
    Inputs(u32, u32),
    Ready,
    Acquire(u32),
    Blt(Vec<u32>),
    Release(u32, u32),
}
#[derive(Default)]
struct Fixture {
    events: Vec<Event>,
    active: bool,
    inputs: bool,
    known: bool,
    no_dc: u32,
    nested: bool,
    depth: u32,
}
thread_local! { static FIXTURE: RefCell<Fixture> = RefCell::default(); }
fn record(e: Event) {
    FIXTURE.with_borrow_mut(|s| s.events.push(e));
}
fn put(at: u32, value: u32) {
    Ptr32::new(at).write(&value).unwrap();
}
fn bytes(at: u32, value: &[u8]) {
    // Test-only mapped pages are writable for their complete lifetime.
    unsafe {
        std::ptr::copy_nonoverlapping(value.as_ptr(), at as usize as *mut u8, value.len());
    }
}
fn address(f: *const ()) -> u32 {
    f as usize as u32
}

extern "fastcall" fn sprite(this: u32, _: u32, canvas: u32, x: u32, y: u32, table: u32) -> i32 {
    record(Event::Sprite(vec![this, canvas, x, y, table]));
    let nested = FIXTURE.with_borrow_mut(|s| {
        if s.nested && s.depth == 0 {
            s.depth = 1;
            true
        } else {
            false
        }
    });
    if nested {
        assert_eq!(invoke(0x2222, 10, 20), 0x789abcde);
        FIXTURE.with_borrow_mut(|s| s.depth = 0);
    }
    0x789abcde
}
extern "fastcall" fn acquire(image: u32, _: u32) -> u32 {
    record(Event::Acquire(image));
    FIXTURE.with_borrow(|s| if s.no_dc == image { 0 } else { image })
}
extern "fastcall" fn release(image: u32, _: u32, n: u32) -> u32 {
    record(Event::Release(image, n));
    0
}
extern "system" fn blt(
    dc: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    src: u32,
    sx: u32,
    sy: u32,
    sw: u32,
    sh: u32,
    key: u32,
) -> i32 {
    record(Event::Blt(vec![dc, x, y, w, h, src, sx, sy, sw, sh, key]));
    0 // ignored, even on failure
}
fn invoke(this: u32, x: u32, y: u32) -> i32 {
    // Invoke the callee bound to the actual checked PCGames call site.
    // Calling the CALL instruction as a function would add a spurious
    // return address before its four stack arguments.
    type Draw = extern "fastcall" fn(u32, u32, u32, u32, u32, u32) -> i32;
    let offset = Ptr32::<u32>::new(native::SITE + 1).read().unwrap();
    let callee = (native::SITE + 5).wrapping_add(offset);
    let draw: Draw = unsafe { std::mem::transmute(callee as usize) };
    draw(this, 0x12345678, CANVAS, x, y, 0x3333)
}

fn invoke_checked(this: u32, x: u32, y: u32) -> i32 {
    let offset = Ptr32::<u32>::new(native::SITE + 1).read().unwrap();
    // Check all four callee-saved registers and exact RET 0x10 cleanup.
    // Nested calls use the typed ABI so this test probe's diagnostic
    // storage is not recursively overwritten.
    unsafe {
        abi::function(
            (native::SITE + 5).wrapping_add(offset),
            this,
            &[CANVAS, x, y, 0x3333],
        ) as i32
    }
}

struct Image {
    header: Vec<u8>,
}
impl Drop for Image {
    fn drop(&mut self) {
        bytes(0x400000, &self.header);
    }
}
fn mapped_binary() -> Image {
    let exe = std::fs::read(
        std::env::var_os("C3R_MILL_EXE").expect("tools/test-mill-icons sets C3R_MILL_EXE"),
    )
    .unwrap();
    let word = |at: usize| u32::from_le_bytes(exe[at..at + 4].try_into().unwrap());
    let nt = word(0x3c) as usize;
    let size = word(nt + 24 + 56) as usize;
    assert_eq!(word(nt + 24 + 28), 0x400000);
    // The runner links this test's text/data above 0x02000000. The PE
    // loader reserves the intervening image gap before Wine can use it for
    // heaps. Only our own reserved image pages may become the fixture.
    let mut at = 0x400000usize;
    while at < 0x400000 + size {
        let mut info = unsafe { std::mem::zeroed() };
        assert_ne!(
            unsafe {
                windows_sys::Win32::System::Memory::VirtualQuery(
                    at as *const c_void,
                    &mut info,
                    std::mem::size_of_val(&info),
                )
            },
            0
        );
        assert_eq!(
            info.AllocationBase as usize, 0x400000,
            "runner must reserve the low image gap"
        );
        at = info.BaseAddress as usize + info.RegionSize;
    }
    let mut old = 0;
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::Memory::VirtualProtect(
                0x400000usize as *const c_void,
                size,
                PAGE_EXECUTE_READWRITE,
                &mut old,
            )
        },
        0
    );
    let header = Ptr32::<u8>::new(0x400000).read_array(4096).unwrap();
    let map = Image { header };
    bytes(0x400000, &exe[..word(nt + 24 + 60) as usize]);
    let sections = u16::from_le_bytes(exe[nt + 6..nt + 8].try_into().unwrap());
    let table = nt + 24 + u16::from_le_bytes(exe[nt + 20..nt + 22].try_into().unwrap()) as usize;
    for n in 0..sections as usize {
        let at = table + n * 40;
        let raw = word(at + 20) as usize;
        let count = word(at + 16) as usize;
        bytes(0x400000 + word(at + 12), &exe[raw..raw + count]);
    }
    map
}
fn configure(count: u32) {
    put(0xe274e0, STATE);
    put(0x7423ac, CITY);
    put(0x9c7194, RESOURCES);
    put(0x9c3d64, 24);
    put(CITY + 0x1c + L::CITY_BODY_ID, 23);
    put(CITY + 0x1c + L::CITY_BODY_CIVID, 0xff);
    put(STATE + X::STATE_CURRENT_CONFIG_MILLS, MILLS);
    put(STATE + X::STATE_CURRENT_CONFIG_COUNT_MILLS, count);
    put(STATE + X::STATE_DRAWING_ICONS_FOR_IMPROV_ID, 2);
    put(STATE + X::STATE_RESOURCES_SHEET, SHEET);
    put(STATE + X::STATE_TRANSPARENTBLT, address(blt as *const ()));
    put(SHEET + 4, JGL_SHEET);
    put(CANVAS + 4, JGL_CANVAS);
    put(JGL_SHEET, TABLE);
    put(JGL_CANVAS, TABLE);
    put(TABLE + 0x28, address(acquire as *const ()));
    put(TABLE + 0x2c, address(release as *const ()));
    for n in 0..24u32 {
        // Original packed IDs/flags. The last case also proves the cap.
        bytes(MILLS + n * 6, &[2, 0, n as u8, 0, 0, 0]);
        put(
            RESOURCES + n * L::RESOURCE_TYPE_SIZE + L::RESOURCE_TYPE_CLASS,
            2,
        );
        put(
            RESOURCES + n * L::RESOURCE_TYPE_SIZE + L::RESOURCE_TYPE_ICONID,
            n + 3,
        );
    }
    FIXTURE.with_borrow_mut(|s| {
        *s = Fixture {
            known: true,
            active: true,
            inputs: true,
            ..Default::default()
        }
    });
}
fn events() -> Vec<Event> {
    FIXTURE.with_borrow_mut(|s| std::mem::take(&mut s.events))
}
fn without_raster(e: &[Event]) -> Vec<Event> {
    e.iter()
        .filter(|e| !matches!(e, Event::Acquire(_) | Event::Blt(_) | Event::Release(..)))
        .cloned()
        .collect()
}

#[test]
#[ignore = "requires supported binary; tools/test-mill-icons uses a private prefix and relocated test PE"]
fn bound_r28_draw_block_preserves_queries_and_skips_raster() {
    let _map = mapped_binary();
    configure(2);
    let producer = native::Producer::find(Build::PcGames)
        .unwrap()
        .expect("checked R28 body");
    // Bind/reject both entry and exit. Never patch an unaudited body.
    bytes(0xdef018, &[0x90]);
    assert!(matches!(
        native::Producer::find(Build::PcGames),
        Err(HookError::UnexpectedCode { addr: 0xdeec18, .. })
    ));
    bytes(0xdef018, &[0x8b]);
    let _sprite = InlinePatch::install(
        0xcd4040,
        &Ptr32::<u8>::new(0xcd4040).read_array(5).unwrap(),
        address(sprite as *const ()),
        |_| {},
    )
    .unwrap();
    let _generate = native::can_generate::install_call_site(
        native::GENERATE_SITE,
        &native::GENERATE_BYTES,
        |c| {
            record(Event::Generate(c.civ, c.mill));
            FIXTURE.with_borrow(|s| s.known as u8)
        },
    )
    .unwrap();
    let _active = native::active_building::install_call_site(
        native::ACTIVE_SITE,
        &native::ACTIVE_BYTES,
        |c| {
            record(Event::Active(c.city, c.improvement));
            FIXTURE.with_borrow(|s| s.active as u8)
        },
    )
    .unwrap();
    let _inputs =
        native::has_inputs::install_call_site(native::INPUTS_SITE, &native::INPUTS_BYTES, |c| {
            record(Event::Inputs(c.city, c.improvement));
            FIXTURE.with_borrow(|s| s.inputs as u8)
        })
        .unwrap();
    let _draw = native::draw::install_call_site(native::SITE, &native::SITE_BYTES, move |c| {
        let row = producer.row().unwrap();
        assert_eq!((row.city_id, row.civ, row.improvement), (23, -1, 2));
        c.original()
    })
    .unwrap();
    let mut pairs = 0;
    let mut avoided = 0;
    // Empty, predicates, all source-availability gates, failed DCs,
    // failed blits, the 16-icon cap, negative placement, and reentry.
    for case in 0..12 {
        let run = |owned| {
            configure(if case == 0 {
                0
            } else if case == 10 {
                24
            } else {
                2
            });
            FIXTURE.with_borrow_mut(|s| {
                s.known = case != 1;
                s.active = case != 2;
                s.inputs = case != 3;
                s.no_dc = if case == 8 {
                    JGL_CANVAS
                } else if case == 9 {
                    JGL_SHEET
                } else {
                    0
                };
                s.nested = case == 11;
            });
            if case == 4 {
                put(STATE + X::STATE_RESOURCES_SHEET, 0)
            }
            if case == 5 {
                put(SHEET + 4, 0)
            }
            if case == 6 {
                put(STATE + X::STATE_TRANSPARENTBLT, 0)
            }
            if case == 7 {
                // A hidden bonus never reaches the native predicates.
                put(RESOURCES + L::RESOURCE_TYPE_CLASS, 0);
            }
            let boundary = producer
                .intercept_drawing(owned, || record(Event::Ready))
                .unwrap();
            let result = if case == 11 {
                invoke(0x1111, (-3i32) as u32, 19)
            } else {
                invoke_checked(0x1111, (-3i32) as u32, 19)
            };
            assert_eq!(result, 0x789abcde);
            drop(boundary);
            assert_eq!(
                Ptr32::<u8>::new(0xdeee34).read_array(6).unwrap(),
                [0x8b, 0x45, 8, 0x83, 0xc0, 4]
            );
            events()
        };
        let original = run(false);
        let owned = run(true);
        assert_eq!(
            without_raster(&original),
            owned,
            "query/order/return pair {case}"
        );
        let raster = original.len() - owned.len();
        avoided += raster;
        pairs += 1;
        if matches!(case, 0..=6) {
            assert!(!owned.contains(&Event::Ready));
        }
        if case == 10 {
            assert_eq!(
                original
                    .iter()
                    .filter(|e| matches!(e, Event::Blt(_)))
                    .count(),
                16
            );
        }
        if case == 11 {
            assert_eq!(
                owned.iter().filter(|e| matches!(e, Event::Ready)).count(),
                2
            );
        }
        assert!(
            owned
                .iter()
                .all(|e| !matches!(e, Event::Acquire(_) | Event::Blt(_) | Event::Release(..)))
        );
        if case == 7 {
            assert!(original.contains(&Event::Blt(vec![
                JGL_CANVAS, 12, 19, 24, 24, JGL_SHEET, 209, 9, 33, 33, 0xff00ff
            ])));
        }
    }
    // With no boundary installed the original still calls both DCs/GDI.
    configure(2);
    assert_eq!(invoke(1, 30, 40), 0x789abcde);
    let restored = events();
    assert_eq!(
        restored
            .iter()
            .filter(|e| matches!(e, Event::Blt(_)))
            .count(),
        2
    );
    assert!(!restored.contains(&Event::Ready));
    println!(
        "{pairs} original/owned binary pairs; {avoided} legacy acquire/blit/release calls avoided; zero owned raster calls; original block restored"
    );
}

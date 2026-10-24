//! The layouts and the slot catalogue checked against jgl.dll itself: its
//! vtables, factory allocation sizes, constructor stores and getter bodies.
//! Needs the game's `jgl.dll` (`CIV3_JGL_DLL`, or the dataset copy named in
//! `docs/rerender-roadmap.md`); skipped without it.

use std::mem::offset_of;

use jgl::slots::Class;
use jgl_ffi::layout::{JglColorTable, JglFont, JglGraphics, JglImage, JglSprite};

const DATASET: &str = "/mnt/datasets/winvm/civ3-src-c3x28/Civilization III Complete/Conquests/jgl.dll";

/// jgl.dll's sections, enough to read it at RVAs.
struct Pe {
    data: Vec<u8>,
    image_base: u32,
    /// (rva, size, file offset)
    sections: Vec<(u32, u32, u32)>,
}

impl Pe {
    fn load() -> Option<Pe> {
        let path = std::env::var("CIV3_JGL_DLL").unwrap_or_else(|_| DATASET.into());
        let data = std::fs::read(&path).ok()?;
        let u16_at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let pe = u32_at(0x3C) as usize;
        assert_eq!(&data[pe..pe + 4], b"PE\0\0");
        let sections = u16_at(pe + 6) as usize;
        let optional = u16_at(pe + 20) as usize;
        let image_base = u32_at(pe + 24 + 28);
        let table = pe + 24 + optional;
        let sections = (0..sections)
            .map(|i| {
                let s = table + i * 40;
                (u32_at(s + 12), u32_at(s + 8).max(u32_at(s + 16)), u32_at(s + 20))
            })
            .collect();
        Some(Pe { data, image_base, sections })
    }

    fn at(&self, rva: u32, len: usize) -> &[u8] {
        let &(start, _, file) = self.sections.iter().find(|&&(s, n, _)| rva >= s && rva < s + n).expect("rva in a section");
        let o = (file + rva - start) as usize;
        &self.data[o..o + len]
    }

    fn u32(&self, rva: u32) -> u32 {
        u32::from_le_bytes(self.at(rva, 4).try_into().unwrap())
    }

    /// The body of `class`'s slot at `offset`.
    fn slot_body(&self, class: Class, offset: u16, len: usize) -> &[u8] {
        let f = self.u32(class.vtable_rva() + offset as u32) - self.image_base;
        self.at(f, len)
    }
}

fn pe() -> Option<Pe> {
    let pe = Pe::load();
    if pe.is_none() {
        eprintln!("jgl.dll not found (CIV3_JGL_DLL): skipped");
    }
    pe
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn disp32(field: usize) -> [u8; 4] {
    (field as u32).to_le_bytes()
}

#[test]
fn every_slot_is_where_the_catalogue_says() {
    let Some(pe) = pe() else { return };
    assert_eq!(pe.image_base, jgl_ffi::PREFERRED_BASE);
    for class in Class::ALL {
        for slot in class.slots() {
            let f = pe.u32(class.vtable_rva() + slot.offset as u32) - pe.image_base;
            assert_eq!(f, slot.rva, "{class:?}+{:#x}", slot.offset);
        }
    }
}

#[test]
fn factories_allocate_the_layout_sizes() {
    let Some(pe) = pe() else { return };
    // graphics +0x74..+0x80: push size; call operator new.
    for (slot, class) in [(0x74, Class::Font), (0x78, Class::ColorTable), (0x7C, Class::Image), (0x80, Class::Sprite)] {
        let body = pe.slot_body(Class::Graphics, slot, 0x30);
        let size = class.object_size();
        let push = if size < 0x80 { vec![0x6A, size as u8] } else { [&[0x68][..], &size.to_le_bytes()].concat() };
        assert!(contains(body, &push), "{class:?}");
    }
}

#[test]
fn image_getters_read_the_layout_fields() {
    let Some(pe) = pe() else { return };
    let body = |o| pe.slot_body(Class::Image, o, 32);
    // lea eax, [ecx + clip]; ret
    assert_eq!(&body(0xCC)[..4], &[0x8D, 0x41, offset_of!(JglImage, clip) as u8, 0xC3]);
    assert_eq!(&body(0xD4)[..4], &[0x8D, 0x41, offset_of!(JglImage, image_rect) as u8, 0xC3]);
    assert_eq!(&body(0xE4)[..4], &[0x8D, 0x41, offset_of!(JglImage, bit_count) as u8, 0xC3]);
    // mov eax, [ecx + right]; mov edx, [ecx + left]; sub; ret
    let r = offset_of!(JglImage, image_rect);
    assert_eq!(&body(0xD8)[..9], &[0x8B, 0x41, (r + 8) as u8, 0x8B, 0x51, r as u8, 0x2B, 0xC2, 0xC3]);
    assert_eq!(&body(0xDC)[..9], &[0x8B, 0x41, (r + 12) as u8, 0x8B, 0x51, (r + 4) as u8, 0x2B, 0xC2, 0xC3]);
    assert_eq!(&body(0xE0)[..4], &[0x8B, 0x41, offset_of!(JglImage, row_width) as u8, 0xC3]);
    assert_eq!(&body(0xE8)[..4], &[0x8B, 0x41, offset_of!(JglImage, field_7c) as u8, 0xC3]);
    assert_eq!(&body(0xC0)[..7], &[[0x8B, 0x81].as_slice(), &disp32(offset_of!(JglImage, color_key)), &[0xC3]].concat()[..]);
    // lock: mov eax, [ecx + bits]
    assert_eq!(&body(0x10)[..6], &[[0x8B, 0x81].as_slice(), &disp32(offset_of!(JglImage, bits))].concat()[..]);
    // acquire_dc: mov eax, [ecx + dc] ... mov [ecx + current_dc], eax
    assert!(contains(body(0x28), &[[0x8B, 0x81].as_slice(), &disp32(offset_of!(JglImage, dc))].concat()));
    assert!(contains(body(0x28), &[[0x89, 0x81].as_slice(), &disp32(offset_of!(JglImage, current_dc))].concat()));
}

#[test]
fn the_image_constructor_stores_owner_and_bitmap_header() {
    let Some(pe) = pe() else { return };
    let ctor = pe.at(0x15F0, 0x120);
    // mov [esi + owner], ecx
    assert!(contains(ctor, &[[0x89, 0x8E].as_slice(), &disp32(offset_of!(JglImage, owner))].concat()));
    // mov dword [esi + bitmap_info], 0x28
    assert!(contains(ctor, &[[0xC7, 0x86].as_slice(), &disp32(offset_of!(JglImage, bitmap_info)), &0x28u32.to_le_bytes()].concat()));
    // mov [esi + color_key], eax
    assert!(contains(ctor, &[[0x89, 0x86].as_slice(), &disp32(offset_of!(JglImage, color_key))].concat()));
}

#[test]
fn sprite_colour_table_and_font_getters() {
    let Some(pe) = pe() else { return };
    let sprite = |o| pe.slot_body(Class::Sprite, o, 8);
    assert_eq!(&sprite(0x14)[..4], &[0x8B, 0x41, offset_of!(JglSprite, color_table) as u8, 0xC3]);
    assert_eq!(&sprite(0x1C)[..4], &[0x8B, 0x41, offset_of!(JglSprite, key) as u8, 0xC3]);
    assert_eq!(&sprite(0x20)[..4], &[0x8B, 0x41, offset_of!(JglSprite, field_14) as u8, 0xC3]);
    let table = |o| pe.slot_body(Class::ColorTable, o, 8);
    assert_eq!(&table(0x18)[..7], &[[0x8D, 0x81].as_slice(), &disp32(offset_of!(JglColorTable, table16_a)), &[0xC3]].concat()[..]);
    assert_eq!(&table(0x1C)[..7], &[[0x8D, 0x81].as_slice(), &disp32(offset_of!(JglColorTable, table16_b)), &[0xC3]].concat()[..]);
    // font +0x0C release: mov eax, [esi + hfont]; later mov eax, [esi + resource]
    let release = pe.slot_body(Class::Font, 0x0C, 0x30);
    assert!(contains(release, &[0x8B, 0x46, offset_of!(JglFont, hfont) as u8]));
    assert!(contains(release, &[0x8B, 0x46, offset_of!(JglFont, resource) as u8]));
    // image +0xA8 select_font reads [font + hfont].
    assert!(contains(pe.slot_body(Class::Image, 0xA8, 0x20), &[0x8B, 0x4A, offset_of!(JglFont, hfont) as u8]));
}

#[test]
fn graphics_getters() {
    let Some(pe) = pe() else { return };
    let body = |o| pe.slot_body(Class::Graphics, o, 8);
    let get = |field: usize| [[0x8B, 0x81].as_slice(), &disp32(field), &[0xC3]].concat();
    assert_eq!(&body(0xA8)[..7], &get(offset_of!(JglGraphics, screen_width))[..]);
    assert_eq!(&body(0xAC)[..7], &get(offset_of!(JglGraphics, screen_height))[..]);
    assert_eq!(&body(0xB0)[..7], &get(offset_of!(JglGraphics, depth))[..]);
    assert_eq!(&body(0x20)[..7], &get(offset_of!(JglGraphics, hwnd))[..]);
    assert_eq!(&body(0x24)[..7], &get(offset_of!(JglGraphics, hinstance))[..]);
    assert_eq!(&body(0x3C)[..7], &get(offset_of!(JglGraphics, dc))[..]);
}

#[test]
fn text_slot_bodies_are_the_ones_taken_over() {
    let Some(pe) = pe() else { return };
    // text_out: TextOutA(dc, x, y, text, len) between acquire/release, ret 0x10.
    let body = pe.slot_body(Class::Image, 0xB8, 0x40);
    assert_eq!(&body[..3], &[0x56, 0x8B, 0xF1]);
    assert!(contains(body, &[0xC2, 0x10, 0x00]));
    // set_text_color16: and ecx, 0xFFFF; or ecx, 0x10FF0000.
    let body = pe.slot_body(Class::Image, 0xB0, 0x30);
    assert!(contains(body, &[0x81, 0xE1, 0xFF, 0xFF, 0x00, 0x00, 0x81, 0xC9, 0x00, 0x00, 0xFF, 0x10]));
    // font create: weight 0x2BC for flag bit 0.
    assert!(contains(pe.slot_body(Class::Font, 0x08, 0xA0), &[0x81, 0xE1, 0xBC, 0x02, 0x00, 0x00]));
}

#[test]
fn set_mode_keep_takes_the_size_without_switching_the_display() {
    let Some(pe) = pe() else { return };
    // The display-mode group answers with this slot's keep branch.
    let body = pe.slot_body(Class::Graphics, 0x60, 0x55);
    // mov al, [esp + 0x10]; test al, al; jne keep (the byte argument).
    assert_eq!(&body[..8], &[0x8A, 0x44, 0x24, 0x10, 0x84, 0xC0, 0x75, 0x19]);
    let keep = &body[0x21..];
    let store = |op: u8, field: usize| [[0x89, op].as_slice(), &disp32(field)].concat();
    assert!(contains(keep, &store(0x81, offset_of!(JglGraphics, screen_width))));
    assert!(contains(keep, &store(0x91, offset_of!(JglGraphics, screen_height))));
    assert!(contains(keep, &store(0xB1, offset_of!(JglGraphics, depth))));
    // mov ecx, [ecx + owner]; ...; push 1; ...; call [edi] (the owner's method 0); ret 0x10.
    assert!(contains(keep, &[[0x8B, 0x89].as_slice(), &disp32(offset_of!(JglGraphics, owner))].concat()));
    assert!(contains(keep, &[0x6A, 0x01, 0x8B, 0x39, 0x56, 0x52, 0x50, 0xFF, 0x17]));
    assert!(contains(keep, &[0xC2, 0x10, 0x00]));
    // No import is called on that branch (ChangeDisplaySettingsA would be `call [imm32]`).
    assert!(!contains(keep, &[0xFF, 0x15]));
    // set_mode and set_mode_refresh switch through +0x64 (call [reg + 0x64]).
    for slot in [0x58, 0x5C] {
        let body = pe.slot_body(Class::Graphics, slot, 0xA0);
        assert!(contains(body, &[0xFF, 0x50, 0x64]) || contains(body, &[0xFF, 0x52, 0x64]), "+{slot:#x}");
    }
}

/// The RVA of the import address table entry through which jgl.dll calls
/// `dll!function`.
fn import_rva(pe: &Pe, imports: u32, dll: &str, function: &str) -> Option<u32> {
    let c_string = |rva: u32| {
        let bytes = pe.at(rva, 64);
        String::from_utf8_lossy(&bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(64)]).into_owned()
    };
    for i in 0.. {
        let desc = imports + 20 * i;
        let (lookup, name, iat) = (pe.u32(desc), pe.u32(desc + 12), pe.u32(desc + 16));
        if name == 0 {
            return None;
        }
        if !c_string(name).eq_ignore_ascii_case(dll) {
            continue;
        }
        let names = if lookup != 0 { lookup } else { iat };
        for j in 0.. {
            let entry = pe.u32(names + 4 * j);
            if entry == 0 {
                break;
            }
            if entry & 0x8000_0000 == 0 && c_string(entry + 2) == function {
                return Some(iat + 4 * j);
            }
        }
    }
    None
}

#[test]
fn every_display_change_of_jgl_goes_through_its_one_import() {
    let Some(pe) = pe() else { return };
    // The PE header is in no section: read it from the file.
    let header = u32::from_le_bytes(pe.data[0x3C..0x40].try_into().unwrap()) as usize;
    let imports = u32::from_le_bytes(pe.data[header + 0x80..header + 0x84].try_into().unwrap());
    assert_ne!(imports, 0);
    let slot = import_rva(&pe, imports, "USER32.dll", "ChangeDisplaySettingsA").expect("jgl imports ChangeDisplaySettingsA");
    let va = (pe.image_base + slot).to_le_bytes();
    // Every mention of the slot in the code is a `call [slot]` (no copy of
    // the pointer to call later), and those are in exactly the four slots
    // the display group answers: shutdown, set mode by index, the movie's
    // switch and its way back.
    let mut calls = Vec::new();
    for &(start, size, file) in &pe.sections {
        let code = &pe.data[file as usize..((file + size) as usize).min(pe.data.len())];
        for (i, w) in code.windows(4).enumerate() {
            if w == va {
                assert!(i >= 2 && code[i - 2..i] == [0xFF, 0x15], "a mention of the slot that is no call at rva {:#x}", start + i as u32);
                calls.push(start + i as u32 - 2);
            }
        }
    }
    let body = |offset: u16| {
        let f = pe.u32(Class::Graphics.vtable_rva() + offset as u32) - pe.image_base;
        f..f + 0xA0
    };
    let mut owners: Vec<u16> = calls
        .iter()
        .map(|&call| *[0x08u16, 0x64, 0x6C, 0x70].iter().filter(|&&o| body(o).contains(&call)).max_by_key(|&&o| body(o).start).unwrap_or_else(|| panic!("a display change at {call:#x} outside the answered slots")))
        .collect();
    owners.sort();
    assert_eq!(owners, [0x08, 0x64, 0x6C, 0x70]);
    // The movie's switch: 640×480 is written by the game's caller, the
    // fields DM_BITSPERPEL | DM_PELSWIDTH | DM_PELSHEIGHT by jgl (mov [esp+0x38], 0x1C0000).
    let movie = pe.slot_body(Class::Graphics, 0x6C, 0x60);
    let fields = jgl::display::DM_BITSPERPEL | jgl::display::DM_PELSWIDTH | jgl::display::DM_PELSHEIGHT;
    assert!(contains(movie, &[[0xC7, 0x44, 0x24, 0x38].as_slice(), &fields.to_le_bytes()].concat()));
}

/// Where `ret n` ends a slot body of `class` (the first one within `len`
/// bytes), checked against the argument bytes the catalogue (and so the
/// entry point taking it over) has.
fn returns_with(pe: &Pe, class: Class, offset: u16, len: usize) -> bool {
    let stack = class.slot(offset).expect("a slot").stack;
    contains(pe.slot_body(class, offset, len), &[0xC2, stack, 0x00])
}

#[test]
fn fill_group_slot_bodies_are_the_ones_taken_over() {
    let Some(pe) = pe() else { return };
    // Every slot pops the arguments its entry point takes.
    for offset in [0x44, 0x48, 0x4C, 0x50, 0x54, 0x58, 0x5C, 0x60, 0x64] {
        assert!(returns_with(&pe, Class::Image, offset, 0x80), "+{offset:#x}");
    }
    // fill(rect, color): one worker per bit count, both given (rect, color); answers 0.
    let fill = pe.slot_body(Class::Image, 0x44, 0x44);
    assert!(contains(fill, &[0x8B, 0x4C, 0x24, 0x0C, 0x8B, 0x54, 0x24, 0x08, 0x51, 0x52, 0x8B, 0xCE, 0xE8]));
    assert!(contains(fill, &[0x33, 0xC0, 0x5E, 0xC2, 0x08, 0x00]));
    // shade_rect: 16-bit images only (cmp [format], 0x10), else 0x17.
    let shade = pe.slot_body(Class::Image, 0x48, 0x34);
    assert!(contains(shade, &[0x83, 0x38, 0x10]));
    assert!(contains(shade, &[0xB8, 0x17, 0x00, 0x00, 0x00]));
    // line(x1, y1, x2, y2, color, flag): answers 0.
    assert!(contains(pe.slot_body(Class::Image, 0x64, 0x64), &[0x33, 0xC0, 0x5E, 0xC2, 0x18, 0x00]));
    // replace_color(rect, from, to): null rect answers 0x10; the 16-bit worker
    // (RVA 0x2740) clips to the image's clip (+0xCC) and swaps `from` for `to`.
    let replace = pe.slot_body(Class::Image, 0x58, 0x30);
    assert!(contains(replace, &[0x85, 0xF6, 0x75, 0x0A]) && contains(replace, &[0xB8, 0x10, 0x00, 0x00, 0x00]));
    let worker = pe.at(0x2740, 0xD0);
    assert!(contains(worker, &[0xFF, 0x92, 0xCC, 0x00, 0x00, 0x00]));
    // cmp [edi], ax; jne +3; mov [edi], bx
    assert!(contains(worker, &[0x66, 0x39, 0x07, 0x75, 0x03, 0x66, 0x89, 0x1F]));
    // frame (+0x5C): 8-bit images only; 16-bit ones answer 0 undrawn.
    let frame = pe.slot_body(Class::Image, 0x5C, 0x20);
    assert!(contains(frame, &[0x83, 0x38, 0x08, 0x74, 0x06, 0x33, 0xC0]));
}

#[test]
fn copy_group_slot_bodies_are_the_ones_taken_over() {
    let Some(pe) = pe() else { return };
    for offset in jgl_ffi::copy::SLOTS {
        assert!(returns_with(&pe, Class::Image, offset, 0x100), "+{offset:#x}");
    }
    // copy(target, src, dst): any null argument answers 0x10; StretchBlt from
    // this image's DC onto the target's.
    let header = u32::from_le_bytes(pe.data[0x3C..0x40].try_into().unwrap()) as usize;
    let imports = u32::from_le_bytes(pe.data[header + 0x80..header + 0x84].try_into().unwrap());
    let stretch = import_rva(&pe, imports, "GDI32.dll", "StretchBlt").expect("jgl imports StretchBlt");
    let copy = pe.slot_body(Class::Image, 0x40, 0xD8);
    assert!(contains(copy, &[[0xFF, 0x15].as_slice(), &(pe.image_base + stretch).to_le_bytes()].concat()));
    assert!(contains(copy, &[0xB8, 0x10, 0x00, 0x00, 0x00]));
    // SRCCOPY, then (hdcDest = the target's DC, saved over the third argument).
    assert!(contains(copy, &[0x68, 0x20, 0x00, 0xCC, 0x00]));
    assert!(contains(copy, &[0x8B, 0x54, 0x24, 0x3C, 0x51, 0x53, 0x50, 0x52]));
    // copy_38: this image 8-bit (cmp [format], 8), the target 8 or 16.
    let indexed = pe.slot_body(Class::Image, 0x38, 0x40);
    assert!(contains(indexed, &[0x83, 0x38, 0x08, 0x74, 0x0A]));
    assert!(contains(indexed, &[0x83, 0xF8, 0x10]));
}

/// The exe's `PCX_Image` functions whose detours the fill and copy groups
/// replaced reach jgl through these slots, with these arguments. Needs the
/// game's exe (`CIV3_EXE`, or the dataset copy next to `jgl.dll`).
#[test]
fn the_exe_wrappers_call_the_taken_over_slots() {
    let path = std::env::var("CIV3_EXE").unwrap_or_else(|_| DATASET.replace("jgl.dll", "Civ3Conquests.exe"));
    let Ok(exe) = std::fs::read(&path) else {
        eprintln!("Civ3Conquests.exe not found (CIV3_EXE): skipped");
        return;
    };
    let u32_at = |o: usize| u32::from_le_bytes(exe[o..o + 4].try_into().unwrap());
    let pe = u32_at(0x3C) as usize;
    let image_base = u32_at(pe + 24 + 28);
    let table = pe + 24 + u16::from_le_bytes([exe[pe + 20], exe[pe + 21]]) as usize;
    let at = |va: u32, len: usize| -> &[u8] {
        let rva = va - image_base;
        let s = (0..u16::from_le_bytes([exe[pe + 6], exe[pe + 7]]) as usize)
            .map(|i| table + i * 40)
            .find(|&s| rva >= u32_at(s + 12) && rva < u32_at(s + 12) + u32_at(s + 8).max(u32_at(s + 16)))
            .expect("in a section");
        let o = (u32_at(s + 20) + rva - u32_at(s + 12)) as usize;
        &exe[o..o + len]
    };
    // PCX_Image::fill (0x5FFF30): push color; push 0 (no rect: the whole image); call [eax+0x44].
    assert!(contains(at(0x5F_FF30, 0x16), &[0x52, 0x6A, 0x00, 0xFF, 0x50, 0x44]));
    // fill_area (0x5FFF50): jmp [eax+0x44] with (rect, color).
    assert!(contains(at(0x5F_FF50, 0x14), &[0xFF, 0x60, 0x44]));
    // line, hline, vline (0x5FFFB0, 0x5FFFE0, 0x600010): +0x64 with flag 1.
    for (va, call) in [(0x5F_FFB0, [0xFF, 0x50, 0x64]), (0x5F_FFE0, [0xFF, 0x52, 0x64]), (0x60_0010, [0xFF, 0x52, 0x64])] {
        let body = at(va, 0x28);
        assert!(contains(body, &[0x6A, 0x01]) && contains(body, &call), "{va:#x}");
    }
    // draw_onto (0x5FC9F0): +0x84 on this image's jgl image, the target's first.
    assert!(contains(at(0x5F_C9F0, 0x38), &[0xFF, 0x92, 0x84, 0x00, 0x00, 0x00]));
    // copy_same, copy_rect, copy_stretch, copy_rects: +0x40 (target, &src, &dst).
    for va in [0x5F_CB60, 0x5F_CBD0, 0x5F_CC10, 0x5F_CCB0] {
        let body = at(va, 0x90);
        assert!(contains(body, &[0xFF, 0x52, 0x40]) || contains(body, &[0xFF, 0x56, 0x40]), "{va:#x}");
    }
}

#[test]
fn sprite_and_colour_table_constructors_store_their_owners() {
    let Some(pe) = pe() else { return };
    // JGLSprite's constructor (0x7E80, which graphics +0x80 new_sprite
    // calls with its first argument at 0x3C097): mov [esi + owner], eax.
    let call = pe.at(0x3_C097, 5);
    assert_eq!(call[0], 0xE8);
    assert_eq!(0x3_C09Cu32.wrapping_add(u32::from_le_bytes(call[1..5].try_into().unwrap())), 0x7E80);
    assert!(contains(pe.at(0x7E80, 0x40), &[0x89, 0x46, offset_of!(JglSprite, owner) as u8]));
    // JGL_Color_Table's (0x3CEE0, from +0x78): mov [esi + owner], eax.
    assert!(contains(pe.at(0x3_CEE0, 0x20), &[[0x89, 0x86].as_slice(), &disp32(offset_of!(JglColorTable, owner))].concat()));
    // The blits dispatch on the sprite's bits: mov eax, [esi + bits]; cmp eax, 8.
    assert!(contains(pe.slot_body(Class::Sprite, 0x44, 0x40), &[0x8B, 0x46, offset_of!(JglSprite, bits) as u8, 0x83, 0xF8, 0x08]));
}

#[test]
fn set_scale_stores_the_globals_the_shim_reads() {
    let Some(pe) = pe() else { return };
    let at = |i: u32| (pe.image_base + jgl_ffi::blit::SCALE_RVA + 4 * i).to_le_bytes();
    let body = pe.slot_body(Class::Graphics, 0x9C, 0x20);
    // mov [num_x], eax; mov [num_y], ecx; mov [den], edx; ret 0xC.
    assert!(contains(body, &[[0xA3].as_slice(), &at(0)].concat()));
    assert!(contains(body, &[[0x89, 0x0D].as_slice(), &at(1)].concat()));
    assert!(contains(body, &[[0x89, 0x15].as_slice(), &at(2)].concat()));
    // get_scale reads them back.
    let get = pe.slot_body(Class::Graphics, 0xA0, 0x24);
    assert!(contains(get, &[[0x8B, 0x0D].as_slice(), &at(0)].concat()));
}

#[test]
fn sprite_inits_write_the_fields_the_placeholder_writes() {
    let Some(pe) = pe() else { return };
    let compile = (pe.image_base + jgl_ffi::blit::COMPILE_RVA).to_le_bytes();
    // 8-bit (0xAA60) and 16-bit (0x9F50) inits: width +0x30, height +0x34,
    // bits +0x20, bytes +0x24, pixels +0x14; both test the compile switch.
    for (at, bits) in [(0xAA60, 8u8), (0x9F50, 16)] {
        let body = pe.at(at, 0x80);
        for store in [&[0x89, 0x6B, 0x30][..], &[0x89, 0x43, 0x14], &[0xC7, 0x43, 0x20, bits, 0, 0, 0]] {
            assert!(contains(body, store), "{at:#x} {store:x?}");
        }
        assert!(contains(body, &[[0xA1].as_slice(), &compile].concat()), "{at:#x}");
    }
    // init calls its clear through the table first ([eax + 8]) and the
    // colour table setter after (call [edx + 0x10]).
    let init = pe.slot_body(Class::Sprite, 0x04, 0x64);
    assert!(contains(init, &[0xFF, 0x52, 0x10]));
    // The destructor (0x7ED0) calls clear (0x7F40) directly at 0x7F14:
    // the delete slot must let go of the placeholder first.
    let call = pe.at(0x7F14, 5);
    assert_eq!(call[0], 0xE8);
    assert_eq!(0x7F19u32.wrapping_add(u32::from_le_bytes(call[1..5].try_into().unwrap())), 0x7F40);
    // clear frees +0x14 and zeroes it: mov eax, [esi + 0x14] ... mov [esi + 0x14], edi.
    let clear = pe.slot_body(Class::Sprite, 0x08, 0x80);
    assert!(contains(clear, &[0x8B, 0x46, 0x14]) && contains(clear, &[0x89, 0x7E, 0x14]));
    // duplicate copies table, flags, sizes, key, bits and bytes.
    let dup = pe.slot_body(Class::Sprite, 0x0C, 0x40);
    for f in [0x10u8, 0x18, 0x34, 0x30, 0x2C, 0x28, 0x20, 0x24] {
        assert!(dup.windows(3).any(|w| w[0] == 0x8B && w[2] == f), "+{f:#x}");
    }
}

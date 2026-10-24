//! The exe call sites `civ3_ui::blit` knows, checked against the game's
//! exe (the PCGames build, unmodded and as C3X leaves it). Needs the
//! dataset copy (`CIV3_EXE`, or the paths below); skipped without it.

use civ3_ui::blit::{CALL_SITES, SCALED};

const DIR: &str = "/mnt/datasets/winvm/civ3-src-c3x28/Civilization III Complete/Conquests";

/// The exe's bytes at virtual address `va` (image base 0x400000).
fn reader(path: &str) -> Option<impl Fn(u32, usize) -> Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    let u16_at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    let pe = u32_at(0x3C) as usize;
    let image_base = u32_at(pe + 24 + 28);
    let table = pe + 24 + u16_at(pe + 20) as usize;
    let sections: Vec<(u32, u32, u32)> =
        (0..u16_at(pe + 6) as usize).map(|i| table + i * 40).map(|s| (u32_at(s + 12), u32_at(s + 8).max(u32_at(s + 16)), u32_at(s + 20))).collect();
    Some(move |va: u32, len: usize| {
        let rva = va - image_base;
        let &(start, _, file) = sections.iter().find(|&&(s, n, _)| rva >= s && rva < s + n).expect("in a section");
        let o = (file + rva - start) as usize;
        data[o..o + len].to_vec()
    })
}

fn exes() -> Vec<String> {
    let paths: Vec<String> = match std::env::var("CIV3_EXE") {
        Ok(p) => vec![p],
        Err(_) => ["Civ3Conquests-Unmodded.exe", "Civ3Conquests.exe"].iter().map(|f| format!("{DIR}/{f}")).collect(),
    };
    let found: Vec<String> = paths.into_iter().filter(|p| std::path::Path::new(p).exists()).collect();
    if found.is_empty() {
        eprintln!("the game's exe not found (CIV3_EXE): skipped");
    }
    found
}

/// `call [reg + disp]` ending at `ret`, for any of eax..edi.
fn calls_slot(at: &dyn Fn(u32, usize) -> Vec<u8>, ret: u32, disp: u32) -> bool {
    if disp < 0x80 {
        let b = at(ret - 3, 3);
        b[0] == 0xFF && (0x50..0x58).contains(&b[1]) && b[1] != 0x54 && b[2] == disp as u8
    } else {
        let b = at(ret - 6, 6);
        b[0] == 0xFF && (0x90..0x98).contains(&b[1]) && b[1] != 0x94 && b[2..6] == disp.to_le_bytes()
    }
}

#[test]
fn every_wrapper_site_is_its_slot_call() {
    for path in exes() {
        let at = reader(&path).unwrap();
        for (w, slot, ret) in CALL_SITES {
            assert!(calls_slot(&at, ret, slot as u32), "{path}: {w:?} at {ret:#x}");
        }
        for (set_at, inner, outer) in SCALED {
            assert!(calls_slot(&at, set_at, 0x9C), "{path}: {outer:?}'s set_scale at {set_at:#x}");
            // The scaled wrapper then calls the unscaled one (the next call
            // instruction is `call rel32` to it).
            let entry = match inner {
                civ3_ui::blit::Wrapper::A4 => 0x5F_84A0,
                civ3_ui::blit::Wrapper::Blend => 0x5F_8620,
                civ3_ui::blit::Wrapper::A4b => 0x5F_8760,
                _ => unreachable!(),
            };
            let code = at(set_at, 0x60);
            let calls = (0..code.len() - 5).filter(|&i| code[i] == 0xE8).any(|i| {
                let rel = i32::from_le_bytes(code[i + 1..i + 5].try_into().unwrap());
                (set_at + i as u32 + 5).wrapping_add(rel as u32) == entry
            });
            assert!(calls, "{path}: {outer:?} calls {inner:?}");
        }
    }
}

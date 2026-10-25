//! C3X R28 improvement-row producer in the supported PCGames binary.
//! The hook wraps its Sprite::draw call at 0x421315. It retains the native
//! return value and all row drawing. Predicate call-site hooks retain the
//! actual answers, including extended/free improvements and chains.

use crate::{Build, c3x::C3xState, c3x_layout as X, controls::Mem, layout as L};
use inproc::{Ptr32, hook::HookError};

pub const SITE: u32 = 0x421315;
pub const SITE_BYTES: [u8; 5] = [0xe8, 0xfe, 0xd8, 0x9c, 0x00];
const STATE_POINTER: u32 = 0xe274e0;
const CITY: u32 = 0x7423ac;
const RESOURCES: u32 = 0x9c7194;
const RESOURCE_COUNT: u32 = 0x9c3d64;
pub const GENERATE_SITE: u32 = 0xdeed65;
pub const GENERATE_BYTES: [u8; 5] = [0xe8, 0x19, 0x0d, 0xfb, 0xff];
pub const ACTIVE_SITE: u32 = 0xdeed87;
pub const ACTIVE_BYTES: [u8; 5] = [0xe8, 0x87, 0x8d, 0xfb, 0xff];
pub const INPUTS_SITE: u32 = 0xdeeda9;
pub const INPUTS_BYTES: [u8; 5] = [0xe8, 0x84, 0xa8, 0xfb, 0xff];

inproc::fastcall_hook! {
    /// C3X patch_Sprite_draw_improv_img_on_city_form, fastcall/thiscall,
    /// four stack arguments, `ret 0x10`. PCG 0x421315 -> 0xDEEC18.
    pub mod draw(sprite, edx; canvas, x, y, color_table) -> i32;
}
inproc::cdecl_hook! { pub mod can_generate(civ, mill) -> u8; }
inproc::cdecl_hook! { pub mod active_building(city, improvement) -> u8; }
inproc::cdecl_hook! { pub mod has_inputs(city, improvement) -> u8; }

#[derive(Clone, Copy, Debug)]
pub struct Producer {
    state: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mill {
    pub address: u32,
    pub improvement: i32,
    pub resource: i32,
    pub flags: u16,
    pub icon: i32,
    pub class: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub city: u32,
    pub city_id: i32,
    pub civ: i32,
    pub improvement: i32,
    pub sheet: u32,
    pub mills: Vec<Mill>,
}

/// Raw fields only. A missing field or invalid configured ID is an error,
/// never an invented resource or a substituted icon.
pub fn read(mem: &dyn Mem, state: u32) -> Option<Row> {
    let city = mem.u32(CITY).filter(|&p| p != 0)?;
    let improvement = mem.i32(state + X::STATE_DRAWING_ICONS_FOR_IMPROV_ID)?;
    let count = mem.i32(state + X::STATE_CURRENT_CONFIG_COUNT_MILLS)?;
    if !(0..=32768).contains(&count) { return None; }
    let mut row = Row { city, city_id: mem.i32(city + 0x1c + L::CITY_BODY_ID)?,
        civ: i32::from(mem.u8(city + 0x1c + L::CITY_BODY_CIVID)? as i8),
        improvement, sheet: mem.u32(state + X::STATE_RESOURCES_SHEET)?, mills: Vec::new() };
    if count == 0 || improvement < 0 || row.sheet == 0 { return Some(row); }
    let mills = mem.u32(state + X::STATE_CURRENT_CONFIG_MILLS).filter(|&p| p != 0)?;
    let packed = mem.read_bytes(mills, count as usize * X::MILL_SIZE as usize)?;
    let resources = mem.u32(RESOURCES).filter(|&p| p != 0)?;
    let resource_count = mem.i32(RESOURCE_COUNT)?;
    for i in 0..count as u32 {
        let address = mills.checked_add(i * X::MILL_SIZE)?;
        let bytes = &packed[i as usize * X::MILL_SIZE as usize..][..X::MILL_SIZE as usize];
        let short = |at: u32| i16::from_le_bytes(bytes[at as usize..at as usize + 2].try_into().unwrap()) as i32;
        let id = short(X::MILL_IMPROV_ID);
        if id != improvement { continue; }
        let resource = short(X::MILL_RESOURCE_ID);
        if resource < 0 || resource >= resource_count { return None; }
        let r = resources.checked_add(resource as u32 * L::RESOURCE_TYPE_SIZE)?;
        row.mills.push(Mill { address, improvement: id, resource, flags: short(X::MILL_FLAGS) as u16,
            icon: mem.i32(r + L::RESOURCE_TYPE_ICONID)?, class: mem.i32(r + L::RESOURCE_TYPE_CLASS)? });
    }
    Some(row)
}

impl Producer {
    /// Only the audited R28 body. A different R28 rebuild must be audited
    /// before adding its producer addresses; a matching version alone is
    /// insufficient to call arbitrary code.
    pub fn find(build: Build) -> Result<Option<Self>, HookError> {
        if build != Build::PcGames || C3xState::find(0x400000).map_err(|_| HookError::Unsupported)?.is_none() { return Ok(None); }
        let check = |at: u32, expected: &[u8]| -> Result<(), HookError> {
            let found = Ptr32::<u8>::new(at).read_array(expected.len())?;
            if found == expected { Ok(()) } else { Err(HookError::UnexpectedCode { addr: at, found, expected: expected.to_vec() }) }
        };
        check(SITE, &SITE_BYTES)?;
        check(0xdeec18, &[0x55, 0x89, 0xe5, 0x81, 0xec, 0x88, 0, 0, 0])?;
        // Bind each helper to the call inside this producer, as well as its prologue.
        check(GENERATE_SITE, &GENERATE_BYTES)?;
        check(ACTIVE_SITE, &ACTIVE_BYTES)?;
        check(INPUTS_SITE, &INPUTS_BYTES)?;
        let state = Ptr32::<u32>::new(STATE_POINTER).read()?;
        let section = crate::c3x::section(0x400000, b".c3xdat\0").map_err(|_| HookError::Unsupported)?;
        if section.map(|s| s.0) != Some(state) { return Err(HookError::Unsupported); }
        check(0xd9fa83, &[0x55, 0x89, 0xe5, 0x81, 0xec, 4, 0, 0, 0])?;
        check(0xda7b13, &[0x55, 0x89, 0xe5, 0x81, 0xec, 8, 0, 0, 0])?;
        check(0xda9632, &[0x55, 0x89, 0xe5, 0x81, 0xec, 0, 0, 0, 0])?;
        Ok(Some(Self { state }))
    }

    pub fn row(self) -> Option<Row> { read(&crate::controls::Live, self.state) }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Memory(BTreeMap<u32, u8>);
    impl Memory {
        fn put(&mut self, p: u32, v: u32) { for (i, b) in v.to_le_bytes().into_iter().enumerate() { self.0.insert(p + i as u32, b); } }
        fn bytes(&mut self, p: u32, bytes: &[u8]) { for (i, &b) in bytes.iter().enumerate() { self.0.insert(p + i as u32, b); } }
    }
    impl Mem for Memory {
        fn u32(&self, p: u32) -> Option<u32> { Some(u32::from_le_bytes([self.u8(p)?, self.u8(p + 1)?, self.u8(p + 2)?, self.u8(p + 3)?])) }
        fn u8(&self, p: u32) -> Option<u8> { self.0.get(&p).copied() }
    }

    #[test]
    fn r28_row_fixture_reads_runtime_ids_and_signed_packed_mills() {
        let (state, city, sheet, mills, resources) = (0x10000, 0x20000, 0x30000, 0x40000, 0x50000);
        let mut m = Memory::default();
        m.put(CITY, city); m.put(city + 0x1c + L::CITY_BODY_ID, 23); m.put(city + 0x1c + L::CITY_BODY_CIVID, 5);
        m.put(state + X::STATE_DRAWING_ICONS_FOR_IMPROV_ID, 71);
        m.put(state + X::STATE_CURRENT_CONFIG_MILLS, mills); m.put(state + X::STATE_CURRENT_CONFIG_COUNT_MILLS, 3);
        m.put(state + X::STATE_RESOURCES_SHEET, sheet); m.put(RESOURCES, resources); m.put(RESOURCE_COUNT, 100);
        // Packed struct mill is six bytes, including a final signed short.
        m.bytes(mills, &[71, 0, 82, 0, 8, 0, 99, 0, 1, 0, 0, 0, 71, 0, 2, 0, 1, 0]);
        for (id, icon, class) in [(82, 7, 0), (2, 19, 2)] {
            m.put(resources + id * L::RESOURCE_TYPE_SIZE + L::RESOURCE_TYPE_ICONID, icon);
            m.put(resources + id * L::RESOURCE_TYPE_SIZE + L::RESOURCE_TYPE_CLASS, class);
        }
        let row = read(&m, state).unwrap();
        assert_eq!((row.city, row.city_id, row.civ, row.improvement, row.sheet), (city, 23, 5, 71, sheet));
        assert_eq!(row.mills.iter().map(|m| (m.resource, m.icon, m.class, m.flags)).collect::<Vec<_>>(), [(82, 7, 0, 8), (2, 19, 2, 1)]);
        m.put(state + X::STATE_CURRENT_CONFIG_COUNT_MILLS, 0);
        assert!(read(&m, state).unwrap().mills.is_empty(), "disabled/no-mill configuration");
        m.put(state + X::STATE_CURRENT_CONFIG_COUNT_MILLS, 3);
        m.bytes(mills + 2, &[0xff, 0xff]);
        assert!(read(&m, state).is_none(), "invalid ID is not mapped to an arbitrary resource");
    }
}

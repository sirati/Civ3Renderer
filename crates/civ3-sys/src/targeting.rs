//! Facts about the game's targeting modes (docs §10.15): the go-to path the
//! game shows while a unit waits for its destination, found with the game's
//! own path finder, and its move points.
//!
//! Only the PCGames build is decoded; the others have no sites (`None`).

use inproc::Ptr32;

use crate::modal::FunctionSite;
use crate::{Build, Civ3Error, layout as L};

inproc::thiscall_fn! {
    /// `int Trade_Net::set_unit_path(this, from_x, from_y, to_x, to_y,
    /// Unit *unit, int civ_id, int flags, int *out_path_length_in_mp)`
    /// (C3X redirects it): above 0 if there is a path; it leaves the path
    /// in the trade net's internal map (see [`direction`]).
    pub fn set_unit_path(trade_net; from_x, from_y, to_x, to_y, unit, civ, flags, out_length) -> u32;
}

inproc::thiscall_fn! {
    /// `int Unit::get_max_move_points(this)`.
    pub fn max_move_points(unit;) -> u32;
}

inproc::fastcall_hook! {
    /// `void Main_Screen_Form::scroll_at_edge(this)` (0x4DE4F0): the main
    /// screen's part of the game's 66 ms timer (0x4DE680), run on the
    /// game's thread in the human player's turn while no menu is open.
    pub mod screen_tick(screen, edx;) -> ();
}

/// The flags the main screen asks the path finder with (0x4E43A8).
pub const PATH_FLAGS: u32 = 0x2001;

/// `Unit_Mode_Actions` values that are go-to modes (their target shows a
/// path, not a tile frame).
pub const GO_TO: i32 = 0x1;
pub const STACK_GO_TO: i32 = 0x12;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sites {
    /// The `Trade_Net` object (`p_trade_net`).
    pub trade_net: u32,
    pub set_unit_path: FunctionSite,
    pub max_move_points: FunctionSite,
    /// [`screen_tick`].
    pub screen_tick: FunctionSite,
}

pub fn sites(build: Build) -> Option<Sites> {
    match build {
        Build::PcGames => Some(Sites {
            trade_net: 0xB7_2848,
            // sub esp,68h / push ebx / push ebp / push esi
            set_unit_path: (0x58_02A0, &[0x83, 0xEC, 0x68, 0x53, 0x55, 0x56]),
            // sub esp,8 / push ebx / push ebp / mov ebp,ecx
            max_move_points: (0x5B_E180, &[0x83, 0xEC, 0x08, 0x53, 0x55, 0x8B, 0xE9]),
            // sub esp,1Ch / push ebp / push esi / push edi
            screen_tick: (0x4D_E4F0, &[0x83, 0xEC, 0x1C, 0x55, 0x56, 0x57]),
        }),
        Build::Gog | Build::Steam => None,
    }
}

/// `neighbor_index_to_diff` (0x5E6D80) for the eight neighbours: 1 NE,
/// 2 E, 3 SE, 4 S, 5 SW, 6 W, 7 NW, 8 N (tile units: x counts half tile
/// widths).
pub fn neighbor_diff(n: u32) -> Option<(i32, i32)> {
    Some(match n {
        1 => (1, -1),
        2 => (2, 0),
        3 => (1, 1),
        4 => (0, 2),
        5 => (-1, 1),
        6 => (-2, 0),
        7 => (-1, -1),
        8 => (0, -2),
        _ => return None,
    })
}

/// `Trade_Net::get_direction_from_internal_map(x, y, true)` (0x581090):
/// the neighbour (1..8) a path left by [`set_unit_path`] goes on to from
/// tile (x, y) back towards its start, 0 if none. The map holds 16-byte
/// entries, `(width / 2)·y + x / 2`, from the pointer at +0x20; an entry
/// whose first dword is negative has the direction in byte 3's low bits.
pub fn direction(trade_net: u32, x: i32, y: i32) -> Result<u32, Civ3Error> {
    let net = Ptr32::<u8>::new(trade_net);
    let width: i32 = net.offset::<i32>(4)?.read()?;
    let entries: Ptr32<u8> = net.offset::<Ptr32<u8>>(0x20)?.read()?;
    if entries.is_null() || x < 0 || y < 0 {
        return Ok(0);
    }
    let index = (width / 2) * y + x / 2;
    let entry = entries.offset::<i32>(u32::try_from(index).unwrap_or(0) * 16)?;
    let first: i32 = entry.read()?;
    if first >= 0 {
        return Ok(0);
    }
    let byte3: u8 = entries.offset::<u8>(u32::try_from(index).unwrap_or(0) * 16 + 3)?.read()?;
    Ok(u32::from(byte3 & 7) + 1)
}

/// The tiles of the path from `to` back to `from`, both included, as
/// [`direction`] leads (0x4E43D1..0x4E4598); just the two ends if the
/// first step has no direction (the main screen then draws one line).
/// `size`/`wrap`: the map's, in tile units.
pub fn walk(mut direction: impl FnMut(i32, i32) -> u32, from: (i32, i32), to: (i32, i32), size: (i32, i32), wrap: (bool, bool)) -> Vec<(i32, i32)> {
    let mut out = vec![to];
    let (mut x, mut y) = to;
    // (A path never visits a tile twice; the bound keeps a corrupt map from
    // looping.)
    for _ in 0..(size.0.max(1) * size.1.max(1)) / 2 + 1 {
        let Some((dx, dy)) = neighbor_diff(direction(x, y)) else {
            if out.len() == 1 {
                out.push(from);
            }
            return out;
        };
        let wrapped = |v: i32, n: i32, wraps: bool| if wraps { v.rem_euclid(n.max(1)) } else { v };
        (x, y) = (wrapped(x + dx, size.0, wrap.0), wrapped(y + dy, size.1, wrap.1));
        out.push((x, y));
        if (x, y) == from {
            return out;
        }
    }
    out
}

/// The moves a unit has used this turn (`Unit_Body.Moves`, in move points).
pub fn moves_used(unit: u32) -> Result<i32, Civ3Error> {
    Ok(Ptr32::<u8>::new(unit).offset::<i32>(L::UNIT_BODY + L::UNIT_BODY_MOVES)?.read()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_are_the_eight_steps() {
        let all: Vec<_> = (1..=8).filter_map(neighbor_diff).collect();
        assert_eq!(all.len(), 8);
        assert!(all.iter().all(|(dx, dy)| (dx + dy) % 2 == 0 && (dx.abs() + dy.abs()) == 2));
        assert_eq!(neighbor_diff(0), None);
        assert_eq!(neighbor_diff(9), None);
    }

    #[test]
    fn walk_follows_directions_back_to_the_start() {
        // From (10, 10) east twice, then north-east: the map leads back.
        let dirs = |x: i32, y: i32| match (x, y) {
            (15, 9) => 5,  // SW to (14, 10)
            (14, 10) => 6, // W to (12, 10)
            (12, 10) => 6, // W to (10, 10)
            _ => 0,
        };
        assert_eq!(walk(dirs, (10, 10), (15, 9), (40, 40), (false, false)), vec![(15, 9), (14, 10), (12, 10), (10, 10)]);
        // No direction at the target: a single line from the start.
        assert_eq!(walk(|_, _| 0, (10, 10), (11, 11), (40, 40), (false, false)), vec![(11, 11), (10, 10)]);
        // Across the seam of a map that wraps east-west.
        let seam = |x: i32, _y: i32| if x == 1 { 6 } else { 0 };
        assert_eq!(walk(seam, (99, 10), (1, 10), (100, 50), (true, false)), vec![(1, 10), (99, 10)]);
    }
}

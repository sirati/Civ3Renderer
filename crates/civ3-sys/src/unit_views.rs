//! The animator's viewport-limited unit list and the functions it uses to
//! choose a tile's units. The original elapsed-time update can consume a
//! separate full-map unit/effect argument array without moving the camera
//! or overwriting the animator's drawing lists. All calls and live pointers
//! remain on the game's thread.

use inproc::Ptr32;

use crate::modal::FunctionSite;
use crate::{Build, Civ3, Civ3Error, TileRecord, layout as L};

inproc::fastcall_hook! {
    /// `void Animator::build_unit_list(this)`.
    pub mod build_unit_list(animator, edx;) -> ();
}

inproc::fastcall_hook! {
    /// `void Units_Image_Data::update(this, float seconds, Unit **units,
    /// int count, vector<FLC_Animation *> *effects)` (PCGames 0x405FF0).
    /// `seconds` is the unchanged IEEE-754 stack word, not an integer time.
    pub mod advance_animations(images, edx; seconds, units, count, effects) -> ();
}

inproc::thiscall_fn! {
    /// `bool Unit::is_visible_to_civ(this, civ, check_detectors)`.
    pub fn visible_to(unit; civ, check_detectors) -> u32;
}

inproc::thiscall_fn! {
    /// `bool Unit::tile_visible_now(this, civ, x, y)` (0x4F0DE0),
    /// including debug and shared vision.
    pub fn tile_visible(unit; civ, x, y) -> u32;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sites {
    pub build_unit_list: FunctionSite,
    pub advance_animations: FunctionSite,
    pub visible_to: FunctionSite,
    pub tile_visible: FunctionSite,
}

pub fn sites(build: Build) -> Option<Sites> {
    match build {
        Build::PcGames => Some(Sites {
            // sub esp,14h / push ebx / mov ebx,ecx
            build_unit_list: (0x4E_F4D0, &[0x83, 0xEC, 0x14, 0x53, 0x8B, 0xD9]),
            // fld [esp+4] / fcomp [6653B4h]. Both instructions use
            // unchanged stack/absolute addressing in the trampoline.
            advance_animations: (0x40_5FF0, &[0xD9, 0x44, 0x24, 0x04, 0xD8, 0x1D, 0xB4, 0x53, 0x66, 0x00]),
            // sub esp,14h / push ebx / mov ebx,[esp+1Ch]
            visible_to: (0x5B_B360, &[0x83, 0xEC, 0x14, 0x53, 0x8B, 0x5C, 0x24, 0x1C]),
            // push ecx / push ebx / mov ebx,ecx / push esi
            tile_visible: (0x4F_0DE0, &[0x51, 0x53, 0x8B, 0xD9, 0x56]),
        }),
        Build::Gog | Build::Steam => None,
    }
}

/// The animator's extra per-unit display gates (docs §10.2), relative to
/// `Unit *`. `bypass_fog` is used for effects that remain visible while
/// their animation crosses a tile boundary.
pub fn display_flags(unit: u32) -> Result<(bool, bool), Civ3Error> {
    let p = Ptr32::<u8>::new(unit);
    let suppressed = p.offset::<u8>(0x259)?.read()? != 0;
    let bypass_fog = p.offset::<u8>(0x24F)?.read()? != 0;
    Ok((suppressed, bypass_fog))
}


/// Native animation-list membership, independent of its drawing array.
/// This is Unit+0x24D, *not* the selectable-unit flag at +0x24C. The worker
/// animation queue (0x406963) and pending-animation loop (0x4EEC97) use it
/// to decide whether to play an animation or finish/seek it immediately.
/// `reset` mirrors the original list builder's loaded-animation reset.
pub fn set_animation_member(unit: u32, member: bool, reset: bool) -> Result<(), Civ3Error> {
    let p = Ptr32::<u8>::new(unit);
    p.offset::<u8>(0x24D)?.write(&u8::from(member))?;
    if reset && p.offset::<u8>(0x38D)?.read()? != 0 {
        p.offset::<u32>(0x3A8)?.write(&0)?;
    }
    Ok(())
}

/// Tiles of the explicitly pending native unit animations. Refreshing
/// their defender choices avoids waiting for the 33-ms full-map selector
/// cache when movement or combat starts on a newly selected stack.
pub fn pending_unit_tiles(game: Civ3) -> Result<Vec<(i32, i32)>, Civ3Error> {
    let animator = Ptr32::<u8>::new(game.addrs.main_screen_form + L::MAIN_SCREEN_FORM_ANIMATOR);
    let count = animator.offset::<i32>(0x180C)?.read()?.clamp(0, 512) as usize;
    let pointers = animator.offset::<u32>(0x100C)?.read_array(count)?;
    let mut out = Vec::new();
    for ptr in pointers {
        if ptr == 0 { continue; }
        let xy: [i32; 2] = Ptr32::<u8>::new(ptr).offset(0x24)?.read()?;
        out.push((xy[0], xy[1]));
    }
    Ok(out)
}

/// A live city or tile effect. These addresses are consumed synchronously
/// on the game thread and never saved between frames. Global extras stay
/// in the original animator vector, so its lifetime rules still apply.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EffectAnimation {
    pub animation: u32,
    pub tile: (i32, i32),
}

/// All city/tile effect objects, without the native camera range test.
/// The original effect builder must have run first: it owns expired tile
/// effects and removes them from the linked list.
pub fn effect_animations(game: Civ3) -> Result<Vec<EffectAnimation>, Civ3Error> {
    let mut out = Vec::new();
    let list = Ptr32::<u8>::new(game.addrs.cities);
    let items: Ptr32<[u32; 2]> = list.offset::<Ptr32<[u32; 2]>>(L::CITIES_CITIES)?.read()?;
    let last: i32 = list.offset::<i32>(L::CITIES_LASTINDEX)?.read()?;
    if !items.is_null() {
        for [_, body] in items.read_array(last.clamp(-1, 0xFFFF).saturating_add(1) as usize)? {
            if body == 0 { continue; }
            let body = Ptr32::<u8>::new(body);
            if body.offset::<i32>(L::CITY_BODY_FIELD_A4)?.read()? > 0 {
                out.push(EffectAnimation {
                    animation: body.offset::<u8>(L::CITY_BODY_STRUCT_198)?.raw(),
                    tile: (i32::from(body.offset::<i16>(L::CITY_BODY_X)?.read()?),
                           i32::from(body.offset::<i16>(L::CITY_BODY_Y)?.read()?)),
                });
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut effect: Ptr32<u8> = Ptr32::<Ptr32<u8>>::new(game.addrs.tile_effects).read()?;
    for _ in 0..4096 {
        if effect.is_null() || !seen.insert(effect.raw()) { break; }
        out.push(EffectAnimation {
            animation: effect.offset::<u8>(L::TILE_ANIMATED_EFFECT_FLC_ANIMATION)?.raw(),
            tile: (effect.offset::<i32>(L::TILE_ANIMATED_EFFECT_V)?.read()?,
                   effect.offset::<i32>(L::TILE_ANIMATED_EFFECT_V + 4)?.read()?),
        });
        effect = effect.offset::<Ptr32<u8>>(L::TILE_ANIMATED_EFFECT_FIELD_194 + 4)?.read()?;
    }
    Ok(out)
}

/// The four visibility words used by 0x4EFC30, including shared vision.
/// The city/tile effect path has no debug-mode or owner override.
pub fn effect_tile_visible(tile: &TileRecord, viewer: i32) -> bool {
    (0..32).contains(&viewer)
        && (tile.fow_status() | tile.v3() | tile.visibility() | tile.visibility_d0()) & (1u32 << viewer) != 0
}

/// Identity of the viewer used for effect masks. This is Leader.ID at
/// +0x1C, loaded by both city and tile branches of 0x4EFC30.
pub fn effect_viewer(game: Civ3, player: i32) -> Result<i32, Civ3Error> {
    if !(0..32).contains(&player) { return Err(Civ3Error::NoGame); }
    Ok(Ptr32::<i32>::new(game.addrs.leaders + player as u32 * L::LEADER_SIZE + 0x1C).read()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_pointer_width = "32")]
    fn animation_membership_preserves_selection_and_matches_the_native_reset() {
        let mut bytes = [0x55u8; 0x404];
        bytes[0x38D] = 1;
        let ptr = bytes.as_ptr() as usize as u32;
        set_animation_member(ptr, true, false).unwrap();
        assert_eq!(bytes[0x24D], 1);
        assert_eq!(bytes[0x24C], 0x55, "selectable-unit membership is a different byte");
        assert_eq!(&bytes[0x3A8..0x3AC], &[0x55; 4]);
        set_animation_member(ptr, false, true).unwrap();
        assert_eq!(bytes[0x24D], 0);
        assert_eq!(&bytes[0x3A8..0x3AC], &[0; 4]);
        for (i, byte) in bytes.iter().enumerate() {
            if i != 0x24D && i != 0x38D && !(0x3A8..0x3AC).contains(&i) {
                assert_eq!(*byte, 0x55, "unexpected write at {i:#x}");
            }
        }
        bytes[0x38D] = 0;
        bytes[0x3A8..0x3AC].fill(0x77);
        set_animation_member(ptr, false, true).unwrap();
        assert_eq!(&bytes[0x3A8..0x3AC], &[0x77; 4], "unloaded animations retain their native state");
    }

    #[test]
    fn effects_use_current_visibility_and_reject_exploration_alone() {
        let mut tile = TileRecord([0; L::TILE_SIZE as usize]);
        for word in [L::TILE_BODY_FOWSTATUS, L::TILE_BODY_V3, L::TILE_BODY_VISIBILITY, L::TILE_BODY_FIELD_D0_VISIBILITY] {
            let at = (L::TILE_BODY + word) as usize;
            tile.0[at..at + 4].copy_from_slice(&(1u32 << 9).to_le_bytes());
            assert!(effect_tile_visible(&tile, 9));
            assert!(!effect_tile_visible(&tile, 8));
            assert!(!effect_tile_visible(&tile, -1));
            assert!(!effect_tile_visible(&tile, 32));
            tile.0[at..at + 4].fill(0);
        }
        let at = (L::TILE_BODY + L::TILE_BODY_FOG_OF_WAR) as usize;
        tile.0[at..at + 4].copy_from_slice(&(1u32 << 9).to_le_bytes());
        assert!(!effect_tile_visible(&tile, 9), "previously explored fog does not advance a hidden effect");
    }
}

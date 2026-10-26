//! The game's sprite blits, as the jgl sprite group gets them, in the
//! game's terms: which of the exe's `Sprite` drawing functions made the
//! call, and its arguments (the game's `Sprite`, `PCX_Image` and
//! `PCX_Color_Table` objects, not jgl's).
//!
//! Each of the exe's functions ends in one jgl sprite slot (`docs/
//! civ3-map-rendering.md` §16.4); a slot's call site (the return address
//! jgl-ffi keeps) names the function. The scaled ones set the draw scale,
//! call an unscaled one and set it back: a blit is theirs when the scale
//! was set at their `set_scale` call. The PCGames build's addresses [V,
//! the binary tests read the exe]; another build's calls have no
//! [`Wrapper`] and are left to jgl.
//!
//! The owners map back as the exe makes the objects [V, the exe's five
//! `new_sprite` calls and `PCX_Color_Table`'s constructor]: a `JGLSprite`'s
//! owner is its `Sprite`'s +4, a `JGL_Color_Table`'s its `PCX_Color_Table`,
//! a `JGL_Image`'s its `PCX_Image`.

use jgl::{BlitCall, BlitHooks};

use crate::draw::canvas_clip;

/// The exe function a blit came from (PCGames build).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Wrapper {
    /// `Sprite::draw(canvas, x, y, ct)` 0x5F8210, slot +0x44.
    Draw,
    /// `Sprite::draw_on_map(canvas, x, y, num_x, num_y, den, ct)` 0x5F8060,
    /// slot +0x44 at that scale.
    DrawOnMap,
    /// 0x5F81A0 `(canvas, x, y, ct)`, slot +0x5C.
    Variant,
    /// 0x5F84A0 `(canvas, x, y, a4, ct)`, slot +0x74.
    A4,
    /// 0x5F83E0, 0x5F84A0 scaled `(canvas, x, y, a4, num_x, num_y, den, ct)`.
    ScaledA4,
    /// `Sprite::draw_blend(canvas, x, y, lut, ct)` 0x5F8620, slot +0x7C.
    Blend,
    /// 0x5F8560, draw_blend scaled `(canvas, x, y, lut, num_x, num_y, den, ct)`.
    OnMapBlend,
    /// 0x5F8760 `(canvas, x, y, a4, ct)`, slot +0x84.
    A4b,
    /// 0x5F86A0, 0x5F8760 scaled.
    ScaledA8,
    /// `Sprite::draw_with_color_table(canvas, canvas2, x, y, table, ct)`
    /// 0x5F87E0, slot +0x8C.
    WithColorTable,
    /// 0x5F8870 `(canvas, canvas2, x, y, num_x, num_y, den, flags, ct)`,
    /// slot +0x88.
    ScaledColored,
    /// `Sprite::draw_opacity(canvas, x, y, opacity, ct, flags)` 0x5F89A0,
    /// slot +0x94.
    Opacity,
    /// `Sprite::draw_alpha(mask, background, dest, x, y, ct)` 0x5F8310,
    /// slot +0x50 (no crop offset).
    Alpha,
    /// `Sprite::draw_masked(mask, dest, x, y, ct)` 0x5F8380, slot +0x58
    /// (no crop offset).
    Masked,
}

/// The return address of each wrapper's slot call, and its slot.
pub const CALL_SITES: [(Wrapper, u16, u32); 11] = [
    (Wrapper::Draw, 0x44, 0x5F_82F2),
    (Wrapper::DrawOnMap, 0x44, 0x5F_8160),
    (Wrapper::Variant, 0x5C, 0x5F_81FE),
    (Wrapper::A4, 0x74, 0x5F_8548),
    (Wrapper::Blend, 0x7C, 0x5F_8683),
    (Wrapper::A4b, 0x84, 0x5F_87C6),
    (Wrapper::WithColorTable, 0x8C, 0x5F_884F),
    (Wrapper::ScaledColored, 0x88, 0x5F_8954),
    (Wrapper::Opacity, 0x94, 0x5F_8A0B),
    (Wrapper::Alpha, 0x50, 0x5F_8365),
    (Wrapper::Masked, 0x58, 0x5F_83C8),
];

/// The scaled wrappers: the return address of their first `set_scale`
/// call, the unscaled wrapper they call, and themselves.
pub const SCALED: [(u32, Wrapper, Wrapper); 3] = [
    (0x5F_8423, Wrapper::A4, Wrapper::ScaledA4),
    (0x5F_85A3, Wrapper::Blend, Wrapper::OnMapBlend),
    (0x5F_86E9, Wrapper::A4b, Wrapper::ScaledA8),
];

/// The wrapper `call` came from, if it is one of the exe's.
pub fn wrapper(call: &BlitCall) -> Option<Wrapper> {
    let (w, ..) = CALL_SITES.iter().find(|&&(_, slot, at)| slot == call.slot && at == call.caller)?;
    Some(SCALED.iter().find(|&&(set_at, inner, _)| inner == *w && set_at == call.scale.set_at).map_or(*w, |&(.., outer)| outer))
}

/// A `JGLSprite`'s owner as the game's `Sprite` (0: none).
pub fn game_sprite(owner: u32) -> u32 {
    owner.saturating_sub(4)
}

/// A blit in the game's terms.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct GameBlit {
    pub wrapper: Wrapper,
    /// The `Sprite` drawn.
    pub sprite: u32,
    /// The `PCX_Image` drawn onto.
    pub canvas: u32,
    /// The other image: `canvas` of [`Wrapper::WithColorTable`] and
    /// [`Wrapper::ScaledColored`] (their `canvas2` is [`GameBlit::canvas`]),
    /// the background of [`Wrapper::Alpha`]; else 0.
    pub other: u32,
    /// The mask `Sprite` of [`Wrapper::Alpha`] and [`Wrapper::Masked`].
    pub mask: u32,
    /// Where the sprite's stored pixels go: the wrapper's (x, y) plus its
    /// crop offset, scaled by the draw scale, as the wrapper passed them.
    pub at: (i32, i32),
    /// The `PCX_Color_Table` (0: the sprite's own colours).
    pub ct: u32,
    /// The wrapper's own extra argument: `a4`, the blend table, the colour
    /// table's blend tables, the opacity, the flags of
    /// [`Wrapper::ScaledColored`]; else 0.
    pub extra: u32,
    /// [`Wrapper::Opacity`]'s flags.
    pub flags: u32,
    /// Whether jgl compiled the sprite (`jgl::SpriteRef::compiled`).
    pub compiled: bool,
    /// The draw scale (num_x, num_y, den).
    pub scale: [u32; 3],
    /// The canvas's clip at the call (see [`canvas_clip`]).
    pub clip: Option<[i32; 4]>,
}

/// `call` in the game's terms, if an exe wrapper made it.
pub fn game_blit(call: &BlitCall) -> Option<GameBlit> {
    let wrapper = wrapper(call)?;
    let target = call.target?;
    let a = &call.args;
    let (extra, flags) = match wrapper {
        Wrapper::A4 | Wrapper::ScaledA4 | Wrapper::Blend | Wrapper::OnMapBlend | Wrapper::A4b | Wrapper::ScaledA8 => (a[3], 0),
        Wrapper::WithColorTable => (a[4], 0),
        Wrapper::ScaledColored => (a[7], 0),
        Wrapper::Opacity => (a[3], a[5]),
        _ => (0, 0),
    };
    Some(GameBlit {
        wrapper,
        sprite: game_sprite(call.sprite.owner),
        canvas: target.owner,
        other: call.other.map_or(0, |s| s.owner),
        mask: call.mask.map_or(0, |m| game_sprite(m.owner)),
        at: call.at,
        ct: call.table.owner,
        extra,
        flags,
        compiled: call.sprite.compiled,
        scale: call.scale.ratio(),
        clip: canvas_clip(&target),
    })
}

/// Where the blit hooks hand the game's blits.
pub trait BlitSink {
    fn blit(&mut self, blit: &GameBlit, call: &BlitCall);
}

impl<F: FnMut(&GameBlit, &BlitCall)> BlitSink for F {
    fn blit(&mut self, blit: &GameBlit, call: &BlitCall) {
        self(blit, call)
    }
}

/// The blit hooks of the interface: every blit an exe wrapper made, to
/// `sink`. Others (no wrapper: another build, a direct caller) are left to
/// jgl.
pub struct BlitCapture<S> {
    sink: S,
}

impl<S: BlitSink> BlitCapture<S> {
    pub fn new(sink: S) -> BlitCapture<S> {
        BlitCapture { sink }
    }
}

impl<S: BlitSink> BlitHooks for BlitCapture<S> {
    fn blit(&mut self, call: &BlitCall) {
        if let Some(b) = game_blit(call) {
            self.sink.blit(&b, call);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jgl::blit::MAX_BLIT_ARGS;
    use jgl::{ImageId, Rect, Scale, SpriteId, SpriteRef, Surface, TableId, TableRef};

    fn call(slot: u16, caller: u32, set_at: u32) -> BlitCall {
        let target = Surface { row: 100, image: ImageId(0x9100), owner: 0x9000, clip: Rect::new(0, 0, 50, 50), bounds: Rect::new(0, 0, 100, 100) };
        BlitCall {
            slot,
            caller,
            sprite: SpriteRef { sprite: SpriteId(0x7000), owner: 0x6004, table: TableId(0), bits: 8, compiled: false, placeholder: false },
            target: Some(target),
            other: None,
            mask: None,
            at: (10, 20),
            table: TableRef { table: TableId(0x8000), owner: 0x8800 },
            scale: Scale { num_x: 2, num_y: 2, den: 4, set_at },
            args: [0x9100, 10, 20, 0x77, 0x8000, 0, 0, 0, 0][..MAX_BLIT_ARGS].try_into().unwrap(),
        }
    }

    #[test]
    fn call_sites_name_the_wrappers() {
        assert_eq!(wrapper(&call(0x44, 0x5F_82F2, 0)), Some(Wrapper::Draw));
        assert_eq!(wrapper(&call(0x44, 0x5F_8160, 0x5F_811E)), Some(Wrapper::DrawOnMap));
        // The wrong slot for the site, an unknown site.
        assert_eq!(wrapper(&call(0x5C, 0x5F_82F2, 0)), None);
        assert_eq!(wrapper(&call(0x44, 0x40_0000, 0)), None);
        // 0x5F84A0 alone, and called by its scaled wrapper.
        assert_eq!(wrapper(&call(0x74, 0x5F_8548, 0x5F_8488)), Some(Wrapper::A4));
        assert_eq!(wrapper(&call(0x74, 0x5F_8548, 0x5F_8423)), Some(Wrapper::ScaledA4));
        assert_eq!(wrapper(&call(0x7C, 0x5F_8683, 0x5F_85A3)), Some(Wrapper::OnMapBlend));
        assert_eq!(wrapper(&call(0x84, 0x5F_87C6, 0x5F_86E9)), Some(Wrapper::ScaledA8));
    }

    #[test]
    fn game_terms() {
        let b = game_blit(&call(0x74, 0x5F_8548, 0)).unwrap();
        assert_eq!((b.sprite, b.canvas, b.ct, b.extra, b.at), (0x6000, 0x9000, 0x8800, 0x77, (10, 20)));
        assert_eq!(b.scale, [2, 2, 4]);
        assert_eq!(b.clip, Some([0, 0, 50, 50]));
        assert_eq!(game_blit(&BlitCall { target: None, ..call(0x44, 0x5F_82F2, 0) }), None);
    }
}

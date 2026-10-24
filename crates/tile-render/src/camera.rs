//! What part of the world is on screen.

use tile_render_shared::FrameParams;

use crate::effect::Effect;
use crate::world::{ColorModel, Grid, Layout, Pass};

/// Largest source picture of a zoomed view, each way.
pub const MAX_SOURCE: u32 = 4096;

/// A view onto the world.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Camera {
    /// World pixel at the top-left corner of the viewport.
    pub x: i32,
    pub y: i32,
    /// Screen pixels per world pixel, as a fraction (1/1, 1/2, 2/1, …). The
    /// view is drawn at 1/1 and scaled by nearest pixel: no blur, no seams.
    pub zoom: (u32, u32),
    /// Viewport size in screen pixels.
    pub viewport: (u32, u32),
    /// How far sprites may extend beyond their cell, in world pixels:
    /// (left, top, right, bottom). Cells whose sprites could reach into the
    /// viewport by that much are drawn too.
    pub overhang: (u32, u32, u32, u32),
    /// Optional cull rectangle in world pixels (x0, y0, x1, y1): cells are
    /// drawn only if their logical anchor (see `LayerSpec::cull_offset`) is
    /// inside it.
    pub cull: Option<(i32, i32, i32, i32)>,
    /// Where the view sits in the render target (its top-left), when several
    /// views share it (see `TileRenderer::render_views`).
    pub origin: (i32, i32),
    /// Additional target-pixel scissor (x0, y0, x1, y1), independent of
    /// viewport origin so clipping preserves the fractional zoom phase.
    /// A stacked view inherits its group head's clip as well as its own.
    pub target_clip: Option<[i32; 4]>,
    /// An effect drawn over the view after it (e.g. a lens's rounded clip
    /// and rim), by the renderer's effect (`Shaders::effect`). For a
    /// standalone camera without stacked children.
    pub effect: Option<Effect>,
    /// Whether the view shows the world (its cells and world-space sprites;
    /// false: only the view's own sprites, e.g. an interface on top).
    pub cells: bool,
    /// How far the view's screen-space sprites and all clip rectangles
    /// (given in target pixels) are moved (e.g. a driver's labels following
    /// the world when the viewer moves the camera).
    pub screen_shift: (i32, i32),
    /// Whether the view shows its `ZoomArt::Zoomed` sprites (made for its
    /// zoom) instead of their `ZoomArt::Low` twins.
    pub zoomed_art: bool,
    /// Drawn into the previous view's picture (before that is scaled), as
    /// part of it: its `origin` is then relative to that view's, its zoom
    /// that view's (e.g. a game's interface over its map, scaled together).
    pub stacked: bool,
    /// When scaled down: sprites the scaling would shrink to nothing keep one
    /// screen pixel (e.g. the one-pixel lines of an interface shown
    /// smaller). The view is then drawn at its zoom straight into the
    /// target (as views showing zoomed art are from their first zoomed
    /// sprite on), which shows the same but for those. A scaled view so
    /// drawn that shows no world (`cells` false, not `stacked`) has no
    /// background of its own: what is below it shows where it draws
    /// nothing (an interface scaled over other views).
    pub hairline: bool,
    /// Shows the world this many times smaller (1: as it is), from the same
    /// art, as a game's zoomed-out view does: cells at the grid's steps
    /// divided, layer offsets and world sprites' positions divided (rounded
    /// toward zero), every sprite drawn as its `scaling` and sample box say
    /// (`tile_render_shared::placed`). `x`, `y`, `cull` and `overhang` are
    /// then in the view's own pixels (the world's divided). Its screen-space
    /// sprites are drawn as they are. Applies before `zoom`: a view at
    /// downscale 2 and zoom 2/1 shows the half-size world twice as big.
    pub downscale: u32,
    /// Category bits hidden in this camera. Category zero is always visible.
    pub hidden_categories: u32,
    /// How UI background sprites are sampled where the view draws them
    /// larger or smaller than their art.
    pub background_sampling: BackgroundSampling,
    /// Which data slots the view draws its layers from
    /// (`TileWorld::bind`): [`crate::world::SHARED_BINDING`], every layer
    /// from its own records, or a binding of the world's that draws some
    /// from slots (e.g. a fog of war of its own). Views of one binding
    /// share its slots; a binding the world does not have is the shared one.
    pub binding: u32,
    /// Drawn after the picture is shown on the target, straight onto it in
    /// its pixels and direct colour (no world: `cells` false), however the
    /// picture was composed and scaled: a small panel at the window's
    /// resolution over a picture composed smaller (a pixel scale's
    /// confirmation), at the cost of its own pixels only. Such views come
    /// last; `origin` and `viewport` are in target pixels.
    pub on_target: bool,
}

/// How a scaled view samples sprites with the explicit UI background role
/// (`tile_render_shared::SAMPLING_UI_BACKGROUND`). Other sprites and views
/// at 1:1 keep their ordinary nearest samples.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BackgroundSampling {
    /// One deterministic source texel, weighted by the scaled footprint.
    #[default]
    Dither,
    /// Box filtering when shrinking; bilinear colour when magnifying.
    /// Transparent neighbours are omitted; nominal alpha stays unchanged.
    Interpolate,
    /// The nominal nearest texel.
    Nearest,
}

impl BackgroundSampling {
    /// `FrameParams::background_sampling`.
    pub fn code(self) -> u32 {
        match self {
            Self::Dither => tile_render_shared::BACKGROUND_DITHER,
            Self::Interpolate => tile_render_shared::BACKGROUND_INTERPOLATE,
            Self::Nearest => tile_render_shared::BACKGROUND_NEAREST,
        }
    }
}

impl Camera {
    pub fn new(viewport: (u32, u32)) -> Self {
        Self {
            x: 0,
            y: 0,
            zoom: (1, 1),
            viewport,
            overhang: (0, 0, 0, 0),
            cull: None,
            origin: (0, 0),
            target_clip: None,
            effect: None,
            cells: true,
            screen_shift: (0, 0),
            zoomed_art: false,
            stacked: false,
            hairline: false,
            downscale: 1,
            hidden_categories: 0,
            background_sampling: BackgroundSampling::Dither,
            binding: crate::world::SHARED_BINDING,
            on_target: false,
        }
    }

    /// User filters together with the annotation layout for this world scale.
    pub fn effective_hidden_categories(&self) -> u32 {
        tile_render_shared::categories_at_downscale(self.hidden_categories, self.downscale)
    }

    /// Whether the view is scaled (zoom other than 1).
    pub fn is_scaled(&self) -> bool {
        self.zoom.0.max(1) != self.zoom.1.max(1)
    }

    /// This view drawn into a canvas `k` times as large each way (one shown
    /// 1:1 rather than in `k`×`k` boxes, `Present::at_target`): its
    /// rectangle, target clip and zoom `k` times larger, its world and its
    /// screen sprites' own coordinates as they were. Art drawn at zoom 1
    /// shows exactly as the boxes showed it where the view's zoom is whole;
    /// what a view draws at its zoom (zoomed art: text from outlines) is
    /// drawn at the target's resolution. A view stacked on another keeps its
    /// origin (in that view's zoom-1 picture). An unscaled view without a
    /// world of its own (an interface) is drawn at its new zoom straight
    /// into the canvas (`hairline`, which only matters below zoom 1), as it
    /// was drawn straight into the smaller one: what is below shows where
    /// it draws nothing.
    pub fn boxed(&self, k: u32) -> Camera {
        let k = k.max(1);
        let ki = k as i32;
        let clip = self.target_clip.map(|c| c.map(|v| v.saturating_mul(ki)));
        if k == 1 {
            return *self;
        }
        if self.stacked {
            return Camera { target_clip: clip, ..*self };
        }
        let (n, d) = (self.zoom.0.max(1) as u64 * k as u64, self.zoom.1.max(1) as u64);
        let g = gcd(n, d);
        let origin = (self.origin.0.saturating_mul(ki), self.origin.1.saturating_mul(ki));
        let (sx, sy) = self.screen_shift;
        Camera {
            zoom: ((n / g) as u32, (d / g) as u32),
            origin,
            viewport: (self.viewport.0.saturating_mul(k), self.viewport.1.saturating_mul(k)),
            screen_shift: (sx + origin.0 - self.origin.0, sy + origin.1 - self.origin.1),
            target_clip: clip,
            effect: self.effect.map(|e| e.scaled(k as f32)),
            hairline: self.hairline || (!self.cells && !self.is_scaled()),
            ..*self
        }
    }

    /// Viewport size in world pixels (rounded up), however large: what the
    /// view shows of its world.
    pub fn world_size(&self) -> (u32, u32) {
        let (n, d) = (self.zoom.0.max(1) as u64, self.zoom.1.max(1) as u64);
        let side = |v: u32| (v as u64 * d).div_ceil(n).min(i32::MAX as u64 / 4) as u32;
        (side(self.viewport.0), side(self.viewport.1))
    }

    /// The picture drawn at zoom 1 and then scaled to the viewport, by
    /// nearest pixel: [`Camera::world_size`], at most [`MAX_SOURCE`] each
    /// way. A view whose world does not fit is drawn at its zoom straight
    /// into the target instead (`placements`).
    pub fn source_size(&self) -> (u32, u32) {
        let (w, h) = self.world_size();
        (w.min(MAX_SOURCE), h.min(MAX_SOURCE))
    }

    /// Whether the view is drawn at its zoom straight into the target from
    /// its start rather than from a picture at zoom 1 (which shows the same,
    /// `hairline` aside): a world shown at half size or smaller, whose
    /// picture would hold at least 4 times the pixels the target shows (a
    /// fitted preview: some 80 times as many), or one too large for a
    /// picture ([`MAX_SOURCE`]).
    pub fn drawn_at_zoom(&self) -> bool {
        let (w, h) = self.world_size();
        self.cells && self.is_scaled() && (2 * self.zoom.0.max(1) as u64 <= self.zoom.1.max(1) as u64 || w > MAX_SOURCE || h > MAX_SOURCE)
    }

    /// The zoom its picture is copied into its rectangle at: its own, or
    /// for one [`Camera::drawn_at_zoom`] (a picture of one background
    /// pixel) one that shows that pixel everywhere.
    pub(crate) fn picture_zoom(&self) -> (u32, u32) {
        if self.drawn_at_zoom() { (u32::MAX, 1) } else { (self.zoom.0.max(1), self.zoom.1.max(1)) }
    }

    /// The target pixels of the view inside a target of `size`.
    pub fn target_rect(&self, (w, h): (u32, u32)) -> impl Iterator<Item = (u32, u32)> + use<> {
        let (ox, oy) = self.origin;
        let [l, t, r, b] = self.target_clip.unwrap_or([0, 0, w as i32, h as i32]);
        let x = ox.max(l).max(0) as u32..(ox + self.viewport.0 as i32).min(r).clamp(0, w as i32) as u32;
        let y = oy.max(t).max(0) as u32..(oy + self.viewport.1 as i32).min(b).clamp(0, h as i32) as u32;
        y.flat_map(move |py| x.clone().map(move |px| (px, py)))
    }

    /// The source picture's texel (see [`Camera::source_size`]) shown at
    /// target pixel `p` of the view.
    pub fn source_texel(&self, (px, py): (u32, u32)) -> (u32, u32) {
        if self.drawn_at_zoom() {
            return (0, 0);
        }
        let (n, d) = (self.zoom.0.max(1), self.zoom.1.max(1));
        let (sw, sh) = self.source_size();
        let rel = |p: u32, o: i32| (p as i32 - o).max(0) as u32;
        let back = |p: u32, o: i32, s: u32| (rel(p, o) as u64 * d as u64 / n as u64).min(s as u64 - 1) as u32;
        (back(px, self.origin.0, sw), back(py, self.origin.1, sh))
    }

    /// `zoom_art`: see `FrameParams::zoom_art`; `scaled_at`: drawn at its zoom
    /// straight into the target, its viewport starting at that target pixel
    /// (`FrameParams::scaled`).
    /// `reach`: how far per-cell offsets move sprites beyond their cells
    /// (`TileWorld::offset_reach`, world pixels), on top of the overhang.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn frame_params(&self, grid: &Grid, color: ColorModel, frame: u32, pass: &Pass, view: u32, zoom_art: u32, scaled_at: Option<(i32, i32)>, reach: (u32, u32, u32, u32)) -> FrameParams {
        let (ew, eh) = self.world_size();
        // The grid at the view's scale.
        let n = self.downscale.max(1);
        let r = |v: u32| v.div_ceil(n);
        let (ol, ot, or, ob) = self.overhang;
        let (ol, ot, or, ob) = (ol + r(reach.0), ot + r(reach.1), or + r(reach.2), ob + r(reach.3));
        let grid_params = tile_render_shared::grid_at(&grid.params(), n);
        let (sx, sy) = ((grid_params.step_x as i32).max(1), (grid_params.step_y as i32).max(1));
        // A cell's origin is its top-left; its footprint is one step (two
        // horizontally on a staggered grid) wide.
        let col_step = match grid.layout {
            Layout::Orthogonal => sx,
            Layout::Staggered => 2 * sx,
        };
        let left = self.x - or as i32 - col_step;
        let right = self.x + ew as i32 + ol as i32;
        let top = self.y - ob as i32 - sy;
        let bottom = self.y + eh as i32 + ot as i32;

        // Nonwrapping cells outside the grid are rejected by wrap_cell in
        // the shader. Do not submit those degenerate instances, especially
        // for a tiny preview with a tall letterbox and conservative overhang.
        // Keep every wrapped copy, and retain the original sprite overhang
        // and offset reach before intersecting with the actual grid.
        let bounded = |first: i32, last: i32, size: u32, wraps: bool| {
            if size == 0 { (0, -1) }
            else if wraps { (first, last) }
            else { (first.max(0), last.min(size.min(i32::MAX as u32) as i32 - 1)) }
        };
        let (first_col, last_col) = bounded(left.div_euclid(col_step), right.div_euclid(col_step), grid.cols, grid.wrap_x);
        let (first_row, last_row) = bounded(top.div_euclid(sy), bottom.div_euclid(sy), grid.rows, grid.wrap_y);
        FrameParams {
            viewport_w: self.viewport.0,
            viewport_h: self.viewport.1,
            camera_x: self.x,
            camera_y: self.y,
            zoom_num: self.zoom.0.max(1),
            zoom_den: self.zoom.1.max(1),
            frame,
            layer_first: pass.first,
            layer_count: pass.count,
            cull: self.cull.is_some() as u32,
            color_bits: color.bits(),
            view,
            cull_x0: self.cull.map_or(0, |c| c.0),
            cull_y0: self.cull.map_or(0, |c| c.1),
            cull_x1: self.cull.map_or(0, |c| c.2),
            cull_y1: self.cull.map_or(0, |c| c.3),
            first_col,
            first_row,
            cols: (last_col - first_col + 1).max(0) as u32,
            rows: (last_row - first_row + 1).max(0) as u32,
            grid: grid_params,
            origin_x: self.origin.0 - self.screen_shift.0,
            origin_y: self.origin.1 - self.screen_shift.1,
            world: self.cells as u32,
            zoom_art,
            scaled: scaled_at.is_some() as u32,
            hairline: (scaled_at.is_some() && self.hairline) as u32,
            target_x: scaled_at.map_or(0, |t| t.0),
            target_y: scaled_at.map_or(0, |t| t.1),
            downscale: n,
            descending: pass.descending as u32,
            named_palettes: 0,
            corner_cuts: 0,
            hidden_categories: self.effective_hidden_categories(),
            background_sampling: self.background_sampling.code(),
            // (Set where the world's tables are known.)
            layer_table: 0,
            quantized_float: 0,
            host_tick_ms: 0,
            _host_reserved0: 0,
            _host_reserved1: 0,
            _host_reserved2: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Layout;

    #[test]
    fn tiny_nonwrapping_preview_bounds_candidates_without_shrinking_sprite_overhang() {
        // The Rise of Rome preview after 1012x632 -> 1280x678 resize.
        // Its 1024-pixel menu leaves a 252x678 world at downscale 32.
        let grid = Grid { cols: 70, rows: 142, layout: Layout::Staggered, wrap_x: false, wrap_y: false, step: (64, 32) };
        let camera = Camera { x: 6, y: -300, downscale: 32, overhang: (128, 128, 128, 128), ..Camera::new((276, 743)) };
        let pass = Pass { first: 0, count: 76, descending: false };
        let p = camera.frame_params(&grid, ColorModel::Direct, 0, &pass, 0, 0, None, (0, 0, 0, 0));
        assert_eq!((p.first_col, p.first_row, p.cols, p.rows), (0, 0, 70, 142));
        assert_eq!(p.cols * p.rows * pass.count, 755_440);
        assert_eq!(camera.overhang, (128, 128, 128, 128));
        let wrapped = Grid { wrap_x: true, wrap_y: true, ..grid };
        let all = camera.frame_params(&wrapped, ColorModel::Direct, 0, &pass, 0, 0, None, (0, 0, 0, 0));
        assert_eq!((all.first_col, all.first_row, all.cols, all.rows), (-32, -429, 135, 1001));
        assert_eq!(all.cols * all.rows * pass.count, 10_270_260);
        for grid in [Grid { wrap_x: true, ..grid }, Grid { wrap_y: true, ..grid }] {
            let p = camera.frame_params(&grid, ColorModel::Direct, 0, &pass, 0, 0, None, (0, 0, 0, 0));
            assert_eq!((p.first_col, p.cols), if grid.wrap_x { (all.first_col, all.cols) } else { (0, 70) });
            assert_eq!((p.first_row, p.rows), if grid.wrap_y { (all.first_row, all.rows) } else { (0, 142) });
        }
    }

    #[test]
    fn nonwrapping_empty_intersections_have_zero_cells_and_keep_interface_parameters() {
        let grid = Grid { cols: 2, rows: 3, layout: Layout::Orthogonal, wrap_x: false, wrap_y: false, step: (16, 16) };
        for (x, y) in [(-1000, 0), (1000, 0), (0, -1000), (0, 1000)] {
            let camera = Camera { x, y, screen_shift: (5, 7), ..Camera::new((40, 30)) };
            let p = camera.frame_params(&grid, ColorModel::Direct, 0, &Pass::default(), 0, 0, None, (0, 0, 0, 0));
            assert_eq!(p.cols * p.rows, 0);
            assert_eq!((p.origin_x, p.origin_y), (-5, -7));
            assert_eq!((p.camera_x, p.camera_y, p.viewport_w, p.viewport_h), (x, y, 40, 30));
        }
        for wrap_x in [false, true] {
            let grid = Grid { cols: 0, wrap_x, ..grid };
            let p = Camera::new((40, 30)).frame_params(&grid, ColorModel::Direct, 0, &Pass::default(), 0, 0, None, (0, 0, 0, 0));
            assert_eq!(p.cols, 0);
        }
    }

    #[test]
    fn target_clip_scissors_do_not_move_camera_or_stacked_sampling_origins() {
        let clip = [64, 19, 131, 87];
        let camera = Camera { origin: (13, 9), target_clip: Some(clip), cells: false, ..Camera::new((180, 120)) };
        let p = placements(&[camera], |_, _, _| None);
        assert_eq!(p[0].drawn.origin, (13, 9));
        assert_eq!(p[0].rect, [13, 9, 180, 120]);
        assert_eq!(p[0].scissor, [64, 19, 67, 68]);
        assert_eq!(camera.target_rect((240, 160)).count(), 67 * 68);
        let head = Camera { zoom: (3, 2), ..camera };
        let child = Camera { origin: (7, 11), stacked: true, target_clip: Some([80, 0, 200, 77]), cells: false, ..Camera::new((150, 110)) };
        let p = placements(&[head, child], |_, _, _| None);
        assert_eq!(p[0].scaled.origin, (13, 9));
        assert_eq!(p[0].scaled_scissor, clip);
        assert_eq!(p[1].scaled.origin, (13, 9));
        assert_eq!(p[1].scaled_scissor, [80, 26, 131, 77]);
        assert_eq!((p[0].scaled_from, p[1].scaled_from), (Some(0), Some(0)));
    }

    #[test]
    fn window_covers_the_viewport_with_overhang() {
        let grid = Grid { cols: 50, rows: 100, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
        let mut cam = Camera::new((1024, 768));
        cam.x = 300;
        cam.y = 200;
        cam.overhang = (0, 24, 0, 0);
        let p = cam.frame_params(&grid, ColorModel::Direct, 0, &Pass { first: 0, count: 3, descending: false }, 0, 0, None, (0, 0, 0, 0));
        // Every cell whose 128×64 footprint (+24 px upwards) touches the
        // viewport must be inside the window.
        for row in -10..120 {
            for col in -10..80 {
                let (x, y) = grid.origin(col, row);
                let touches = x + 128 > cam.x && x < cam.x + 1024 && y + 64 > cam.y && y - 24 < cam.y + 768;
                if touches {
                    assert!(col >= p.first_col && col < p.first_col + p.cols as i32, "col {col}");
                    assert!(row >= p.first_row && row < p.first_row + p.rows as i32, "row {row}");
                }
            }
        }
    }
}

/// How a view is drawn (see `TileRenderer::render_views`): views are drawn
/// at zoom 1, those of a scaled group (a scaled view and the views stacked
/// on it) into a picture of their own that is then copied scaled. From the
/// first zoomed sprite of a group's view showing zoomed art (or a hairline
/// view) on, the rest of the group is drawn at its zoom straight into the
/// target instead, which shows the same, zoomed art (text at the zoom's
/// resolution) included in its place in the drawing order (under the fog
/// of war, say).
///
/// A view's drawing is a sequence of steps: step `2·i` the cells of pass
/// `i`, step `2·i + 1` the free sprites after it (`i` up to the number of
/// passes: those after all of them).
#[derive(Copy, Clone, Debug)]
pub(crate) struct Placement {
    /// The camera it is drawn with, at zoom 1.
    pub drawn: Camera,
    /// Where (x, y, w, h): in its group's picture (`source`), else in the
    /// target.
    pub rect: [i32; 4],
    /// Scissor in the same coordinates as rect, separate from its viewport.
    pub scissor: [i32; 4],
    /// The group's picture in the source texture, if the group is scaled.
    pub source: Option<[u32; 4]>,
    /// The group's camera and picture, when the group's picture is done
    /// after this view's steps at zoom 1: the picture is copied scaled then.
    pub blit: Option<(Camera, [u32; 4])>,
    /// Whether it shows zoomed art (rather than their 1:1 twins).
    pub art: bool,
    /// The first step drawn at its zoom straight into the target, if any.
    pub scaled_from: Option<usize>,
    /// The camera it is drawn with then (at its group's zoom, in its
    /// group's rectangle of the target), and the target pixels it may
    /// touch (x0, y0, x1, y1; exclusive, maybe empty).
    pub scaled: Camera,
    pub scaled_scissor: [i32; 4],
}

impl Placement {
    /// Its steps drawn at zoom 1 (into its group's picture or the target).
    pub fn steps_at_one(&self) -> std::ops::Range<usize> {
        0..self.scaled_from.unwrap_or(usize::MAX)
    }

    /// Its steps drawn at its zoom.
    pub fn steps_scaled(&self) -> std::ops::Range<usize> {
        self.scaled_from.map_or(0..0, |s| s..usize::MAX)
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

/// `v·n/d` rounded down, and up.
fn floor_mul(v: i32, n: u32, d: u32) -> i32 {
    (v as i64 * n as i64).div_euclid(d as i64) as i32
}
fn ceil_mul(v: i32, n: u32, d: u32) -> i32 {
    -floor_mul(-v, n, d)
}

/// Where in the source texture each scaled group's picture goes (rows at
/// most `MAX_SOURCE` wide), and how each view is drawn. `first_zoomed`: the
/// first pass that zoomed sprites made for a zoom follow, if there are any.
pub(crate) fn placements(cameras: &[Camera], first_zoomed: impl Fn((u32, u32), u32, &Camera) -> Option<usize>) -> Vec<Placement> {
    let (mut x, mut y, mut row) = (0, 0, 0);
    let mut out: Vec<Placement> = Vec::with_capacity(cameras.len());
    let mut head: Option<(Camera, Option<[u32; 4]>)> = None;
    // Of the current scaled group: whether it is drawn at its zoom yet.
    let mut scaling = false;
    for (i, c) in cameras.iter().enumerate() {
        let last = !cameras.get(i + 1).is_some_and(|n| n.stacked);
        let (drawn, rect, source, group) = match head {
            Some((g, source)) if c.stacked => match source {
                // Into the group's picture, at its own origin there.
                Some(r) => {
                    let drawn = Camera { zoom: (1, 1), ..*c };
                    (drawn, [r[0] as i32 + c.origin.0, r[1] as i32 + c.origin.1, c.viewport.0 as i32, c.viewport.1 as i32], Some(r), g)
                }
                // An unscaled group: at its origin in the target.
                None => {
                    let origin = (g.origin.0 + c.origin.0, g.origin.1 + c.origin.1);
                    let drawn = Camera { zoom: (1, 1), origin, screen_shift: (c.screen_shift.0 + g.origin.0, c.screen_shift.1 + g.origin.1), ..*c };
                    (drawn, [origin.0, origin.1, c.viewport.0 as i32, c.viewport.1 as i32], None, g)
                }
            },
            _ if c.is_scaled() => {
                // A view without a world drawn at its zoom from its start
                // (below) never uses its picture: none is set aside for it
                // (nor for the views stacked on it, drawn so too).
                let unused = !c.cells && (c.hairline || c.target_clip.is_some());
                // One drawn at its zoom that shows a world keeps a pixel of
                // background, copied over its rectangle (`picture_zoom`).
                let (w, h) = if unused { (0, 0) } else if c.drawn_at_zoom() { (1, 1) } else { c.source_size() };
                if x + w > MAX_SOURCE {
                    (x, y, row) = (0, y + row, 0);
                }
                let r = [x, y, w, h];
                x += w;
                row = row.max(h);
                head = Some((*c, Some(r)));
                scaling = false;
                let drawn = Camera { zoom: (1, 1), viewport: (w, h), ..*c };
                (drawn, [r[0] as i32, r[1] as i32, w as i32, h as i32], Some(r), *c)
            }
            _ => {
                head = Some((*c, None));
                (*c, [c.origin.0, c.origin.1, c.viewport.0 as i32, c.viewport.1 as i32], None, *c)
            }
        };
        let art = source.is_some() && c.zoomed_art;
        // At its group's zoom, in the group's frame: a stacked view's screen
        // sprites from its origin there, its world ones from its camera
        // there, and only over its rectangle there.
        let (scaled, local) = if c.stacked {
            let camera = Camera {
                x: c.x - c.origin.0,
                y: c.y - c.origin.1,
                zoom: group.zoom,
                origin: group.origin,
                viewport: group.viewport,
                screen_shift: (group.origin.0 + c.screen_shift.0, group.origin.1 + c.screen_shift.1),
                ..*c
            };
            (camera, [c.origin.0, c.origin.1, c.origin.0 + c.viewport.0 as i32, c.origin.1 + c.viewport.1 as i32])
        } else {
            (*c, [0, 0, i32::MAX, i32::MAX])
        };
        // (The group's picture's zoom-1 pixels, mapped to the target.)
        let (n, d) = (group.zoom.0.max(1), group.zoom.1.max(1));
        let (sw, sh) = group.world_size();
        let local =[local[0].max(0), local[1].max(0), local[2].min(sw as i32), local[3].min(sh as i32)];
        let (ox, oy) = group.origin;
        let mut scaled_scissor = [
            ox + ceil_mul(local[0], n, d).max(0),
            oy + ceil_mul(local[1], n, d).max(0),
            ox + ceil_mul(local[2], n, d).min(group.viewport.0 as i32),
            oy + ceil_mul(local[3], n, d).min(group.viewport.1 as i32),
        ];
        // A target clip is a scissor, never a new camera origin. Preserve
        // sampling and text positions while confining this view to its pane.
        let intersect = |a: [i32; 4], b: [i32; 4]| [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
        let target_clip = [group.target_clip, c.target_clip].into_iter().flatten().reduce(intersect);
        if let Some(clip) = target_clip { scaled_scissor = intersect(scaled_scissor, clip); }
        let scissor = if source.is_none() {
            let r = [rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]];
            let r = target_clip.map_or(r, |clip| intersect(r, clip));
            [r[0], r[1], (r[2] - r[0]).max(0), (r[3] - r[1]).max(0)]
        } else { rect };
        // Drawn at the zoom from the first zoomed sprite of a view showing
        // them, or from the start of a hairline view, on.
        let scaled_from = match source {
            None => None,
            Some(_) if scaling => Some(0),
            Some(_) => {
                let zoomed = first_zoomed((n, d), i as u32, c).filter(|_| art).map(|p| 2 * p + 1);
                let from = if c.hairline || target_clip.is_some() || c.drawn_at_zoom() { Some(0) } else { zoomed };
                scaling = from.is_some();
                from
            }
        };
        // The picture is copied once its last zoom-1 step is drawn; not at
        // all for a view drawn at its zoom from its start that shows no
        // world: it has nothing of its own beneath what it draws (an
        // interface over other views, scaled).
        let starts_scaling = scaled_from.is_some() && !out.last().is_some_and(|p: &Placement| p.source == source && p.scaled_from.is_some());
        let empty = scaled_from == Some(0) && !c.stacked && !c.cells;
        let blit = source.filter(|_| !empty && (starts_scaling || (last && scaled_from.is_none()))).map(|r| (group, r));
        out.push(Placement { drawn, rect, scissor, source, blit, art, scaled_from, scaled, scaled_scissor });
    }
    out
}

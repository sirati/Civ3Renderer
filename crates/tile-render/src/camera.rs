//! What part of the world is on screen.

use tile_render_shared::FrameParams;

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
    /// sprite on), which shows the same but for those.
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
            cells: true,
            screen_shift: (0, 0),
            zoomed_art: false,
            stacked: false,
            hairline: false,
            downscale: 1,
        }
    }

    /// Whether the view is scaled (zoom other than 1).
    pub fn is_scaled(&self) -> bool {
        self.zoom.0.max(1) != self.zoom.1.max(1)
    }

    /// Viewport size in world pixels (rounded up): the picture drawn at
    /// zoom 1 and then scaled to the viewport, by nearest pixel. At most
    /// [`MAX_SOURCE`] each way (a view zoomed out further shows less).
    pub fn source_size(&self) -> (u32, u32) {
        let (n, d) = (self.zoom.0.max(1), self.zoom.1.max(1));
        ((self.viewport.0 * d).div_ceil(n).min(MAX_SOURCE), (self.viewport.1 * d).div_ceil(n).min(MAX_SOURCE))
    }

    /// The target pixels of the view inside a target of `size`.
    pub fn target_rect(&self, (w, h): (u32, u32)) -> impl Iterator<Item = (u32, u32)> + use<> {
        let (ox, oy) = self.origin;
        let x = ox.max(0) as u32..(ox + self.viewport.0 as i32).clamp(0, w as i32) as u32;
        let y = oy.max(0) as u32..(oy + self.viewport.1 as i32).clamp(0, h as i32) as u32;
        y.flat_map(move |py| x.clone().map(move |px| (px, py)))
    }

    /// The source picture's texel (see [`Camera::source_size`]) shown at
    /// target pixel `p` of the view.
    pub fn source_texel(&self, (px, py): (u32, u32)) -> (u32, u32) {
        let (n, d) = (self.zoom.0.max(1), self.zoom.1.max(1));
        let (sw, sh) = self.source_size();
        let rel = |p: u32, o: i32| (p as i32 - o).max(0) as u32;
        ((rel(px, self.origin.0) * d / n).min(sw - 1), (rel(py, self.origin.1) * d / n).min(sh - 1))
    }

    /// `zoom_art`: see `FrameParams::zoom_art`; `scaled_at`: drawn at its zoom
    /// straight into the target, its viewport starting at that target pixel
    /// (`FrameParams::scaled`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn frame_params(&self, grid: &Grid, color: ColorModel, frame: u32, pass: &Pass, view: u32, zoom_art: u32, scaled_at: Option<(i32, i32)>) -> FrameParams {
        let (ew, eh) = self.source_size();
        let (ol, ot, or, ob) = self.overhang;
        // The grid at the view's scale.
        let n = self.downscale.max(1);
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

        let first_col = left.div_euclid(col_step);
        let last_col = right.div_euclid(col_step);
        let first_row = top.div_euclid(sy);
        let last_row = bottom.div_euclid(sy);
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
            cols: (last_col - first_col + 1) as u32,
            rows: (last_row - first_row + 1) as u32,
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
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Layout;

    #[test]
    fn window_covers_the_viewport_with_overhang() {
        let grid = Grid { cols: 50, rows: 100, layout: Layout::Staggered, wrap_x: true, wrap_y: false, step: (64, 32) };
        let mut cam = Camera::new((1024, 768));
        cam.x = 300;
        cam.y = 200;
        cam.overhang = (0, 24, 0, 0);
        let p = cam.frame_params(&grid, ColorModel::Direct, 0, &Pass { first: 0, count: 3 }, 0, 0, None);
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
pub(crate) fn placements(cameras: &[Camera], first_zoomed: impl Fn((u32, u32)) -> Option<usize>) -> Vec<Placement> {
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
                let (w, h) = c.source_size();
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
        let (sw, sh) = group.source_size();
        let local = [local[0].max(0), local[1].max(0), local[2].min(sw as i32), local[3].min(sh as i32)];
        let (ox, oy) = group.origin;
        let scaled_scissor = [
            ox + ceil_mul(local[0], n, d).max(0),
            oy + ceil_mul(local[1], n, d).max(0),
            ox + ceil_mul(local[2], n, d).min(group.viewport.0 as i32),
            oy + ceil_mul(local[3], n, d).min(group.viewport.1 as i32),
        ];
        // Drawn at the zoom from the first zoomed sprite of a view showing
        // them, or from the start of a hairline view, on.
        let scaled_from = match source {
            None => None,
            Some(_) if scaling => Some(0),
            Some(_) => {
                let zoomed = first_zoomed((n, d)).filter(|_| art).map(|p| 2 * p + 1);
                let from = if c.hairline { Some(0) } else { zoomed };
                scaling = from.is_some();
                from
            }
        };
        // The picture is copied once its last zoom-1 step is drawn.
        let starts_scaling = scaled_from.is_some() && !out.last().is_some_and(|p: &Placement| p.source == source && p.scaled_from.is_some());
        let blit = source.filter(|_| starts_scaling || (last && scaled_from.is_none())).map(|r| (group, r));
        out.push(Placement { drawn, rect, source, blit, art, scaled_from, scaled, scaled_scissor });
    }
    out
}

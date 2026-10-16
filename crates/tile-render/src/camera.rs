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
}

impl Camera {
    pub fn new(viewport: (u32, u32)) -> Self {
        Self { x: 0, y: 0, zoom: (1, 1), viewport, overhang: (0, 0, 0, 0), cull: None, origin: (0, 0), cells: true }
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

    pub(crate) fn frame_params(&self, grid: &Grid, color: ColorModel, frame: u32, pass: &Pass, view: u32) -> FrameParams {
        let (ew, eh) = self.source_size();
        let (ol, ot, or, ob) = self.overhang;
        let (sx, sy) = (grid.step.0 as i32, grid.step.1 as i32);
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
            _pad0: 0,
            _pad1: 0,
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
            grid: grid.params(),
            origin_x: self.origin.0,
            origin_y: self.origin.1,
            world: self.cells as u32,
            _pad3: 0,
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
        let p = cam.frame_params(&grid, ColorModel::Direct, 0, &Pass { first: 0, count: 3 }, 0);
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

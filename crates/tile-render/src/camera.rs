//! What part of the world is on screen.

use tile_render_shared::FrameParams;

use crate::world::{Grid, Layout};

/// A view onto the world.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Camera {
    /// World pixel at the top-left corner of the viewport.
    pub x: i32,
    pub y: i32,
    /// Screen pixels per world pixel, as a fraction (1/1, 1/2, 2/1, …).
    pub zoom: (u32, u32),
    /// Viewport size in screen pixels.
    pub viewport: (u32, u32),
    /// How far sprites may extend beyond their cell, in world pixels:
    /// (left, top, right, bottom). Cells whose sprites could reach into the
    /// viewport by that much are drawn too.
    pub overhang: (u32, u32, u32, u32),
}

impl Camera {
    pub fn new(viewport: (u32, u32)) -> Self {
        Self { x: 0, y: 0, zoom: (1, 1), viewport, overhang: (0, 0, 0, 0) }
    }

    /// Viewport size in world pixels (rounded up).
    pub fn world_extent(&self) -> (u32, u32) {
        let (n, d) = self.zoom;
        ((self.viewport.0 * d).div_ceil(n), (self.viewport.1 * d).div_ceil(n))
    }

    pub(crate) fn frame_params(&self, grid: &Grid, frame: u32, layers: (u32, u32)) -> FrameParams {
        let (ew, eh) = self.world_extent();
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
            zoom_num: self.zoom.0,
            zoom_den: self.zoom.1,
            frame,
            layer_first: layers.0,
            layer_count: layers.1,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
            first_col,
            first_row,
            cols: (last_col - first_col + 1) as u32,
            rows: (last_row - first_row + 1) as u32,
            grid: grid.params(),
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
        let p = cam.frame_params(&grid, 0, (0, 3));
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

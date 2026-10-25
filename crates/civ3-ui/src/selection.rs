//! File geometry for Palace piece selection. The game's +0x74 blits name
//! eligible pieces and their order; no pixel map is rasterised or copied.
//! Both hit queries and pointer targets use the viewer's sprite sources.

use crate::ui::Source;

/// Immutable file pixels, decoded by the game or an asset loader. Keeping
/// the source stride avoids repacking a padded game allocation per sprite.
#[derive(Debug)]
pub struct FilePixels {
    pub size: (u32, u32),
    pub row: u32,
    pub indices: Vec<u8>,
    pub palette: [[u8; 3]; 256],
}

impl FilePixels {
    pub fn index(&self, x: i32, y: i32) -> Option<u8> {
        if x < 0 || y < 0 || x as u32 >= self.size.0 || y as u32 >= self.size.1 { return None; }
        self.indices.get(y as usize * self.row as usize + x as usize).copied()
    }

    pub fn pixel(&self, x: i32, y: i32, keyed: bool) -> Option<[u8; 3]> {
        let index = self.index(x, y)?;
        (!keyed || index < 254).then_some(self.palette[index as usize])
    }
}

#[derive(Clone, Debug)]
struct Piece {
    index: u8,
    path: String,
    source: [i32; 4],
    at: (i32, i32),
    clip: [i32; 4],
}

#[derive(Clone, Debug)]
pub struct Selection {
    size: (i32, i32),
    pieces: Vec<Piece>,
}

impl Selection {
    /// A freshly cleared piece selection, after the game's full 0xFF fill.
    pub fn new(size: (i32, i32)) -> Self {
        Self { size, pieces: Vec::new() }
    }

    /// Keep the source *at this call*, so a later sprite reset or style
    /// change cannot change a selection that has not been rebuilt yet.
    pub fn piece(&mut self, source: &Source, index: u8, at: (i32, i32), clip: Option<[i32; 4]>) -> Result<(), &'static str> {
        let Source::File { path, rect, mask: None, .. } = source else {
            return Err("Palace selection needs an unmasked file sprite");
        };
        self.pieces.push(Piece { index, path: path.clone(), source: *rect, at, clip: clip.unwrap_or([0, 0, self.size.0, self.size.1]) });
        Ok(())
    }

    /// The same keyed file-pixel query UiModel::shown_at uses. For Palace
    /// sprites indices 0xFE and 0xFF are outside the piece, regardless of
    /// their RGB. None means the source file is unavailable, never a hit.
    pub fn hit(&self, x: i32, y: i32, pixel: &impl Fn(&str, i32, i32, bool) -> Option<Option<[u8; 3]>>) -> Option<u8> {
        if x < 0 || y < 0 || x >= self.size.0 || y >= self.size.1 { return Some(0xFF); }
        for piece in self.pieces.iter().rev() {
            let [l, t, r, b] = piece.clip;
            let (sx, sy) = (x - piece.at.0, y - piece.at.1);
            if x < l || x >= r || y < t || y >= b || sx < 0 || sy < 0 || sx >= piece.source[2] || sy >= piece.source[3] { continue; }
            if pixel(&piece.path, piece.source[0] + sx, piece.source[1] + sy, true)?.is_some() { return Some(piece.index); }
        }
        Some(0xFF)
    }

    /// One verified point per selectable piece. These become pointer
    /// targets even when the game's pixel allocation contains placeholders.
    /// Testing each candidate through hit also handles occluding pieces.
    pub fn anchors(&self, pixel: &impl Fn(&str, i32, i32, bool) -> Option<Option<[u8; 3]>>) -> Option<Vec<(u8, [i32; 4])>> {
        let mut out = Vec::new();
        for piece in &self.pieces {
            if out.iter().any(|(i, _)| *i == piece.index) { continue; }
            let (cx, cy) = (piece.source[2] / 2, piece.source[3] / 2);
            let mut best = None;
            for y in 0..piece.source[3] {
                for x in 0..piece.source[2] {
                    let distance = i64::from(x - cx).pow(2) + i64::from(y - cy).pow(2);
                    if best.is_some_and(|(d, _, _)| distance >= d) { continue; }
                    let (px, py) = (piece.at.0 + x, piece.at.1 + y);
                    if self.hit(px, py, pixel)? == piece.index { best = Some((distance, px, py)); }
                }
            }
            if let Some((_, x, y)) = best { out.push((piece.index, [x, y, 1, 1])); }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(name: &str) -> Source {
        Source::File { path: name.into(), rect: [10, 20, 4, 3], mask: None, palette: None, opacity: 255 }
    }

    #[test]
    fn piece_holes_overlap_clip_and_missing_files_use_the_viewers_query() {
        let mut map = Selection::new((20, 20));
        map.piece(&source("lower"), 1, (2, 3), None).unwrap();
        map.piece(&source("upper"), 2, (3, 3), Some([0, 0, 5, 20])).unwrap();
        let pixel = |path: &str, x: i32, y: i32, keyed: bool| {
            assert!(keyed);
            assert!((10..14).contains(&x) && (20..23).contains(&y));
            Some((path == "lower" || x != 10).then_some([0; 3]))
        };
        assert_eq!(map.hit(2, 3, &pixel), Some(1));
        assert_eq!(map.hit(3, 3, &pixel), Some(1), "upper hole reveals lower piece");
        assert_eq!(map.hit(4, 3, &pixel), Some(2));
        assert_eq!(map.hit(5, 3, &pixel), Some(1), "upper clip is exclusive");
        assert_eq!(map.hit(6, 3, &pixel), Some(0xFF));
        assert_eq!(map.hit(-1, 3, &pixel), Some(0xFF));
        assert_eq!(map.hit(4, 3, &|_, _, _, _| None), None);
        for (index, [x, y, w, h]) in map.anchors(&pixel).unwrap() {
            assert_eq!((w, h), (1, 1));
            assert_eq!(map.hit(x, y, &pixel), Some(index));
        }
    }

    #[test]
    fn rebuilt_selection_discards_old_eligibility_and_snapshots_sources() {
        let mut map = Selection::new((20, 20));
        let mut sprite = source("first style");
        map.piece(&sprite, 3, (1, 2), None).unwrap();
        sprite = source("second style");
        assert_eq!(map.hit(1, 2, &|path, _, _, _| { assert_eq!(path, "first style"); Some(Some([1; 3])) }), Some(3));
        map = Selection::new((20, 20));
        assert_eq!(map.hit(1, 2, &|_, _, _, _| panic!("no eligible piece")), Some(0xFF));
        map.piece(&sprite, 4, (3, 4), None).unwrap();
        assert_eq!(map.hit(3, 4, &|path, _, _, _| { assert_eq!(path, "second style"); Some(Some([1; 3])) }), Some(4));
    }
}

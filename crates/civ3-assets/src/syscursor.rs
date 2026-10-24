//! The system's own mouse pointers, as the game shows them: Civilization III
//! has no wait cursor of its own (`Art/cursor.pcx` has none, nor do its
//! executables' resources); while busy it asks the system for `IDC_WAIT`
//! (`LoadCursorA(NULL, IDC_WAIT)`), which under Wine is the hourglass in
//! Wine's `user32.dll` (group cursor resource 32514).
//!
//! [`find`] reads a standard cursor (`IDC_*`) out of the system's
//! `user32.dll` (its `RT_GROUP_CURSOR` / `RT_CURSOR` resources): the
//! Windows folder's (`%SystemRoot%`) on Windows, else the Wine prefix's
//! (`$WINEPREFIX`, or the `drive_c` the game is installed in). Wine names
//! each group by its `IDC_*` id; Windows' own `user32.dll` (Windows 10 and
//! 11 at least) numbers them from 100 ([`windows_group`]), the classic
//! black and white hourglass of `IDC_WAIT` being group 102. (The pointer
//! Windows shows for `IDC_WAIT` is the user's scheme's, by default the
//! turning ring `aero_busy.ani`; the hourglass is what the game showed.)

use std::path::{Path, PathBuf};

const RT_CURSOR: u32 = 1;
const RT_GROUP_CURSOR: u32 = 12;

/// A decoded cursor image: `width`×`height` RGBA8 rows, top to bottom, and
/// its hot spot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub hotspot: (u16, u16),
}

/// The `user32.dll` files that may hold the system's cursors, most likely
/// first; `roots`: the game's folders (a Wine prefix's `drive_c` above
/// them).
pub fn user32_candidates(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut windows: Vec<PathBuf> = Vec::new();
    for var in ["SystemRoot", "windir"] {
        if let Some(dir) = std::env::var_os(var) {
            windows.push(PathBuf::from(dir));
        }
    }
    if let Some(prefix) = std::env::var_os("WINEPREFIX") {
        windows.push(Path::new(&prefix).join("drive_c").join("windows"));
    }
    for root in roots {
        if let Some(drive) = root.ancestors().find(|a| a.file_name().is_some_and(|n| n.eq_ignore_ascii_case("drive_c"))) {
            windows.push(drive.join("windows"));
        }
    }
    let mut out = Vec::new();
    for dir in windows {
        // (A 32-bit process reads system32 as syswow64 anyway.)
        for sub in ["syswow64", "SysWOW64", "system32", "System32"] {
            let p = dir.join(sub).join("user32.dll");
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// System cursor `id` (`IDC_*`, e.g. 32514 for `IDC_WAIT`) at `size`
/// pixels (the group's entry of that size, the most colours; else its
/// largest), from the first of `candidates` that has it.
pub fn find(candidates: &[PathBuf], id: u32, size: u32) -> Result<CursorImage, String> {
    let mut why = Vec::new();
    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else { continue };
        match cursor(&bytes, id, size) {
            Ok(c) => return Ok(c),
            Err(e) => why.push(format!("{}: {e}", path.display())),
        }
    }
    if why.is_empty() {
        Err(format!("system cursor {id}: no user32.dll found"))
    } else {
        Err(format!("system cursor {id}: {}", why.join("; ")))
    }
}

/// The group resource Windows' own `user32.dll` keeps standard cursor `id`
/// (`IDC_*`) in, where it is not named by its id as in Wine's. Only the
/// groups whose pictures were checked against Windows 11's `user32.dll`
/// (10.0.26200): 100 the arrow, 102 the hourglass.
pub fn windows_group(id: u32) -> Option<u32> {
    match id {
        32512 => Some(100),
        32514 => Some(102),
        _ => None,
    }
}

/// Cursor `id` at `size` of a PE file's resources: its group named `id`
/// (Wine), else the group Windows numbers it by ([`windows_group`]).
pub fn cursor(pe: &[u8], id: u32, size: u32) -> Result<CursorImage, String> {
    let res = Resources::of(pe)?;
    let group = res.find(RT_GROUP_CURSOR, id)
        .or_else(|| res.find(RT_GROUP_CURSOR, windows_group(id)?))
        .ok_or("no such group cursor")?;
    // NEWHEADER (reserved, type 2, count), then 14-byte entries: width,
    // height (doubled), planes, bit count, bytes, id.
    let count = u16_at(group, 4)? as usize;
    let mut best: Option<(u32, u16, u16)> = None;
    for i in 0..count {
        let e = 6 + 14 * i;
        let (w, bits, rid) = (u16_at(group, e)? as u32, u16_at(group, e + 6)?, u16_at(group, e + 12)?);
        let w = if w == 0 { 256 } else { w };
        let better = match best {
            None => true,
            Some((bw, bb, _)) => {
                let (fits, best_fits) = (w == size, bw == size);
                (fits && !best_fits) || (fits == best_fits && (bits > bb || (bits == bb && w > bw)))
            }
        };
        if better {
            best = Some((w, bits, rid));
        }
    }
    let (_, _, rid) = best.ok_or("empty group cursor")?;
    let data = res.find(RT_CURSOR, rid as u32).ok_or("missing cursor image")?;
    let hotspot = (u16_at(data, 0)?, u16_at(data, 2)?);
    let mut image = dib(data.get(4..).ok_or("short cursor")?)?;
    image.hotspot = hotspot;
    Ok(image)
}

/// A cursor's DIB (BITMAPINFOHEADER, its height the XOR and AND masks'):
/// 32 bits (alpha, or the AND mask where all alpha is 0) or 1 bit (the
/// AND mask's transparency; inverting pixels drawn black).
fn dib(d: &[u8]) -> Result<CursorImage, String> {
    let header = u32_at(d, 0)? as usize;
    let width = i32_at(d, 4)?;
    let height = i32_at(d, 8)? / 2;
    let bits = u16_at(d, 14)?;
    if width <= 0 || height <= 0 || width > 256 || height > 256 {
        return Err(format!("cursor of {width}×{height}"));
    }
    let (w, h) = (width as usize, height as usize);
    let colors = match bits {
        1 => 2,
        32 => 0,
        _ => return Err(format!("{bits}-bit cursor")),
    };
    let palette_at = header;
    let xor_at = palette_at + 4 * colors;
    let xor_stride = (w * bits as usize).div_ceil(32) * 4;
    let and_at = xor_at + xor_stride * h;
    let and_stride = w.div_ceil(32) * 4;
    if d.len() < and_at + and_stride * h {
        return Err("short cursor bitmap".into());
    }
    let masked = |x: usize, y: usize| d[and_at + (h - 1 - y) * and_stride + x / 8] & (0x80 >> (x % 8)) != 0;
    let mut rgba = vec![0u8; w * h * 4];
    let any_alpha = bits == 32 && (0..h).any(|y| (0..w).any(|x| d[xor_at + y * xor_stride + 4 * x + 3] != 0));
    for y in 0..h {
        let row = xor_at + (h - 1 - y) * xor_stride;
        for x in 0..w {
            let px = match bits {
                32 => {
                    let p = &d[row + 4 * x..row + 4 * x + 4];
                    let a = if any_alpha { p[3] } else if masked(x, y) { 0 } else { 255 };
                    [p[2], p[1], p[0], a]
                }
                _ => {
                    let i = (d[row + x / 8] >> (7 - x % 8)) & 1;
                    let c = &d[palette_at + 4 * i as usize..palette_at + 4 * i as usize + 4];
                    // (AND set: the screen shows through; XOR set too, it
                    // is inverted: drawn black.)
                    match (masked(x, y), i) {
                        (true, 0) => [0; 4],
                        (true, _) => [0, 0, 0, 255],
                        (false, _) => [c[2], c[1], c[0], 255],
                    }
                }
            };
            rgba[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&px);
        }
    }
    Ok(CursorImage { width: w as u32, height: h as u32, rgba, hotspot: (0, 0) })
}

/// A PE file's resource section.
struct Resources<'a> {
    pe: &'a [u8],
    /// The resource directory's file offset.
    base: usize,
    /// (RVA, raw size, file offset) of each section.
    sections: Vec<(u32, u32, u32)>,
}

impl<'a> Resources<'a> {
    fn of(pe: &'a [u8]) -> Result<Self, String> {
        if pe.get(0..2) != Some(b"MZ") {
            return Err("not a PE file".into());
        }
        let nt = u32_at(pe, 0x3c)? as usize;
        if pe.get(nt..nt + 4) != Some(b"PE\0\0") {
            return Err("not a PE file".into());
        }
        let coff = nt + 4;
        let n_sections = u16_at(pe, coff + 2)? as usize;
        let opt_size = u16_at(pe, coff + 16)? as usize;
        let opt = coff + 20;
        let dirs = match u16_at(pe, opt)? {
            0x10b => opt + 96,
            0x20b => opt + 112,
            m => return Err(format!("optional header {m:#x}")),
        };
        let rva = u32_at(pe, dirs + 2 * 8)?;
        if rva == 0 {
            return Err("no resources".into());
        }
        let table = opt + opt_size;
        let sections = (0..n_sections)
            .map(|i| {
                let s = table + 40 * i;
                Ok((u32_at(pe, s + 12)?, u32_at(pe, s + 16)?.max(u32_at(pe, s + 8)?), u32_at(pe, s + 20)?))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut r = Resources { pe, base: 0, sections };
        r.base = r.offset(rva).ok_or("resources outside the sections")?;
        Ok(r)
    }

    fn offset(&self, rva: u32) -> Option<usize> {
        self.sections.iter().find(|&&(start, len, _)| rva >= start && rva - start < len).map(|&(start, _, raw)| (raw + (rva - start)) as usize)
    }

    /// The entries of the directory at `dir` (offset in the section):
    /// (id, offset of its subdirectory or data entry, whether a directory);
    /// named entries are skipped.
    fn entries(&self, dir: usize) -> Option<Vec<(u32, usize, bool)>> {
        let at = self.base + dir;
        let named = u16_at(self.pe, at + 12).ok()? as usize;
        let ids = u16_at(self.pe, at + 14).ok()? as usize;
        (named..named + ids)
            .map(|i| {
                let e = at + 16 + 8 * i;
                let id = u32_at(self.pe, e).ok()?;
                let to = u32_at(self.pe, e + 4).ok()?;
                Some((id, (to & 0x7fff_ffff) as usize, to & 0x8000_0000 != 0))
            })
            .collect()
    }

    /// The data of resource `kind`/`id` (its first language).
    fn find(&self, kind: u32, id: u32) -> Option<&'a [u8]> {
        let (_, types, _) = self.entries(0)?.into_iter().find(|&(k, _, dir)| k == kind && dir)?;
        let (_, langs, _) = self.entries(types)?.into_iter().find(|&(n, _, dir)| n == id && dir)?;
        let (_, data, dir) = *self.entries(langs)?.first()?;
        if dir {
            return None;
        }
        let entry = self.base + data;
        let (rva, len) = (u32_at(self.pe, entry).ok()?, u32_at(self.pe, entry + 4).ok()? as usize);
        let at = self.offset(rva)?;
        self.pe.get(at..at + len)
    }
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| "truncated".into())
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| "truncated".into())
}

fn i32_at(b: &[u8], at: usize) -> Result<i32, String> {
    u32_at(b, at).map(|v| v as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×2 32-bit cursor DIB: alpha per pixel; without any, the AND mask.
    #[test]
    fn a_32_bit_cursor_dib_is_its_colours_and_alpha() {
        let dib_of = |alpha: u8| {
            let mut d = vec![0u8; 40];
            d[0] = 40;
            d[4] = 2;
            d[8] = 4;
            d[12] = 1;
            d[14] = 32;
            // Rows bottom-up, BGRA.
            d.extend([1, 2, 3, alpha, 4, 5, 6, alpha, 7, 8, 9, alpha, 10, 11, 12, 0]);
            // AND mask rows (4 bytes each), bottom-up: bottom-left masked.
            d.extend([0x80, 0, 0, 0, 0, 0, 0, 0]);
            d
        };
        let c = dib(&dib_of(255)).unwrap();
        assert_eq!((c.width, c.height), (2, 2));
        assert_eq!(&c.rgba[0..4], &[9, 8, 7, 255]);
        assert_eq!(&c.rgba[8..12], &[3, 2, 1, 255]);
        assert_eq!(c.rgba[7], 0);
        let c = dib(&dib_of(0)).unwrap();
        assert_eq!((c.rgba[3], c.rgba[7], c.rgba[11], c.rgba[15]), (255, 255, 0, 255));
    }

    /// A PE32 file whose only section holds a resource directory of
    /// `resources` (type, id, data), each in language 1033.
    fn pe_with(resources: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
        const RVA: u32 = 0x1000;
        const RAW: usize = 0x200;
        let mut types: Vec<u32> = resources.iter().map(|r| r.0).collect();
        types.dedup();
        // Directories first (root, then per type its names, then per
        // resource its languages), then the data entries, then the data.
        let dir = |n: usize| 16 + 8 * n;
        let root = 0;
        let mut at = dir(types.len());
        let type_dirs: Vec<usize> = types.iter().map(|&t| {
            let d = at;
            at += dir(resources.iter().filter(|r| r.0 == t).count());
            d
        }).collect();
        let lang_dirs: Vec<usize> = resources.iter().map(|_| { let d = at; at += dir(1); d }).collect();
        let data_entries: Vec<usize> = resources.iter().map(|_| { let d = at; at += 16; d }).collect();
        let data_at: Vec<usize> = resources.iter().map(|r| { let d = at; at += r.2.len().next_multiple_of(4); d }).collect();
        let mut s = vec![0u8; at];
        let put = |s: &mut Vec<u8>, at: usize, v: u32| s[at..at + 4].copy_from_slice(&v.to_le_bytes());
        let header = |s: &mut Vec<u8>, at: usize, ids: usize| s[at + 14..at + 16].copy_from_slice(&(ids as u16).to_le_bytes());
        header(&mut s, root, types.len());
        for (i, &t) in types.iter().enumerate() {
            put(&mut s, root + 16 + 8 * i, t);
            put(&mut s, root + 20 + 8 * i, 0x8000_0000 | type_dirs[i] as u32);
            let mine: Vec<usize> = (0..resources.len()).filter(|&r| resources[r].0 == t).collect();
            header(&mut s, type_dirs[i], mine.len());
            for (k, &r) in mine.iter().enumerate() {
                put(&mut s, type_dirs[i] + 16 + 8 * k, resources[r].1);
                put(&mut s, type_dirs[i] + 20 + 8 * k, 0x8000_0000 | lang_dirs[r] as u32);
            }
        }
        for (r, res) in resources.iter().enumerate() {
            header(&mut s, lang_dirs[r], 1);
            put(&mut s, lang_dirs[r] + 16, 1033);
            put(&mut s, lang_dirs[r] + 20, data_entries[r] as u32);
            put(&mut s, data_entries[r], RVA + data_at[r] as u32);
            put(&mut s, data_entries[r] + 4, res.2.len() as u32);
            s[data_at[r]..data_at[r] + res.2.len()].copy_from_slice(&res.2);
        }
        let mut pe = vec![0u8; RAW];
        pe[0..2].copy_from_slice(b"MZ");
        put(&mut pe, 0x3c, 0x40);
        pe[0x40..0x44].copy_from_slice(b"PE\0\0");
        let coff = 0x44;
        pe[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
        pe[coff + 16..coff + 18].copy_from_slice(&224u16.to_le_bytes());
        let opt = coff + 20;
        pe[opt..opt + 2].copy_from_slice(&0x10bu16.to_le_bytes());
        put(&mut pe, opt + 96 + 16, RVA);
        put(&mut pe, opt + 96 + 20, s.len() as u32);
        let section = opt + 224;
        put(&mut pe, section + 8, s.len() as u32);
        put(&mut pe, section + 12, RVA);
        put(&mut pe, section + 16, s.len() as u32);
        put(&mut pe, section + 20, RAW as u32);
        pe.extend(s);
        pe
    }

    /// A 1-bit 32×32 cursor resource (hot spot, DIB): a black bar in rows
    /// 4..8 with a white middle row, the rest clear.
    fn bar_cursor(hotspot: (u16, u16)) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend(hotspot.0.to_le_bytes());
        d.extend(hotspot.1.to_le_bytes());
        let mut header = [0u8; 40];
        header[0] = 40;
        header[4] = 32;
        header[8] = 64;
        header[12] = 1;
        header[14] = 1;
        d.extend(header);
        d.extend([0, 0, 0, 0, 255, 255, 255, 0]);
        // XOR rows bottom-up: index 1 (white) in row 6.
        for y in (0..32).rev() { d.extend(if y == 6 { [0xff; 4] } else { [0; 4] }); }
        // AND rows bottom-up: opaque in rows 4..8.
        for y in (0..32).rev() { d.extend(if (4..8).contains(&y) { [0; 4] } else { [0xff; 4] }); }
        d
    }

    fn group(entries: &[(u16, u16, u16)]) -> Vec<u8> {
        let mut g = vec![0, 0, 2, 0];
        g.extend((entries.len() as u16).to_le_bytes());
        for &(w, bits, id) in entries {
            g.extend(w.to_le_bytes());
            g.extend((w * 2).to_le_bytes());
            g.extend(1u16.to_le_bytes());
            g.extend(bits.to_le_bytes());
            g.extend(0u32.to_le_bytes());
            g.extend(id.to_le_bytes());
        }
        g
    }

    /// Windows' own `user32.dll` has no group named 32514: the hourglass
    /// is group 102. Without either the cursor is missing, as before.
    #[test]
    fn windows_numbers_its_standard_cursors_from_100() {
        assert_eq!(windows_group(32514), Some(102));
        assert_eq!(windows_group(32512), Some(100));
        assert_eq!(windows_group(32650), None, "only groups checked against Windows' pictures");
        let windows = pe_with(&[(RT_CURSOR, 7, bar_cursor((15, 15))), (RT_GROUP_CURSOR, 102, group(&[(32, 1, 7)]))]);
        let c = cursor(&windows, 32514, 32).unwrap();
        assert_eq!((c.width, c.height, c.hotspot), (32, 32, (15, 15)));
        let at = |x: usize, y: usize| &c.rgba[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(at(3, 5), [0, 0, 0, 255]);
        assert_eq!(at(3, 6), [255, 255, 255, 255]);
        assert_eq!(at(3, 9)[3], 0);
        assert_eq!(cursor(&windows, 32515, 32).unwrap_err(), "no such group cursor");
        // Wine's numbering wins where both are present.
        let wine = pe_with(&[(RT_CURSOR, 7, bar_cursor((15, 15))), (RT_CURSOR, 8, bar_cursor((1, 2))),
            (RT_GROUP_CURSOR, 102, group(&[(32, 1, 7)])), (RT_GROUP_CURSOR, 32514, group(&[(32, 1, 8)]))]);
        assert_eq!(cursor(&wine, 32514, 32).unwrap().hotspot, (1, 2));
    }

    /// Windows 11's own `user32.dll` (`CIV3R_WINDOWS_USER32`, e.g. copied
    /// from `C:\Windows\SysWOW64`): its hourglass, 32×32, upright.
    #[test]
    fn windows_wait_cursor_is_found_in_its_own_user32() {
        let Some(dll) = std::env::var_os("CIV3R_WINDOWS_USER32") else { return eprintln!("CIV3R_WINDOWS_USER32 unset, skipped") };
        let c = find(&[PathBuf::from(dll)], 32514, 32).unwrap();
        assert_eq!((c.width, c.height), (32, 32));
        let opaque = |y: usize| (0..32).filter(|x| c.rgba[(y * 32 + x) * 4 + 3] != 0).count();
        // Wide glass ends at top and bottom, a narrow waist between.
        assert!(opaque(0) >= 12 && opaque(21) >= 12 && opaque(10) <= 6, "{} {} {}", opaque(0), opaque(10), opaque(21));
    }

    /// Wine's IDC_WAIT, where a Wine prefix is at hand: a 32×32 hourglass
    /// with its hot spot in its middle.
    #[test]
    fn wines_wait_cursor_is_found_in_its_user32() {
        let prefix = std::env::var("WINEPREFIX").unwrap_or_else(|_| "/mnt/datasets/winvm/wine-civ3".into());
        let dll = Path::new(&prefix).join("drive_c/windows/syswow64/user32.dll");
        if !dll.is_file() {
            return eprintln!("no Wine prefix, skipped");
        }
        let c = find(&[dll], 32514, 32).unwrap();
        assert_eq!((c.width, c.height), (32, 32));
        assert_eq!(c.hotspot, (15, 15));
        let opaque = c.rgba.chunks(4).filter(|p| p[3] != 0).count();
        assert!(opaque > 100 && opaque < 32 * 32, "{opaque} opaque pixels");
    }
}

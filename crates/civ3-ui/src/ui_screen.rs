//! The mirrored interface (`ui`) as a list of things to draw, and the
//! frame cut where the menus start. How our renderer draws them (art,
//! fonts, sprites) is the bridge's `ui_screen`.

use std::path::PathBuf;

use menu::screen::Element;

use crate::text::TextRun;
use crate::ui::Frame;

/// One thing to draw, in order; `true`: part of an underlay (drawn under
/// the fog of war, see the bridge's `ui_screen::sprites`).
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Element(Element, bool),
    Text(TextRun, [i32; 4], bool),
}

/// A path the game opened, relative to the art root it lies in (the
/// loader searches the same roots), else as it is.
pub fn relative_path(path: &str, roots: &[PathBuf]) -> String {
    let norm = |s: &str| s.replace('/', "\\").to_ascii_lowercase();
    let p = norm(path);
    for root in roots {
        let mut r = norm(&root.to_string_lossy());
        if !r.ends_with('\\') {
            r.push('\\');
        }
        if p.starts_with(&r) {
            return path[r.len()..].replace('\\', "/");
        }
    }
    // Relative to the game's working folder (the install's `Conquests`,
    // the first root after any scenario's own): as it is, so a scenario's
    // folder (`Conquests\WWII in the Pacific\Art\...`) stays part of it.
    let rooted = p.starts_with('\\') || p.as_bytes().get(1) == Some(&b':');
    let p_rel = p.trim_start_matches(".\\");
    if !rooted && !p_rel.starts_with("..") {
        return path[p.len() - p_rel.len()..].replace('\\', "/");
    }
    // Under no known root (e.g. a trace read elsewhere, or the install
    // seen under another drive or prefix): from its `Art` folder on, which
    // the art search resolves the same way; for a scenario's own art from
    // its `Conquests` or `civ3PTW` folder on, resolved in the install root,
    // whatever art folders are in use.
    let Some(i) = p.find("\\art\\") else { return path.replace('\\', "/") };
    let parts: Vec<&str> = p[..i].split('\\').collect();
    let anchor = parts.iter().rposition(|c| *c == "conquests" || *c == "civ3ptw").filter(|&k| k + 1 < parts.len());
    match anchor {
        Some(k) => {
            let start: usize = parts[..k].iter().map(|c| c.len() + 1).sum();
            path[start..].replace('\\', "/")
        }
        None => path[i + 1..].replace('\\', "/"),
    }
}

/// What an opaque screen starting at draw `first` leaves of `frame`: the
/// underlays (map labels; they go with the map, which is shown in its own
/// view) and the draws from `first` on.
pub fn from_draw(frame: &Frame, first: usize) -> Frame {
    let under: Vec<usize> = frame.underlay_draws.iter().flat_map(|r| r.clone()).filter(|&i| i < first).collect();
    let mut draws: Vec<_> = under.iter().map(|&i| frame.draws[i].clone()).collect();
    let n = draws.len();
    draws.extend(frame.draws[first.min(frame.draws.len())..].iter().cloned());
    let mut underlay_draws = vec![0..n];
    underlay_draws.extend(frame.underlay_draws.iter().filter(|r| r.start >= first).map(|r| r.start - first + n..r.end - first + n));
    Frame { overlay_draws: 0..0, draws, forms: frame.forms.clone(), form_draws: Vec::new(), underlay_draws }
}

/// Where the menus start in `frame`'s draws, over the game's main screen
/// (`frame.draws.len()`: no menu). The main screen is composed first: its
/// forms filling the screen (the map, the frame around it), then its
/// buttons (small forms); the first other form starts the menus (an
/// advisor, a popup, a full-screen menu). A frame cut by [`from_draw`]
/// (an opaque screen) is all menu but its underlays.
pub fn menus_from(frame: &Frame) -> usize {
    if frame.form_draws.is_empty() {
        return frame.underlay_draws.first().filter(|r| r.start == 0).map_or(0, |r| r.end).min(frame.draws.len());
    }
    // A button: no larger than this each way.
    const BUTTON: i32 = 64;
    let screen = frame.forms.first().map(|f| (f.2, f.3));
    let mut buttons = false;
    let first_menu = frame.forms.iter().position(|&(_, _, origin, size)| {
        let full = !buttons && Some((origin, size)) == screen;
        let button = size.0 <= BUTTON && size.1 <= BUTTON;
        buttons |= button;
        !full && !button
    });
    let Some(first_menu) = first_menu else { return frame.draws.len() };
    let canvases: Vec<u32> = frame.forms[first_menu..].iter().map(|f| f.1).collect();
    frame
        .form_draws
        .iter()
        .filter(|(canvas, _)| canvases.contains(canvas))
        .map(|(_, r)| r.start)
        .min()
        .unwrap_or(frame.draws.len())
        .min(frame.draws.len())
}

/// `frame` cut at draw `at`: the draws before (with the underlays among
/// them), and those from `at` on.
pub fn split(frame: &Frame, at: usize) -> (Frame, Frame) {
    let at = at.min(frame.draws.len());
    let part = |range: std::ops::Range<usize>| {
        let clip = |r: &std::ops::Range<usize>| r.start.max(range.start)..r.end.min(range.end);
        Frame {
            overlay_draws: 0..0,
            draws: frame.draws[range.clone()].to_vec(),
            forms: frame.forms.clone(),
            form_draws: frame.form_draws.iter().map(|(c, r)| (*c, clip(r))).filter(|(_, r)| !r.is_empty()).map(|(c, r)| (c, r.start - range.start..r.end - range.start)).collect(),
            underlay_draws: frame.underlay_draws.iter().map(clip).filter(|r| !r.is_empty()).map(|r| r.start - range.start..r.end - range.start).collect(),
        }
    };
    (part(0..at), part(at..frame.draws.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ScreenDraw;

    #[test]
    fn paths_become_relative_to_their_root() {
        let roots = [PathBuf::from("C:\\Games\\Civ3\\Conquests"), PathBuf::from("C:\\Games\\Civ3")];
        assert_eq!(relative_path("C:\\Games\\Civ3\\Conquests\\Art\\Advisors\\a.pcx", &roots), "Art/Advisors/a.pcx");
        assert_eq!(relative_path("c:\\games\\civ3\\Art\\x.PCX", &roots), "Art/x.PCX");
        assert_eq!(relative_path("D:\\other\\y.pcx", &roots), "D:/other/y.pcx");
        assert_eq!(relative_path("D:\\x\\Conquests\\Art\\Civilopedia\\a.pcx", &[]), "Art/Civilopedia/a.pcx");
    }

    #[test]
    fn a_scenarios_own_art_keeps_its_folder_whatever_the_art_folders() {
        let roots = [PathBuf::from("C:\\Games\\Civ3\\Conquests"), PathBuf::from("C:\\Games\\Civ3")];
        let scenario = [PathBuf::from("C:\\Games\\Civ3\\Conquests\\Conquests\\WWII in the Pacific"), roots[0].clone(), roots[1].clone()];
        let opened = "C:\\Games\\Civ3\\Conquests\\Conquests\\WWII in the Pacific\\Art\\Leaderheads\\american_large.pcx";
        assert_eq!(relative_path(opened, &scenario), "Art/Leaderheads/american_large.pcx");
        assert_eq!(relative_path(opened, &roots), "Conquests/WWII in the Pacific/Art/Leaderheads/american_large.pcx");
        // Relative to the game's working folder (`Conquests`).
        assert_eq!(relative_path("Conquests\\WWII in the Pacific\\Art\\Advisors\\china_all.pcx", &roots), "Conquests/WWII in the Pacific/Art/Advisors/china_all.pcx");
        assert_eq!(relative_path(".\\Art\\x.pcx", &roots), "Art/x.pcx");
        assert_eq!(relative_path("..\\Art\\x.pcx", &roots), "Art/x.pcx");
        // The install seen under another prefix.
        assert_eq!(relative_path("E:\\Civ3\\Conquests\\Conquests\\WWII in the Pacific\\Art\\Flics\\japan_T.flc", &roots), "Conquests/WWII in the Pacific/Art/Flics/japan_T.flc");
        assert_eq!(relative_path("E:\\Civ3\\Conquests\\Scenarios\\Mod\\Art\\a.pcx", &roots), "Conquests/Scenarios/Mod/Art/a.pcx");
        assert_eq!(relative_path("E:\\Civ3\\civ3PTW\\Art\\a.pcx", &roots), "Art/a.pcx");
    }

    #[test]
    fn menus_start_at_the_first_form_beyond_the_main_screen() {
        // As traced at 1920x1080 with the domestic advisor open: the map,
        // the frame, its buttons, then the advisor (and its own buttons).
        let fill = |i| ScreenDraw::Fill { rect: [i, 0, 1, 1], rgb: [0; 3], alpha: 255, clip: [0; 4] };
        let full = (1920, 1080);
        let frame = Frame {
            overlay_draws: 0..0,
            draws: (0..10).map(fill).collect(),
            forms: vec![
                (0x66b3d8, 1, (0, 0), full),
                (0x66b570, 2, (0, 0), full),
                (0x6656c0, 3, (57, 12), (36, 30)),
                (0x6688b8, 4, (448, 156), (1024, 768)),
                (0x6656c0, 5, (1183, 267), (12, 12)),
            ],
            form_draws: vec![(1, 0..4), (2, 4..6), (3, 6..7), (4, 7..9), (5, 9..10)],
            underlay_draws: vec![0..4],
        };
        assert_eq!(menus_from(&frame), 7);
        let (main, menus) = split(&frame, 7);
        assert_eq!((main.draws.len(), menus.draws.len()), (7, 3));
        assert_eq!(main.underlay_draws, [0..4]);
        assert!(menus.underlay_draws.is_empty());
        assert_eq!(menus.form_draws, [(4, 0..2), (5, 2..3)]);
        // The main screen alone: no menus. A full-screen form after the
        // buttons is a menu (e.g. diplomacy).
        let main_only = Frame { forms: frame.forms[..3].to_vec(), form_draws: frame.form_draws[..3].to_vec(), ..frame.clone() };
        assert_eq!(menus_from(&main_only), 10);
        let mut over = main_only.clone();
        over.forms.push((0x123, 9, (0, 0), full));
        over.form_draws.push((9, 7..10));
        assert_eq!(menus_from(&over), 7);
        // An opaque screen (cut from its first draw): all menu but the
        // underlays carried along.
        assert_eq!(menus_from(&from_draw(&frame, 7)), 4);
    }
}

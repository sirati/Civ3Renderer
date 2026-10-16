//! Decodes real game files when an installation is available
//! (`CIV3_INSTALL`, default: the copy on this machine). Skipped otherwise.

use civ3_assets::{GameFiles, flc, pcx};

fn files() -> Option<GameFiles> {
    let install = std::env::var("CIV3_INSTALL")
        .unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
    std::path::Path::new(&install).is_dir().then(|| GameFiles::complete(install))
}

#[test]
fn every_terrain_pcx_decodes() {
    let Some(files) = files() else { return eprintln!("no Civ III install, skipped") };
    let mut n = 0;
    for root in files.roots() {
        let Ok(dir) = std::fs::read_dir(root.join("Art/Terrain")) else { continue };
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcx")) {
                let img = pcx::decode(&std::fs::read(&path).unwrap()).unwrap_or_else(|e| panic!("{path:?}: {e}"));
                assert_eq!(img.pixels.len() as u32, img.width * img.height);
                n += 1;
            }
        }
    }
    assert!(n > 20, "only {n} terrain files found");
}

#[test]
fn base_terrain_sheet_is_a_9_by_9_grid_of_128_by_64() {
    let Some(files) = files() else { return };
    let img = pcx::decode(&files.read("Art/Terrain/xtgc.pcx").unwrap()).unwrap();
    assert_eq!((img.width, img.height), (9 * 128, 9 * 64));
}

#[test]
fn a_unit_animation_decodes() {
    let Some(files) = files() else { return };
    let bytes = files.read("Art/Units/Warrior/WarriorDefault.flc").or_else(|_| files.read("Art/Units/Warrior/WarriorRun.flc")).unwrap();
    let anim = flc::decode(&bytes).unwrap();
    assert!(!anim.frames.is_empty());
    assert!(anim.width > 0 && anim.height > 0);
    // Frames differ (it is an animation) and use more than one colour.
    let first = &anim.frames[0].pixels;
    assert!(first.iter().any(|&p| p != first[0]));
}

#[test]
fn every_unit_animation_decodes() {
    let Some(files) = files() else { return };
    let mut n = 0;
    let mut failures = Vec::new();
    for root in files.roots() {
        let Ok(units) = std::fs::read_dir(root.join("Art/Units")) else { continue };
        for unit in units.flatten() {
            let Ok(dir) = std::fs::read_dir(unit.path()) else { continue };
            for entry in dir.flatten() {
                let path = entry.path();
                if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("flc")) {
                    continue;
                }
                n += 1;
                match flc::decode(&std::fs::read(&path).unwrap()) {
                    Ok(anim) if anim.frames.len() >= anim.header_frames => {}
                    Ok(anim) => failures.push(format!("{path:?}: {} of {} frames", anim.frames.len(), anim.header_frames)),
                    Err(e) => failures.push(format!("{path:?}: {e}")),
                }
            }
        }
    }
    eprintln!("{n} unit animations");
    assert!(failures.is_empty(), "{} of {n} failed:\n{}", failures.len(), failures.join("\n"));
}

//! The exact application icon from the original Windows game executable.

pub fn application() -> Option<application_icon::ApplicationIcon> {
    match application_icon::ApplicationIcon::from_png(include_bytes!(
        "../assets/civ3-conquests-icon.png"
    )) {
        Ok(icon) => Some(icon),
        Err(error) => {
            log::warn!("game application icon: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The same image embedded by the game's native host.
    const GAME_ICON: &[u8] = include_bytes!("../assets/civ3-conquests-icon.png");

    #[test]
    fn decodes_the_original_game_icon_without_changing_its_size_or_bytes() {
        let icon = application().unwrap();
        assert_eq!(icon.size(), (32, 32));
        assert_eq!(icon.png(), GAME_ICON);
        let rgba = image::load_from_memory_with_format(icon.png(), image::ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!(rgba.get_pixel(16, 0).0, [255, 0, 0, 255]);
        assert_eq!(rgba.get_pixel(0, 16).0, [192, 192, 192, 255]);
        assert_eq!(rgba.pixels().filter(|p| p[3] == 255).count(), 1024);
        let colors: std::collections::HashSet<_> = rgba.pixels().map(|p| p.0).collect();
        assert_eq!(colors.len(), 7);
    }
}

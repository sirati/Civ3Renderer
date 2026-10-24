//! Civilization III's screens as the viewer arranges them: the generic
//! viewer never reads a screen's name, the driver tells it the screen's
//! role (`scene::ScreenRole`).

use scene::ScreenRole;

/// The role of the screen `name` (as `civ3_sys::ui_hooks::screen_forms`
/// names them): the city screen is a site's screen, "Menu" the game's
/// menu (Preferences is placed as "Menu" too); the advisors, the palace,
/// the Civilopedia and the rest are screens of their own.
pub fn role(name: &str) -> ScreenRole {
    match name {
        "City" => ScreenRole::Site,
        "Menu" => ScreenRole::Menu,
        _ => ScreenRole::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_city_screen_is_a_site_and_menu_the_menu() {
        assert_eq!(role("City"), ScreenRole::Site);
        assert_eq!(role("Menu"), ScreenRole::Menu);
        for other in ["F1", "F4", "Palace", "Spy", "Help", "Civilopedia", "Diplomacy", "city"] {
            assert_eq!(role(other), ScreenRole::Other, "{other}");
        }
    }
}

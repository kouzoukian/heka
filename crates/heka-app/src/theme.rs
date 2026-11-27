use std::borrow::Cow;

use gpui::{App, Result};
use gpui_component::{Theme, ThemeRegistry};
use heka_core::app_settings::ThemeChoice;

pub const MIN_DARK_JSON: &str = include_str!("../themes/min-dark.json");

pub const MIN_DARK: &str = "Min Dark (Solid)";

pub const MIN_LIGHT_JSON: &str = include_str!("../themes/min-light.json");

pub const MIN_LIGHT: &str = "Min Light (Solid)";

pub const FONT: &[u8] = include_bytes!("../fonts/GoogleSansCode-VariableFont_wght.ttf");

pub const FONT_FAMILY: &str = "Google Sans Code";

pub fn load(cx: &mut App) -> Result<()> {
    cx.text_system().add_fonts(vec![Cow::Borrowed(FONT)])?;
    for themes in [MIN_DARK_JSON, MIN_LIGHT_JSON] {
        ThemeRegistry::global_mut(cx).load_themes_from_str(themes)?;
    }
    show(ThemeChoice::default(), cx);
    Ok(())
}

pub fn show(choice: ThemeChoice, cx: &mut App) {
    let name = match choice {
        ThemeChoice::Dark => MIN_DARK,
        ThemeChoice::Light => MIN_LIGHT,
    };
    let theme = ThemeRegistry::global(cx)
        .themes()
        .get(name)
        .cloned()
        .expect("both built-in themes are loaded");
    Theme::update(cx, |current| current.apply_config(&theme));
}

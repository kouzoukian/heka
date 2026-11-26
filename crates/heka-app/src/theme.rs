use std::borrow::Cow;

use gpui::{App, Result};
use gpui_component::{Theme, ThemeRegistry};

pub const MIN_DARK_JSON: &str = include_str!("../themes/min-dark.json");

pub const MIN_DARK: &str = "Min Dark (Solid)";

pub const FONT: &[u8] = include_bytes!("../fonts/GoogleSansCode-VariableFont_wght.ttf");

pub const FONT_FAMILY: &str = "Google Sans Code";

pub fn apply(cx: &mut App) -> Result<()> {
    cx.text_system().add_fonts(vec![Cow::Borrowed(FONT)])?;
    ThemeRegistry::global_mut(cx).load_themes_from_str(MIN_DARK_JSON)?;
    let theme = ThemeRegistry::global(cx)
        .themes()
        .get(MIN_DARK)
        .cloned()
        .expect("the theme file holds Min Dark");
    Theme::update(cx, |current| current.apply_config(&theme));
    Ok(())
}

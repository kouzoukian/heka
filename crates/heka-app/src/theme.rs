//! The Min Dark (Solid) theme for gpui-component (ADR 0002).
//!
//! `themes/min-dark.json` is a conversion of the Zed port of Miguel Solorio's
//! Min Theme. Both are MIT licensed: their notices are `themes/LICENSE-MIN-THEME`.

use gpui::{App, Result};
use gpui_component::{Theme, ThemeRegistry};

/// The theme file built into heka.
pub const MIN_DARK_JSON: &str = include_str!("../themes/min-dark.json");

/// The theme's name in [`MIN_DARK_JSON`].
pub const MIN_DARK: &str = "Min Dark (Solid)";

/// Makes Min Dark the theme. Call after `gpui_component::init`.
pub fn apply(cx: &mut App) -> Result<()> {
    ThemeRegistry::global_mut(cx).load_themes_from_str(MIN_DARK_JSON)?;
    let theme = ThemeRegistry::global(cx)
        .themes()
        .get(MIN_DARK)
        .cloned()
        .expect("the theme file holds Min Dark");
    Theme::update(cx, |current| current.apply_config(&theme));
    Ok(())
}

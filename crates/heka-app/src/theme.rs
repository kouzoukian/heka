//! The Min Dark (Solid) theme for gpui-component and the font it asks for
//! (ADR 0002).
//!
//! `themes/min-dark.json` is a conversion of the Zed port of Miguel Solorio's
//! Min Theme. Both are MIT licensed: their notices are `themes/LICENSE-MIN-THEME`.
//! Google Sans Code is under the SIL Open Font License:
//! `fonts/LICENSE-GOOGLE-SANS-CODE`.

use std::borrow::Cow;

use gpui::{App, Result};
use gpui_component::{Theme, ThemeRegistry};

/// The theme file built into heka.
pub const MIN_DARK_JSON: &str = include_str!("../themes/min-dark.json");

/// The theme's name in [`MIN_DARK_JSON`].
pub const MIN_DARK: &str = "Min Dark (Solid)";

/// The font built into heka, with every weight in one variable font.
pub const FONT: &[u8] = include_bytes!("../fonts/GoogleSansCode-VariableFont_wght.ttf");

/// The family name inside [`FONT`].
pub const FONT_FAMILY: &str = "Google Sans Code";

/// Loads the font and makes Min Dark the theme. Call after
/// `gpui_component::init`.
pub fn apply(cx: &mut App) -> Result<()> {
    // Before the theme names it, so that it is never looked up while missing.
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

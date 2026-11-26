//! Zed's One Dark theme for gpui-component (ADR 0002).
//!
//! `themes/one-dark.json` is a conversion of Zed's `assets/themes/one/one.json`.
//! One Dark is MIT licensed: its notice is `themes/LICENSE-ONE-DARK`.

use gpui::{App, Result};
use gpui_component::{Theme, ThemeRegistry};

/// The theme file built into heka.
pub const ONE_DARK_JSON: &str = include_str!("../themes/one-dark.json");

/// The theme's name in [`ONE_DARK_JSON`].
pub const ONE_DARK: &str = "One Dark";

/// Makes One Dark the theme. Call after `gpui_component::init`.
pub fn apply(cx: &mut App) -> Result<()> {
    ThemeRegistry::global_mut(cx).load_themes_from_str(ONE_DARK_JSON)?;
    let theme = ThemeRegistry::global(cx)
        .themes()
        .get(ONE_DARK)
        .cloned()
        .expect("the theme file holds One Dark");
    Theme::update(cx, |current| current.apply_config(&theme));
    Ok(())
}

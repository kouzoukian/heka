//! The One Dark theme and the icons, as the window loads them.

use gpui::{AssetSource, Rgba};
use gpui_component::ThemeSet;
use heka_app::theme::{ONE_DARK, ONE_DARK_JSON};
use heka_app::ui::assets::{Assets, battery_icon, link_icon};
use heka_app::ui::model::{ActiveLink, Battery};
use serde_json::Value;

#[test]
fn the_theme_file_holds_the_dark_theme_heka_asks_for() {
    let set: ThemeSet = serde_json::from_str(ONE_DARK_JSON).expect("a gpui-component theme");

    let [theme] = &set.themes[..] else {
        panic!("one theme expected");
    };
    assert_eq!(theme.name, ONE_DARK);
    assert!(theme.mode.is_dark());
    // Zed's editor background, behind the cards.
    assert_eq!(theme.colors.background.as_deref(), Some("#282c33"));
}

/// gpui-component ignores a key it does not know, so a typo would silently
/// leave its default colour.
#[test]
fn gpui_component_knows_every_colour_the_theme_sets() {
    let file: Value = serde_json::from_str(ONE_DARK_JSON).unwrap();
    let written = file["themes"][0]["colors"].as_object().unwrap();
    let set: ThemeSet = serde_json::from_str(ONE_DARK_JSON).unwrap();
    let read = serde_json::to_value(&set.themes[0].colors).unwrap();

    for (key, value) in written {
        assert_eq!(read.get(key), Some(value), "unknown theme key {key}");
        let text = value.as_str().unwrap();
        assert!(Rgba::try_from(text).is_ok(), "{key}: bad colour {text}");
    }
}

#[test]
fn every_link_and_battery_icon_ships_with_the_binary() {
    let links = [None, Some(ActiveLink::Usb), Some(ActiveLink::Bluetooth)];
    let level = |percent, charging| Some(Battery { percent, charging });
    let batteries = [
        None,
        level(5, false),
        level(30, false),
        level(60, false),
        level(100, false),
        level(60, true),
    ];

    let icons = links
        .map(link_icon)
        .into_iter()
        .chain(batteries.map(battery_icon));
    for icon in icons {
        let path = icon.path();
        let loaded = Assets.load(&path).expect("icon asset");
        assert!(loaded.is_some_and(|bytes| !bytes.is_empty()), "{path}");
    }
}

#[test]
fn the_icons_gpui_component_draws_with_ship_too() {
    // The title bar's close button.
    assert!(Assets.load("icons/window-close.svg").unwrap().is_some());
}

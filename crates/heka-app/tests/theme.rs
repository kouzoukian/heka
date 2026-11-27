use gpui::{AssetSource, Rgba};
use gpui_component::ThemeSet;
use heka_app::theme::{FONT, FONT_FAMILY, MIN_DARK, MIN_DARK_JSON};
use heka_app::ui::assets::{Assets, battery_icon, link_icon};
use heka_core::link::LinkKind;
use heka_core::state::{Battery, Charge};
use serde_json::Value;

#[test]
fn the_theme_file_holds_the_dark_theme_heka_asks_for() {
    let set: ThemeSet = serde_json::from_str(MIN_DARK_JSON).expect("a gpui-component theme");

    let [theme] = &set.themes[..] else {
        panic!("one theme expected");
    };
    assert_eq!(theme.name, MIN_DARK);
    assert!(theme.mode.is_dark());
    assert_eq!(theme.colors.background.as_deref(), Some("#1a1a1a"));
}

#[test]
fn the_theme_asks_for_the_built_in_font_at_zeds_size() {
    let set: ThemeSet = serde_json::from_str(MIN_DARK_JSON).expect("a gpui-component theme");
    let theme = &set.themes[0];

    assert_eq!(theme.font_family.as_deref(), Some(FONT_FAMILY));
    assert_eq!(theme.mono_font_family.as_deref(), Some(FONT_FAMILY));
    assert_eq!(theme.font_size, Some(15.));
    assert_eq!(FONT.get(..4), Some(&[0, 1, 0, 0][..]));
}

#[test]
fn gpui_component_knows_every_colour_the_theme_sets() {
    let file: Value = serde_json::from_str(MIN_DARK_JSON).unwrap();
    let written = file["themes"][0]["colors"].as_object().unwrap();
    let set: ThemeSet = serde_json::from_str(MIN_DARK_JSON).unwrap();
    let read = serde_json::to_value(&set.themes[0].colors).unwrap();

    for (key, value) in written {
        assert_eq!(read.get(key), Some(value), "unknown theme key {key}");
        let text = value.as_str().unwrap();
        assert!(Rgba::try_from(text).is_ok(), "{key}: bad colour {text}");
    }
}

#[test]
fn every_link_and_battery_icon_ships_with_the_binary() {
    let links = [None, Some(LinkKind::Usb), Some(LinkKind::Bluetooth)];
    let battery = |percent, charge| Some(Battery { percent, charge });
    let batteries = [
        None,
        battery(5, Charge::Discharging),
        battery(35, Charge::Discharging),
        battery(65, Charge::Discharging),
        battery(95, Charge::Discharging),
        battery(65, Charge::Charging),
        battery(100, Charge::Full),
        battery(0, Charge::NotCharging),
        battery(0, Charge::Unknown),
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
fn the_turn_off_icon_ships_with_the_binary() {
    assert!(Assets.load("icons/power.svg").unwrap().is_some());
}

#[test]
fn the_icons_gpui_component_draws_with_ship_too() {
    assert!(Assets.load("icons/window-close.svg").unwrap().is_some());
}

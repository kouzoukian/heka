use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use gpui_kit_assets::{IconName, icon_assets};
use heka_core::link::LinkKind;
use heka_core::state::{Battery, Charge};

icon_assets!(
    ExtraIcons,
    [
        Bluetooth,
        Gamepad2,
        Palette,
        ScrollText,
        SlidersHorizontal,
        Unplug,
        Usb
    ]
);

pub const APP_ICON: &str = "heka/icon.png";

const APP_ICON_PNG: &[u8] = include_bytes!("../../icon/heka.png");

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == APP_ICON {
            return Ok(Some(Cow::Borrowed(APP_ICON_PNG)));
        }
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => gpui_kit_assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit_assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        Ok(paths)
    }
}

pub fn link_icon(link: Option<LinkKind>) -> IconName {
    match link {
        Some(LinkKind::Usb) => IconName::Usb,
        Some(LinkKind::Bluetooth) => IconName::Bluetooth,
        None => IconName::Unplug,
    }
}

pub fn battery_icon(battery: Option<Battery>) -> IconName {
    let Some(Battery { percent, charge }) = battery else {
        return IconName::Battery;
    };
    match charge {
        Charge::Charging => IconName::BatteryCharging,
        Charge::Full => IconName::BatteryFull,
        Charge::NotCharging => IconName::BatteryWarning,
        Charge::Unknown => IconName::Battery,
        Charge::Discharging => match percent {
            0..=10 => IconName::BatteryWarning,
            11..=35 => IconName::BatteryLow,
            36..=75 => IconName::BatteryMedium,
            _ => IconName::BatteryFull,
        },
    }
}

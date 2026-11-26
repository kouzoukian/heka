//! The icons built into heka. Without an asset source gpui-component's icons
//! render blank.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use gpui_kit_assets::{IconName, icon_assets};
use heka_core::link::LinkKind;
use heka_core::state::{Battery, Charge};

// The icons heka uses beyond gpui-component's default bundle.
icon_assets!(ExtraIcons, [Bluetooth, Gamepad2, Unplug, Usb]);

/// gpui-component's default icons and heka's own few.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
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

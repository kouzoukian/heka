//! The icons built into heka. Without an asset source gpui-component's icons
//! render blank.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use gpui_kit_assets::{IconName, icon_assets};

use crate::ui::model::{ActiveLink, Battery};

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

pub fn link_icon(link: Option<ActiveLink>) -> IconName {
    match link {
        Some(ActiveLink::Usb) => IconName::Usb,
        Some(ActiveLink::Bluetooth) => IconName::Bluetooth,
        None => IconName::Unplug,
    }
}

pub fn battery_icon(battery: Option<Battery>) -> IconName {
    match battery {
        None => IconName::Battery,
        Some(Battery { charging: true, .. }) => IconName::BatteryCharging,
        Some(Battery {
            percent: 0..=10, ..
        }) => IconName::BatteryWarning,
        Some(Battery {
            percent: 11..=35, ..
        }) => IconName::BatteryLow,
        Some(Battery {
            percent: 36..=75, ..
        }) => IconName::BatteryMedium,
        Some(_) => IconName::BatteryFull,
    }
}

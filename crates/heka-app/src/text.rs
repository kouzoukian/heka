//! All the text heka shows, in one place. English only.
//!
//! The `{error}` parts come from Windows, the drivers and heka's lower layers,
//! and are shown as they arrive.

use heka_core::lifecycle::MAX_BRIDGES;
use heka_core::link::ControllerModel;
use heka_core::state::{Battery, Charge};

pub const APP_NAME: &str = "heka";

// The Controller list and the selected Controller's pane.
pub const CONTROLLERS: &str = "Controllers";
pub const NO_CONTROLLERS: &str = "No Controllers";
pub const DUALSENSE: &str = "DualSense";
pub const DUALSHOCK4: &str = "DualShock 4";
pub const ROW_STATE: &str = "State";
pub const ROW_LINK: &str = "Link";
pub const ROW_BATTERY: &str = "Battery";
pub const ROW_ADDRESS: &str = "Address";
pub const ROW_SUPPORT: &str = "Support";
/// A DualShock 4's support row: heka's support has not been tried on one.
pub const EXPERIMENTAL: &str = "Experimental";
pub const BRIDGED: &str = "Bridged";
pub const STOPPED: &str = "Stopped";
pub const NOT_BRIDGED: &str = "Not Bridged";
pub const STOP: &str = "Stop";
pub const START: &str = "Start";
pub const USB: &str = "USB";
pub const BLUETOOTH: &str = "Bluetooth";
pub const NO_LINK: &str = "No Link";
pub const BATTERY_UNKNOWN: &str = "Battery unknown";
pub const BATTERY_NOT_CHARGING: &str = "Battery not charging";
pub const OUTPUT_TYPE: &str = "Output Type";
pub const XBOX: &str = "Xbox";
pub const PS4: &str = "PS4";
pub const HELD_HINT: &str = "Reconnect the Controller to keep its Virtual Controller.";

/// A Controller's name as the window shows it.
pub fn controller(model: ControllerModel) -> &'static str {
    match model {
        ControllerModel::DualSense => DUALSENSE,
        ControllerModel::DualShock4 => DUALSHOCK4,
    }
}

pub fn held(seconds_left: u64) -> String {
    format!("Held, {seconds_left} s left")
}

/// What is left of a Hold, in the Controller list.
pub fn held_short(seconds_left: u64) -> String {
    format!("{seconds_left}s")
}

pub fn battery(battery: Option<Battery>) -> String {
    let Some(Battery { percent, charge }) = battery else {
        return BATTERY_UNKNOWN.into();
    };
    match charge {
        Charge::Discharging => format!("{percent}%"),
        Charge::Charging => format!("{percent}%, charging"),
        Charge::Full => format!("{percent}%, full"),
        Charge::NotCharging => BATTERY_NOT_CHARGING.into(),
        Charge::Unknown => BATTERY_UNKNOWN.into(),
    }
}

pub fn bridge_failed(error: &str) -> String {
    format!("Could not Bridge: {error}")
}

pub fn with_error(state: &str, error: &str) -> String {
    format!("{state} ({error})")
}

pub fn all_bridges_taken() -> String {
    format!("{MAX_BRIDGES} Controllers are already Bridged.")
}

// Live drawing: the names printed on a DualShock 4.
pub const L1: &str = "L1";
pub const R1: &str = "R1";
pub const L2: &str = "L2";
pub const R2: &str = "R2";
pub const PS: &str = "PS";

// Live drawing: the names printed on an Xbox pad.
pub const LB: &str = "LB";
pub const RB: &str = "RB";
pub const LT: &str = "LT";
pub const RT: &str = "RT";
pub const A: &str = "A";
pub const B: &str = "B";
pub const X: &str = "X";
pub const Y: &str = "Y";

// Empty window.
pub const STARTING: &str = "Looking for Controllers";
pub const NO_CONTROLLER: &str = "No Controller connected";
pub const CONNECT_HINT: &str =
    "Connect a DualSense or DualShock 4 (experimental) with a USB cable or over Bluetooth.";

// Tray menu.
pub const TRAY_OPEN: &str = "Open heka";
pub const START_WITH_WINDOWS: &str = "Start with Windows";
pub const QUIT: &str = "Quit";

/// The setup screen: which drivers are missing, what that costs and where to get
/// them.
pub mod setup {
    use heka_core::setup::{Driver, Missing};

    pub const CHECK_AGAIN: &str = "Check again";
    /// Between a missing driver and the link to its installer.
    pub const SEPARATOR: &str = "·";

    /// An installer's address as its link shows it, without what every one has.
    pub fn short_url(url: &str) -> &str {
        let url = url.strip_prefix("https://").unwrap_or(url);
        url.strip_suffix("/releases/latest").unwrap_or(url)
    }
    pub const AFTER_INSTALLING: &str =
        "Run the installer yourself, restart Windows if it asks, then check again.";
    /// A Controller's state while ViGEmBus is missing.
    pub const NOT_BRIDGED: &str = "Not Bridged: ViGEmBus is missing";

    pub fn heading(missing: &Missing) -> String {
        let name = name(missing.driver);
        match &missing.error {
            None => format!("{name} is not installed or not running"),
            Some(error) => format!("{name} cannot be used: {error}"),
        }
    }

    pub fn consequence(driver: Driver) -> &'static str {
        match driver {
            Driver::ViGEmBus => {
                "Controllers are listed and show live input, but none is Bridged until it is installed."
            }
            Driver::HidHide => {
                "Bridges work, but games may see each Controller twice (double input): the Controller and its Virtual Controller."
            }
        }
    }

    pub fn name(driver: Driver) -> &'static str {
        match driver {
            Driver::ViGEmBus => "ViGEmBus",
            Driver::HidHide => "HidHide",
        }
    }
}

// Status bar.
pub fn summary(controllers: usize, bridged: usize) -> String {
    match controllers {
        0 => NO_CONTROLLERS.into(),
        1 => format!("1 Controller, {bridged} Bridged"),
        _ => format!("{controllers} Controllers, {bridged} Bridged"),
    }
}

// Notices.
pub fn hid_error(error: &str) -> String {
    format!("Cannot look for Controllers: {error}")
}

pub fn cannot_hide_at_all(error: &str) -> String {
    format!("Controllers cannot be Hidden: {error}")
}

pub fn cannot_restore_earlier_run(error: &str) -> String {
    format!("Could not restore Controllers Hidden by an earlier run: {error}")
}

pub fn cannot_identify_for_hiding(error: &str) -> String {
    format!("cannot identify the Controller for HidHide: {error}")
}

pub const NO_LINK_TO_HIDE: &str = "no Link to Hide";

pub fn not_hidden(error: &str) -> String {
    format!("{error}. Games may see the Controller twice.")
}

pub fn not_allowed(error: &str) -> String {
    format!("Controllers hidden by other tools cannot be found yet: {error}")
}

pub fn cannot_restore(error: &str) -> String {
    format!("Could not restore the Controller: {error}")
}

//! All the text heka shows, in one place. English only.
//!
//! The `{error}` parts come from Windows, the drivers and heka's lower layers,
//! and are shown as they arrive.

use heka_core::lifecycle::MAX_BRIDGES;

pub const APP_NAME: &str = "heka";

// Controller cards.
pub const DUALSENSE: &str = "DualSense";
pub const BRIDGED: &str = "Bridged";
pub const STOPPED: &str = "Stopped";
pub const NOT_BRIDGED: &str = "Not Bridged";
pub const STOP: &str = "Stop";
pub const START: &str = "Start";
pub const USB: &str = "USB";
pub const BLUETOOTH: &str = "Bluetooth";
pub const NO_LINK: &str = "No Link";
pub const BATTERY_UNKNOWN: &str = "Battery unknown";
pub const OUTPUT_TYPE: &str = "Output";
pub const XBOX: &str = "Xbox";
pub const PS4: &str = "PS4";
pub const HELD_HINT: &str = "Reconnect the Controller to keep its Virtual Controller.";

pub fn held(seconds_left: u64) -> String {
    format!("Held, {seconds_left} s left")
}

pub fn battery(percent: u8, charging: bool) -> String {
    if charging {
        format!("{percent}%, charging")
    } else {
        format!("{percent}%")
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

// Live drawing: the names printed on a DualSense.
pub const L1: &str = "L1";
pub const R1: &str = "R1";
pub const L2: &str = "L2";
pub const R2: &str = "R2";
pub const PS: &str = "PS";
pub const CROSS_SYMBOL: &str = "✕";
pub const CIRCLE_SYMBOL: &str = "○";
pub const SQUARE_SYMBOL: &str = "□";
pub const TRIANGLE_SYMBOL: &str = "△";

// Empty window.
pub const STARTING: &str = "Looking for Controllers";
pub const NO_CONTROLLER: &str = "No Controller connected";
pub const CONNECT_HINT: &str = "Connect a DualSense with a USB cable.";

// Tray menu.
pub const TRAY_OPEN: &str = "Open heka";
pub const START_WITH_WINDOWS: &str = "Start with Windows";
pub const QUIT: &str = "Quit";

/// The setup screen: which drivers are missing, what that costs and where to get
/// them.
pub mod setup {
    use heka_core::setup::{Driver, Missing};

    pub const CHECK_AGAIN: &str = "Check again";
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
        0 => "No Controllers".into(),
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

pub fn cannot_restore(error: &str) -> String {
    format!("Could not restore the Controller: {error}")
}

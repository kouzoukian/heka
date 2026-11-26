use heka_core::discovery::{OpenError, Unopened};
use heka_core::hider::{HideError, HidingWarning};
use heka_core::hiding::{
    HidHideError, HidingError, HidingUnavailable, IdentifyError, JournalError,
};
use heka_core::lifecycle::MAX_BRIDGES;
use heka_core::link::{ControllerModel, LinkKind, ReportError};
use heka_core::state::{Battery, Charge};
use heka_core::supervisor::BridgeError;
use heka_core::virtual_controller::OutputType;

pub const APP_NAME: &str = "heka";

pub const CONTROLLERS: &str = "Controllers";
pub const NO_CONTROLLERS: &str = "No Controllers";
pub const DUALSENSE: &str = "DualSense";
pub const DUALSHOCK4: &str = "DualShock 4";
pub const ROW_STATE: &str = "State";
pub const ACTIVE_LINK: &str = "Active Link";
pub const BATTERY: &str = "Battery";
pub const ROW_ADDRESS: &str = "Address";
pub const ROW_SUPPORT: &str = "Support";
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

pub const SETTINGS: &str = "Controller settings";
pub const ROW_NAME: &str = "Name";
pub const ROW_RUMBLE: &str = "Rumble";
pub const ROW_LIGHTBAR: &str = "Light bar";
pub const ROW_PLAYER_LIGHTS: &str = "Player lights";
pub const ROW_HIDE: &str = "Hide from games";
pub const DEFAULT: &str = "Default";
pub const ON: &str = "On";
pub const OFF: &str = "Off";
pub const BLUE: &str = "Blue";
pub const RED: &str = "Red";
pub const GREEN: &str = "Green";
pub const PINK: &str = "Pink";
pub const ORANGE: &str = "Orange";
pub const WHITE: &str = "White";
pub const RAINBOW: &str = "Rainbow";
pub const CUSTOM_COLOUR: &str = "Custom colour";
pub const HUE: &str = "Hue";
pub const SATURATION: &str = "Saturation";
pub const LIGHTNESS: &str = "Lightness";
pub const NAME_TIP: &str = "Shown in place of the Controller's model.";
pub const RUMBLE_TIP: &str = "How strongly the Controller rumbles when a game asks it to.";
pub const LIGHTBAR_TIP: &str =
    "The colour while no game sets one. Default leaves the light bar to the Controller.";
pub const PLAYER_LIGHTS_TIP: &str = "The white lights under the touchpad.";
pub const HIDE_TIP: &str = "Off lets games see the Controller too, which can cause double input. Restart a running game after a change.";
pub const SETTINGS_NOTE: &str = "Rumble and lights apply while the Controller is Bridged.";

pub fn percent(percent: u8) -> String {
    format!("{percent}%")
}

pub fn controller(model: ControllerModel) -> &'static str {
    match model {
        ControllerModel::DualSense => DUALSENSE,
        ControllerModel::DualShock4 => DUALSHOCK4,
    }
}

pub fn held(seconds_left: u64) -> String {
    format!("Held, {seconds_left} s left")
}

pub fn link(link: Option<LinkKind>) -> &'static str {
    match link {
        Some(LinkKind::Usb) => USB,
        Some(LinkKind::Bluetooth) => BLUETOOTH,
        None => NO_LINK,
    }
}

pub fn battery_in_full(state: Option<Battery>) -> String {
    let words = battery(state);
    if words.starts_with(BATTERY) {
        words
    } else {
        format!("{BATTERY} {words}")
    }
}

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

pub const L1: &str = "L1";
pub const R1: &str = "R1";
pub const L2: &str = "L2";
pub const R2: &str = "R2";
pub const PS: &str = "PS";

pub const LB: &str = "LB";
pub const RB: &str = "RB";
pub const LT: &str = "LT";
pub const RT: &str = "RT";
pub const A: &str = "A";
pub const B: &str = "B";
pub const X: &str = "X";
pub const Y: &str = "Y";

pub const STARTING: &str = "Looking for Controllers";
pub const NO_CONTROLLER_CONNECTED: &str = "No Controller connected";
pub const CONNECT_HINT: &str =
    "Connect a DualSense or DualShock 4 (experimental) with a USB cable or over Bluetooth.";

pub const TRAY_OPEN: &str = "Open heka";
pub const START_WITH_WINDOWS: &str = "Start with Windows";
pub const QUIT: &str = "Quit";

pub mod setup {
    use heka_core::setup::{Driver, DriverError, Missing};

    pub const CHECK_AGAIN: &str = "Check again";
    pub const SEPARATOR: &str = "·";

    pub fn short_url(url: &str) -> &str {
        let url = url.strip_prefix("https://").unwrap_or(url);
        url.strip_suffix("/releases/latest").unwrap_or(url)
    }
    pub const AFTER_INSTALLING: &str =
        "Run the installer yourself, restart Windows if it asks, then check again.";
    pub const NOT_BRIDGED: &str = "Not Bridged: ViGEmBus is missing";

    pub fn heading(missing: &Missing) -> String {
        let name = name(missing.driver);
        match &missing.error {
            None => format!("{name} is not installed or not running"),
            Some(error) => format!("{name} cannot be used: {}", reason(error)),
        }
    }

    fn reason(error: &DriverError) -> String {
        match error {
            DriverError::Bus(error)
            | DriverError::HidHide(heka_core::hiding::HidHideError::Failed(error)) => error.clone(),
            DriverError::HidHide(error) => super::hidhide_error(error),
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

pub fn summary(controllers: usize, bridged: usize) -> String {
    match controllers {
        0 => NO_CONTROLLERS.into(),
        1 => format!("1 Controller, {bridged} Bridged"),
        _ => format!("{controllers} Controllers, {bridged} Bridged"),
    }
}

pub fn hid_error(error: &str) -> String {
    format!("Cannot look for Controllers: {error}")
}

pub fn cannot_hide_at_all(error: &str) -> String {
    format!("Controllers cannot be Hidden: {error}")
}

pub fn cannot_restore_earlier_run(error: &str) -> String {
    format!(
        "Could not restore Controllers Hidden by an earlier run, trying again: {}",
        sentence(error)
    )
}

pub fn cannot_identify_for_hiding(error: &str) -> String {
    format!("cannot identify the Controller for HidHide: {error}")
}

pub const NO_LINK_TO_HIDE: &str = "no Link to Hide";

pub fn not_hidden(error: &str) -> String {
    format!(
        "Could not Hide a Controller, so games may see it twice: {}",
        sentence(error)
    )
}

pub fn not_allowed(error: &str) -> String {
    format!("Controllers hidden by other tools cannot be found yet: {error}")
}

pub fn cannot_restore(error: &str) -> String {
    format!(
        "Could not restore a Controller, trying again: {}",
        sentence(error)
    )
}

pub const HIDHIDE_INVERTED: &str = "Games may see each Controller twice. Turn off the inverse application cloak in the HidHide Configuration Client, then close it.";

pub fn hiding_warning(warning: &HidingWarning) -> String {
    match warning {
        HidingWarning::Unavailable(error) => cannot_hide_at_all(&hiding_unavailable(error)),
        HidingWarning::EarlierRunNotRestored(error) => {
            cannot_restore_earlier_run(&hiding_error(error))
        }
        HidingWarning::NotRestored(error) => cannot_restore(&hiding_error(error)),
        HidingWarning::NotAllowed(error) => not_allowed(&hiding_error(error)),
        HidingWarning::NotHidden(HideError::NoLink) => not_hidden(NO_LINK_TO_HIDE),
        HidingWarning::NotHidden(HideError::Unidentified(error)) => {
            not_hidden(&cannot_identify_for_hiding(&identify_error(*error)))
        }
        HidingWarning::NotHidden(HideError::Failed(error)) => not_hidden(&hiding_error(error)),
        HidingWarning::Inverted => HIDHIDE_INVERTED.into(),
    }
}

pub fn hidhide_error(error: &HidHideError) -> String {
    match error {
        HidHideError::NotInstalled => {
            "HidHide is not installed or its driver is not running".into()
        }
        HidHideError::InUse => {
            "HidHide is busy: close the HidHide Configuration Client or HidHideCLI".into()
        }
        HidHideError::UnexpectedSize(bytes) => {
            format!("HidHide failed: HidHide returned {bytes} bytes for a BOOLEAN")
        }
        HidHideError::TooLarge => "HidHide failed: buffer too large".into(),
        HidHideError::Failed(error) => format!("HidHide failed: {error}"),
    }
}

pub fn hiding_error(error: &HidingError) -> String {
    match error {
        HidingError::HidHide(error) => hidhide_error(error),
        HidingError::Journal(error) => {
            format!(
                "cannot use heka's HidHide journal: {}",
                journal_error(error)
            )
        }
    }
}

fn journal_error(error: &JournalError) -> String {
    match error {
        JournalError::Io(error) => error.clone(),
        JournalError::UnexpectedLine { path, line } => {
            format!("{}: unexpected line {line:?}", path.display())
        }
    }
}

fn hiding_unavailable(error: &HidingUnavailable) -> String {
    match error {
        HidingUnavailable::NoLocalAppData => "LOCALAPPDATA is not set".into(),
        HidingUnavailable::NoImageName(error) => error.clone(),
    }
}

fn identify_error(error: IdentifyError) -> String {
    match error {
        IdentifyError::Status(status) => {
            format!("cannot read the device instance ID (CONFIGRET {status})")
        }
        IdentifyError::NotText => "the device instance ID is not text".into(),
    }
}

pub fn bridge_error(error: &BridgeError) -> String {
    match error {
        BridgeError::NoBus => "ViGEmBus is not connected".into(),
        BridgeError::NoThread(error) => format!("cannot start a thread: {error}"),
        BridgeError::VirtualController(error) => error.clone(),
    }
}

pub fn report_error(error: ReportError) -> String {
    match error {
        ReportError::TooShort { len } => format!("report too short ({len} bytes)"),
        ReportError::UnexpectedId { id } => format!("unexpected report ID {id:#04x}"),
        ReportError::BadCrc => "report CRC does not match".into(),
    }
}

pub fn open_error(error: &OpenError) -> String {
    match error {
        OpenError::Hid(error) => error.clone(),
        OpenError::Address(error) => {
            format!("cannot read the MAC address: {}", report_error(*error))
        }
        OpenError::SerialNumber => "the serial number is not a MAC address".into(),
        OpenError::NotAController => "no Controller is there".into(),
    }
}

pub fn output_type(output_type: OutputType) -> &'static str {
    match output_type {
        OutputType::Xbox => XBOX,
        OutputType::Ps4 => PS4,
    }
}

fn sentence(words: &str) -> String {
    format!("{}.", words.trim_end_matches('.'))
}

pub fn cannot_open(unopened: &Unopened) -> String {
    format!(
        "A {} was found but cannot be opened: {}",
        controller(unopened.model),
        open_error(&unopened.error)
    )
}

pub fn not_released(error: &str) -> String {
    format!("Could not release a Virtual Controller: {error}")
}

pub fn not_unplugged(error: &str) -> String {
    format!(
        "Could not remove a Virtual Controller, so games may see it until heka quits: {}",
        sentence(error)
    )
}

pub fn cannot_draw(error: &str) -> String {
    format!("cannot draw a Controller: {error}")
}

pub mod log {
    use std::path::Path;

    use heka_core::hiding::{HidingChange, HidingError};
    use heka_core::lifecycle::{Change, Removal};
    use heka_core::link::{ControllerModel, LinkKind, LinkProblem};
    use heka_core::mac::MacAddress;
    use heka_core::setup::{Driver, Setup};
    use heka_core::supervisor::BridgeError;

    pub const QUITTING: &str = "heka is quitting";
    pub const LOOKING_AGAIN: &str = "Looking for Controllers again";
    pub const TRIMMED: &str = "Older entries were left out to keep the log small.";
    pub const NO_APPDATA: &str = "APPDATA is not set; settings are not saved";

    pub fn started(version: &str, in_tray: bool) -> String {
        if in_tray {
            format!("heka {version} started in the tray")
        } else {
            format!("heka {version} started")
        }
    }

    pub fn panicked(thread: Option<&str>, panic: &str) -> String {
        let thread = thread.unwrap_or("without a name");
        format!("Panic on thread {thread}: {panic}")
    }

    pub fn repeated(message: &str, left_out: u32) -> String {
        format!("{message} (left out {left_out} repeats)")
    }

    pub fn cannot_write(error: &str) -> String {
        format!("cannot write the log: {error}")
    }

    pub fn drivers(setup: &Setup) -> Vec<String> {
        let missing = setup.missing();
        [Driver::ViGEmBus, Driver::HidHide]
            .into_iter()
            .map(|driver| match missing.iter().find(|m| m.driver == driver) {
                Some(missing) => super::setup::heading(missing),
                None => format!("{} found", super::setup::name(driver)),
            })
            .collect()
    }

    pub fn change(change: &Change) -> String {
        match *change {
            Change::Bridged { mac, output_type } => {
                let output_type = super::output_type(output_type);
                format!("{mac}: Bridged to a Virtual Controller ({output_type})")
            }
            Change::Held { mac } => format!("{mac}: Held, every Link is lost"),
            Change::Returned { mac } => format!("{mac}: returned while Held"),
            Change::Removed { mac, why } => {
                let why = match why {
                    Removal::Stopped => "Stopped",
                    Removal::Expired => "the Held time is over",
                    Removal::Failed => "the Bridge failed",
                    Removal::OutputTypeChanged => "the Output Type changed",
                    Removal::Quit => "heka is quitting",
                };
                format!("{mac}: Virtual Controller removed, {why}")
            }
        }
    }

    pub fn link_found(mac: MacAddress, model: ControllerModel, link: LinkKind) -> String {
        let model = super::controller(model);
        let link = super::link(Some(link));
        format!("{mac}: {model} found over {link}")
    }

    pub fn link_gone(mac: MacAddress) -> String {
        format!("{mac}: a Link is gone")
    }

    pub fn link_problem(mac: MacAddress, problem: &LinkProblem) -> String {
        let problem = match problem {
            LinkProblem::NoFeedback(error) => {
                format!("no Feedback for this Controller: {error}")
            }
            LinkProblem::CalibrationUnread(None) => {
                "no motion calibration for this Controller".into()
            }
            LinkProblem::CalibrationUnread(Some(error)) => {
                format!("no motion calibration for this Controller: {error}")
            }
            LinkProblem::CalibrationInvalid(error) => format!(
                "no motion calibration for this Controller: {}",
                super::report_error(*error)
            ),
        };
        format!("{mac}: {problem}")
    }

    pub fn bridge_error(mac: MacAddress, error: &BridgeError) -> String {
        format!(
            "{mac}: {}",
            super::bridge_failed(&super::bridge_error(error))
        )
    }

    pub fn hiding_change(change: &HidingChange) -> String {
        match change {
            HidingChange::Allowed(app) => format!("HidHide: allowed {app}"),
            HidingChange::Hidden(device) => format!("HidHide: Hid {device}"),
            HidingChange::CloakOn => "HidHide: turned hiding on".into(),
            HidingChange::CloakOff => "HidHide: turned hiding off".into(),
            HidingChange::Restored(device) => format!("HidHide: restored {device}"),
            HidingChange::Disallowed(app) => {
                format!("HidHide: removed {app} from the allowed applications")
            }
        }
    }

    pub fn no_longer(warning: &str) -> String {
        format!("No longer: {warning}")
    }

    pub fn cannot_restore_on_quit(error: &HidingError) -> String {
        format!("could not restore HidHide: {}", super::hiding_error(error))
    }

    pub fn no_virtual_feedback(error: &str) -> String {
        format!("no Feedback from the Virtual Controller: {error}")
    }

    pub fn cannot_read_settings(path: &Path, error: &str) -> String {
        format!("cannot read {}: {error}", path.display())
    }

    pub fn cannot_save_settings(path: &Path, error: &str) -> String {
        format!("cannot save {}: {error}", path.display())
    }

    pub fn cannot_check_for_running_copy(error: &str) -> String {
        format!("cannot check for a running copy: {error}")
    }

    pub fn cannot_add_tray_icon(error: &str) -> String {
        format!("cannot add the tray icon: {error}")
    }

    pub fn cannot_update_autostart(error: &str) -> String {
        format!("cannot update Start with Windows: {error}")
    }

    pub fn cannot_change_autostart(error: &str) -> String {
        format!("cannot change Start with Windows: {error}")
    }
}

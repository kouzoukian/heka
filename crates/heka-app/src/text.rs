use heka_core::desktop::{Chord, Key, MouseButton, WheelStep};
use heka_core::discovery::{OpenError, Unopened};
use heka_core::drift::DriftError;
use heka_core::gyro::{GyroActivation, GyroAxis, GyroUse};
use heka_core::hider::{HideError, HidingWarning};
use heka_core::hiding::{
    HidHideError, HidingError, HidingUnavailable, IdentifyError, JournalError,
};
use heka_core::idle::IdleTime;
use heka_core::lifecycle::MAX_BRIDGES;
use heka_core::link::{ControllerModel, LinkKind, ReportError};
use heka_core::macros::{Mode, Target};
use heka_core::profile::{Binding, Input, Output, Profile, ProfileError};
use heka_core::radio::TurnOffError;
use heka_core::release::Version;
use heka_core::state::{Battery, Charge};
use heka_core::stick_use::{Direction, StickUse};
use heka_core::supervisor::{BridgeError, DriftStatus};
use heka_core::tuning::Curve;
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
pub const TURN_OFF: &str = "Turn off";
pub const USB: &str = "USB";
pub const BLUETOOTH: &str = "Bluetooth";
pub const NO_LINK: &str = "No Link";
pub const BATTERY_UNKNOWN: &str = "Battery unknown";
pub const BATTERY_NOT_CHARGING: &str = "Battery not charging";
pub const OUTPUT_TYPE: &str = "Output Type";
pub const XBOX: &str = "Xbox";
pub const PS4: &str = "PS4";
pub const HELD_HINT: &str = "Reconnect the Controller to keep its Virtual Controller.";

pub const CONTROLLER_SETTINGS: &str = "Controller settings";
pub const ROW_NAME: &str = "Name";
pub const GROUP_RUMBLE: &str = "Rumble";
pub const ROW_STRENGTH: &str = "Strength";
pub const ROW_SWAP_MOTORS: &str = "Swap motors";
pub const ROW_RUMBLE_TEST: &str = "Test";
pub const LARGE_MOTOR: &str = "Large motor";
pub const SMALL_MOTOR: &str = "Small motor";
pub const GROUP_LIGHTS: &str = "Lights";
pub const ROW_LIGHTBAR: &str = "Light bar";
pub const ROW_PLAYER_LIGHTS: &str = "Player lights";
pub const ROW_HIDE: &str = "Hide from games";
pub const GROUP_POWER: &str = "Power";
pub const ROW_IDLE: &str = "Turn off when idle";
pub const IDLE_TIP: &str = "Bluetooth only. Motion alone does not count as use.";
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
pub const STRENGTH_TIP: &str = "How strongly the Controller rumbles when a game asks it to.";
pub const SWAP_MOTORS_TIP: &str = "Exchanges the large and small motor of a game's rumble.";
pub const RUMBLE_TEST_TIP: &str = "Runs a motor for a second at the chosen strength.";
pub const LIGHTBAR_TIP: &str =
    "The colour while no game sets one. Default leaves the light bar to the Controller.";
pub const PLAYER_LIGHTS_TIP: &str = "The white lights under the touchpad.";
pub const HIDE_TIP: &str = "Off lets games see the Controller too, which can cause double input. Restart a running game after a change.";
pub const SETTINGS_NOTE: &str = "Rumble and lights apply while the Controller is Bridged.";

pub fn idle_time(time: IdleTime) -> String {
    match time {
        IdleTime::Thirty => format!("{} min", time.minutes()),
        _ => time.minutes().to_string(),
    }
}

pub fn named(model: ControllerModel, short_mac: &str) -> String {
    format!("{} {short_mac}", controller(model))
}

pub fn cannot_turn_off(controller: &str, error: &TurnOffError) -> String {
    format!("Could not turn off {controller}: {}", turn_off_error(error))
}

pub fn turn_off_error(error: &TurnOffError) -> String {
    match error {
        TurnOffError::NoRadio => "no Bluetooth radio was found".into(),
        TurnOffError::Failed(error) => error.clone(),
    }
}

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
pub const FN: &str = "Fn";

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

pub const APP_SETTINGS: &str = "Settings";
pub const APP_SETTINGS_TIP: &str = "Settings (Ctrl+,)";
pub const START_WITH_WINDOWS_TIP: &str = "The same choice as the tray menu item.";
pub const ROW_START_IN_TRAY: &str = "Start in the tray";
pub const START_IN_TRAY_TIP: &str = "Also when started by hand.";
pub const ROW_CLOSING: &str = "Closing the window";
pub const CLOSING_TIP: &str = "Keep running leaves heka in the tray.";
pub const KEEP_RUNNING: &str = "Keep running";
pub const THEME_TIP: &str = "heka does not follow the Windows colour mode.";
pub const DARK: &str = "Dark";
pub const LIGHT: &str = "Light";
pub const UPDATES: &str = "Updates";
pub const ROW_CHECK_FOR_RELEASES: &str = "Check for new releases";
pub const CHECK_FOR_RELEASES_TIP: &str = "At start and once a day. heka downloads nothing.";
pub const RELEASE_NOTES: &str = "Release notes";
pub const FILES: &str = "Files";
pub const ROW_SETTINGS_FOLDER: &str = "Settings folder";
pub const OPEN_FOLDER: &str = "Open folder";

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

pub fn new_release(version: Version) -> String {
    format!("heka {version} is available.")
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

pub const LOG: &str = "Log";
pub const COPY_LOG: &str = "Copy log";
pub const OPEN_LOG_FOLDER: &str = "Open log folder";
pub const NOTHING_LOGGED: &str = "Nothing logged yet";

pub const ABOUT: &str = "About heka";
pub const TAGLINE: &str = "Lets games treat a PlayStation controller as an Xbox or PS4 controller.";
pub const ROW_VERSION: &str = "Version";
pub const ROW_LICENSE: &str = "License";
pub const LICENSE_MIT: &str = "MIT";
pub const LICENSE_OR: &str = "or";
pub const LICENSE_APACHE: &str = "Apache-2.0";
pub const ROW_THEME: &str = "Theme";
pub const THEME_CREDIT: &str = "Min Theme, MIT";
pub const ROW_FONT: &str = "Font";
pub const FONT_CREDIT: &str = "Google Sans Code, OFL 1.1";
pub const ROW_NOTICES: &str = "Notices";
pub const NOTICES_FILE: &str = "THIRD-PARTY-NOTICES.txt";
pub const NOTICES_TIP: &str =
    "The licenses of everything built into heka.exe, in the file beside it.";
pub const ROW_SOURCE: &str = "Source";

pub const PROFILE: &str = "Profile";
pub const GROUP_PROFILES: &str = "Profiles";
pub const GROUP_BINDINGS: &str = "Bindings";
pub const DEFAULT_PROFILE: &str = "Default";
pub const NEW_PROFILE: &str = "New Profile";
pub const DEFAULT_PROFILE_FIXED: &str = "The Default Profile cannot be changed. Choose another Profile, or make a new one, to remap buttons.";
pub const ROW_PROFILE_OUTPUT: &str = "Output Type";
pub const PROFILE_OUTPUT_TIP: &str = "The Output Type while this Profile is chosen";
pub const OUTPUT_OWN: &str = "Controller's own";
pub const OUTPUT_SET_BY_PROFILE: &str = "Set by the Profile";
pub const BINDING_NOTHING: &str = "Nothing";
pub const SECTION_PROFILES: &str = "Profiles";
pub const SECTION_TOUCHPAD: &str = "Touchpad";
pub const SECTION_GYRO: &str = "Gyro";
pub const ROW_GYRO_USE: &str = "Gyro moves";
pub const GYRO_USE_TIP: &str = "What turning the Controller moves";
pub const ROW_ACTIVE: &str = "Active";
pub const ACTIVE_TIP: &str = "When the gyro moves it";
pub const ROW_GYRO_BUTTON: &str = "Button";
pub const GYRO_BUTTON_TIP: &str =
    "The button held or toggled to use the gyro; it keeps its Binding";
pub const ROW_AXIS: &str = "Horizontal from";
pub const AXIS_TIP: &str = "Turning the Controller (yaw) or tilting it sideways (roll)";
pub const ROW_SENSITIVITY_X: &str = "Sensitivity X";
pub const SENSITIVITY_X_TIP: &str = "How far it moves sideways";
pub const ROW_SENSITIVITY_Y: &str = "Sensitivity Y";
pub const SENSITIVITY_Y_TIP: &str = "How far it moves up and down";
pub const GYRO_DEAD_ZONE_TIP: &str = "Turning slower than this, in degrees a second, does nothing";
pub const ROW_DRIFT: &str = "Drift";
pub const DRIFT_TIP: &str = "What this Controller reads as turning while at rest";
pub const MEASURE: &str = "Measure";
pub const MEASURING: &str = "Measuring";
pub const GYRO_DEFAULT: &str =
    "The Default Profile does not use the gyro. Choose another Profile, or make a new one.";

pub fn gyro_use_name(used_as: GyroUse) -> &'static str {
    match used_as {
        GyroUse::Off => "Off",
        GyroUse::Mouse => "Mouse",
        GyroUse::RightStick => "Right stick",
    }
}

pub fn activation_name(activation: GyroActivation) -> &'static str {
    match activation {
        GyroActivation::Always => "Always",
        GyroActivation::WhileHeld => "While held",
        GyroActivation::Toggle => "Toggle",
    }
}

pub fn axis_name(axis: GyroAxis) -> &'static str {
    match axis {
        GyroAxis::Yaw => "Yaw",
        GyroAxis::Roll => "Roll",
    }
}

pub fn degrees_a_second(degrees: u16) -> String {
    format!("{degrees}\u{b0}/s")
}

pub fn drift_note(status: DriftStatus) -> &'static str {
    match status {
        DriftStatus::Unmeasured => {
            "Put the Controller at rest, then measure. heka removes the drift."
        }
        DriftStatus::Measuring => "Keep the Controller still on a flat surface.",
        DriftStatus::Measured => "Drift measured. heka removes it from this Controller.",
        DriftStatus::Failed(DriftError::Moved) => {
            "The Controller moved. Put it down and measure again."
        }
        DriftStatus::Failed(DriftError::NoInput) => {
            "The Controller stopped sending input. Measure again."
        }
    }
}
pub const ROW_TOUCHPAD_USE: &str = "Touchpad goes to";
pub const TOUCHPAD_USE_TIP: &str = "The Virtual Controller or the mouse, not both";
pub const TOUCHPAD_CONTROLLER: &str = "Controller";
pub const TOUCHPAD_MOUSE: &str = "Mouse";
pub const TOUCHPAD_SENSITIVITY_TIP: &str = "How far the pointer moves for a finger's travel";
pub const ROW_INVERT_VERTICAL: &str = "Invert vertical";
pub const INVERT_VERTICAL_TIP: &str = "A finger moving up moves the pointer down";
pub const ROW_TAP_CLICK: &str = "Tap to left click";
pub const TAP_CLICK_TIP: &str = "A short touch clicks the left button";
pub const ROW_TWO_FINGER_CLICK: &str = "Two-finger right click";
pub const TWO_FINGER_CLICK_TIP: &str = "A two-finger tap clicks the right button";
pub const TOUCHPAD_SCROLL_NOTE: &str = "Drag two fingers to scroll.";
pub const TOUCHPAD_CLICK_NOTE: &str =
    "Touchpad click Bindings are under Buttons and work either way.";
pub const TOUCHPAD_DEFAULT: &str =
    "The Default Profile does not change the touchpad. Choose another Profile, or make a new one.";
pub const SECTION_BUTTONS: &str = "Buttons";
pub const SECTION_STICKS: &str = "Sticks";
pub const SIDE_LEFT: &str = "Left stick";
pub const SIDE_RIGHT: &str = "Right stick";
pub const ROW_LEFT_STICK_USE: &str = "Left stick as";
pub const ROW_RIGHT_STICK_USE: &str = "Right stick as";
pub const STICK_USE_TIP: &str = "What this stick drives";
pub const STICK_RESTS: &str = "The Virtual Controller's stick rests at the centre.";
pub const ROW_SPEED: &str = "Speed";
pub const SPEED_TIP: &str = "How fast the pointer moves, or the wheel turns, at full travel";
pub const POINTING_DEAD_ZONE_TIP: &str =
    "Travel from the centre that does nothing, never less than 6%";
pub const KEY_DEAD_ZONE_TIP: &str = "Travel from the centre that does nothing, never less than 25%";
pub const STICK_KEY_TIP: &str = "The key held while the stick points this way";

pub fn stick_use_name(used_as: StickUse) -> &'static str {
    match used_as {
        StickUse::Stick => "Stick",
        StickUse::Mouse => "Mouse",
        StickUse::Scroll => "Scroll",
        StickUse::Wasd => "WASD",
        StickUse::Arrows => "Arrows",
        StickUse::Keys => "Four keys",
    }
}

pub fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "Up",
        Direction::Left => "Left",
        Direction::Down => "Down",
        Direction::Right => "Right",
    }
}
pub const ROW_DEAD_ZONE: &str = "Dead zone";
pub const DEAD_ZONE_TIP: &str = "Travel from the centre that does nothing";
pub const ROW_ANTI_DEAD_ZONE: &str = "Anti-dead zone";
pub const ANTI_DEAD_ZONE_TIP: &str =
    "Where the smallest movement starts, for games with a dead zone of their own";
pub const ROW_MAX_ZONE: &str = "Max zone";
pub const MAX_ZONE_TIP: &str = "Travel at which the stick counts as fully pushed";
pub const ROW_SENSITIVITY: &str = "Sensitivity";
pub const SENSITIVITY_TIP: &str = "How far the stick's travel is stretched";
pub const ROW_CURVE: &str = "Curve";
pub const CURVE_TIP: &str = "How the travel is shaped between the centre and the edge";
pub const ROW_INVERT_X: &str = "Invert X";
pub const ROW_INVERT_Y: &str = "Invert Y";
pub const ROW_SWAP_STICKS: &str = "Swap sticks";
pub const SWAP_STICKS_TIP: &str = "Exchanges the left and right stick";
pub const GROUP_SHORTCUTS: &str = "Shortcuts";
pub const ROW_SHORTCUTS: &str = "Controller shortcuts";
pub const SHORTCUTS_NOTE: &str = "Hold PS and press a button. PS on its own works as before.";
pub const SHORTCUTS_TURN_OFF_NOTE: &str =
    "Turning off needs a two-second hold, for a Controller on Bluetooth.";
pub const SHORTCUTS_HIDDEN_NOTE: &str =
    "The other button does not reach the game while a shortcut is recognised.";
pub const ROW_NEXT_PROFILE: &str = "Next Profile";
pub const ROW_PREVIOUS_PROFILE: &str = "Previous Profile";
pub const ROW_SHORTCUT_TURN_OFF: &str = "Turn off";
pub const NO_BUTTON: &str = "No button";

pub fn shortcut_choice(input: Option<Input>) -> String {
    match input {
        Some(input) => format!("PS + {}", input_name(input)),
        None => NO_BUTTON.to_owned(),
    }
}

pub fn shortcut_profile(controller: &str, profile: Option<&str>, rule: Option<&str>) -> String {
    let profile = profile.unwrap_or(DEFAULT_PROFILE);
    match rule {
        Some(program) => format!(
            "Profile for {controller}: {profile} (a rule applies: {program}; it returns afterwards)"
        ),
        None => format!("Profile for {controller}: {profile}"),
    }
}

pub const GROUP_GAMES: &str = "Game rules";
pub const GAMES_NOTE: &str =
    "A program in front applies its Profile to every Controller. Leaving it restores their own.";
pub const GAMES_ADMIN_NOTE: &str = "Rules also work for programs run as administrator in most cases. heka says when it cannot see the program in front.";
pub const FOREGROUND_UNREADABLE: &str = "heka cannot see the program in front, so its rule cannot apply. This happens with some protected programs.";
pub const NO_PROFILES_FOR_RULE: &str = "Make a Profile first: a rule needs one.";
pub const ADD_RULE: &str = "Add rule";
pub const CHOOSE_FILE: &str = "Choose a file";
pub const RUNNING_PROGRAMS: &str = "Running programs";
pub const NO_RUNNING_PROGRAMS: &str = "No program with a window was found.";
pub const RULE_APPLIES_NOW: &str = "Applies now";
pub const RULE_PROFILE_MISSING: &str = "Profile missing";
pub const DELETE_RULE: &str = "Delete rule";
pub const RULE_PROFILE_TIP: &str = "The Profile this program applies";
pub const CHOOSE_PROGRAM_PROMPT: &str = "Choose the program";

pub fn rule_applies(program: &str, profile: &str, own: &str) -> String {
    format!("A rule applies: {program} uses {profile}. Your choice, {own}, returns afterwards.")
}

pub const SECTION_TRIGGERS: &str = "Triggers";
pub const SIDE_LEFT_TRIGGER: &str = "Left trigger";
pub const SIDE_RIGHT_TRIGGER: &str = "Right trigger";
pub const ROW_HAIR_TRIGGER: &str = "Hair trigger";
pub const HAIR_TRIGGER_TIP: &str = "A press past this point reads as fully pressed";
pub const ROW_PRESS_POINT: &str = "Button press at";
pub const PRESS_POINT_TIP: &str =
    "Where this trigger counts as pressed when it is bound to a button";
pub const PRESS_CLICK: &str = "Click";
pub const TRIGGERS_DEFAULT: &str =
    "The Default Profile does not tune triggers. Choose another Profile, or make a new one.";

pub fn travel_percent(travel: u8) -> String {
    format!("{}%", (u32::from(travel) * 100 + 127) / 255)
}

pub const READOUT_RAW: &str = "Raw";
pub const READOUT_TUNED: &str = "Tuned";
pub const STICKS_DEFAULT: &str =
    "The Default Profile does not tune sticks. Choose another Profile, or make a new one.";

pub fn curve_name(curve: Curve) -> &'static str {
    match curve {
        Curve::Linear => "Linear",
        Curve::Soft => "Soft",
        Curve::Softer => "Softer",
        Curve::Sharp => "Sharp",
        Curve::Smooth => "Smooth",
    }
}

pub fn percent_of(value: u16) -> String {
    format!("{value}%")
}

pub fn report_rate(rate: Option<u32>) -> String {
    match rate {
        Some(rate) => format!("{rate} reports a second"),
        None => "Report rate not known yet".to_owned(),
    }
}

pub const DUPLICATE: &str = "Duplicate";
pub const RENAME: &str = "Rename";
pub const DELETE: &str = "Delete";
pub const IMPORT: &str = "Import";
pub const EXPORT: &str = "Export";
pub const CANCEL: &str = "Cancel";
pub const PROFILE_NAME: &str = "Profile name";
pub const IMPORT_PROMPT: &str = "Import a Profile";

pub fn delete_profile_question(name: &str, users: usize) -> String {
    match users {
        0 => format!("Delete {name}? No Controller uses it."),
        1 => format!("Delete {name}? 1 Controller goes back to the Default Profile."),
        users => {
            format!("Delete {name}? {users} Controllers go back to the Default Profile.")
        }
    }
}

pub const BINDING_TIP: &str = "What this input drives";
pub const KIND_BUTTON: &str = "Button";
pub const KIND_KEY: &str = "Key";
pub const KIND_MOUSE: &str = "Mouse";
pub const KIND_MACRO: &str = "Macro";
pub const ROW_PRESS_MODE: &str = "Press";
pub const PRESS_MODE_TIP: &str =
    "Hold: while held. Toggle: one press holds, the next releases. Turbo: repeats while held.";
pub const ROW_TURBO_RATE: &str = "Repeats a second";
pub const TURBO_RATE_TIP: &str = "How often turbo presses it while held";
pub const ROW_RUNS: &str = "Runs";
pub const RUNS_TIP: &str = "Letting the input go cancels the macro either way";
pub const RUNS_ONCE: &str = "Once per press";
pub const RUNS_HELD: &str = "While held";
pub const GROUP_STEPS: &str = "Steps";
pub const STEP_PRESS: &str = "Press";
pub const STEP_RELEASE: &str = "Release";
pub const STEP_WAIT: &str = "Wait";
pub const ROW_DELAY: &str = "Delay in ms";
pub const DELAY_TIP: &str = "How long the macro waits here";
pub const ROW_SENDS: &str = "Sends";
pub const SENDS_TIP: &str = "The key or mouse button this step presses or releases";
pub const STEP_KEY_TIP: &str = "The key or combination of this step";
pub const STEP_MOUSE_TIP: &str = "The mouse button of this step";
pub const ADD_STEP: &str = "Add step";
pub const MOVE_UP: &str = "Move up";
pub const MOVE_DOWN: &str = "Move down";
pub const DELETE_STEP: &str = "Delete step";

pub fn step(number: usize) -> String {
    format!("{number}.")
}

pub fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Hold => "Hold",
        Mode::Toggle => "Toggle",
        Mode::Turbo(_) => "Turbo",
    }
}

pub fn mouse_button_short(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Forward => FORWARD_SHORT,
        other => mouse_button_name(other),
    }
}

pub fn target_words(target: Target, label: &dyn Fn(Key) -> Option<String>) -> String {
    match target {
        Target::Keys(chord) => binding_words(Binding::Keys(chord), label),
        Target::Mouse(button) => binding_words(Binding::Mouse(button), label),
    }
}

pub fn input_words(
    profile: &Profile,
    input: Input,
    label: &dyn Fn(Key) -> Option<String>,
) -> String {
    let words = binding_words(profile.binding(input), label);
    if let Some(found) = profile.macro_of(input) {
        return match found.steps().len() {
            1 => format!("{words}, 1 step"),
            steps => format!("{words}, {steps} steps"),
        };
    }
    match profile.mode(input) {
        Mode::Hold => words,
        mode => format!("{words}, {}", mode_name(mode).to_lowercase()),
    }
}
pub const ROW_KEYS: &str = "Keys";
pub const CAPTURE_KEY: &str = "Capture a key";
pub const CAPTURING_KEY: &str = "Press a key, Esc cancels";
pub const CAPTURE_TIP: &str = "Capture a key or combination by pressing it";
pub const KEYS_TIP: &str = "The key or combination held while this input is";
pub const MOUSE_BUTTON_TIP: &str = "The mouse button held while this input is";
pub const WHEEL_TIP: &str = "Turns the wheel one step for each press";
pub const BIND_ESCAPE: &str = "Bind Esc";
pub const ROW_MOUSE_BUTTON: &str = "Button";
pub const ROW_WHEEL: &str = "Wheel";
pub const FORWARD_SHORT: &str = "Fwd";
pub const DESKTOP_NOTE: &str = "Programs run as administrator ignore keys and clicks unless heka is too. Anti-cheat may reject them.";

pub fn mouse_button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "Left",
        MouseButton::Right => "Right",
        MouseButton::Middle => "Middle",
        MouseButton::Back => "Back",
        MouseButton::Forward => "Forward",
    }
}

pub fn wheel_name(step: WheelStep) -> &'static str {
    match step {
        WheelStep::Up => "Up",
        WheelStep::Down => "Down",
    }
}

pub fn key_name(key: Key, label: &dyn Fn(Key) -> Option<String>) -> String {
    if let Some(name) = named_key(key) {
        return name.to_owned();
    }
    label(key)
        .or_else(|| us_character(key).map(String::from))
        .unwrap_or_else(|| format!("{:04X}", key.scan()))
}

pub fn chord_words(chord: Chord, label: &dyn Fn(Key) -> Option<String>) -> String {
    chord
        .modifiers()
        .map(|modifier| modifier_name(modifier).to_owned())
        .chain([key_name(chord.key(), label)])
        .collect::<Vec<_>>()
        .join(" + ")
}

fn modifier_name(key: Key) -> &'static str {
    match key {
        Key::LEFT_CTRL => "Ctrl",
        Key::LEFT_ALT => "Alt",
        Key::LEFT_SHIFT => "Shift",
        Key::LEFT_WINDOWS => "Win",
        other => named_key(other).expect("every modifier has a name"),
    }
}

fn named_key(key: Key) -> Option<&'static str> {
    const FUNCTION: [&str; 10] = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10"];
    const MORE_FUNCTION: [&str; 11] = [
        "F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23",
    ];
    let name = match key.scan() {
        0x01 => "Esc",
        0x0E => "Backspace",
        0x0F => "Tab",
        0x1C => "Enter",
        0x1D => "Left Ctrl",
        0x2A => "Left Shift",
        0x36 => "Right Shift",
        0x37 => "Num *",
        0x38 => "Left Alt",
        0x39 => "Space",
        0x3A => "Caps Lock",
        scan @ 0x3B..=0x44 => FUNCTION[usize::from(scan - 0x3B)],
        0x45 => "Pause",
        0x46 => "Scroll Lock",
        0x47 => "Num 7",
        0x48 => "Num 8",
        0x49 => "Num 9",
        0x4A => "Num -",
        0x4B => "Num 4",
        0x4C => "Num 5",
        0x4D => "Num 6",
        0x4E => "Num +",
        0x4F => "Num 1",
        0x50 => "Num 2",
        0x51 => "Num 3",
        0x52 => "Num 0",
        0x53 => "Num .",
        0x54 | 0xE037 => "Print Screen",
        0x57 => "F11",
        0x58 => "F12",
        scan @ 0x64..=0x6E => MORE_FUNCTION[usize::from(scan - 0x64)],
        0x76 => "F24",
        0xE01C => "Num Enter",
        0xE01D => "Right Ctrl",
        0xE035 => "Num /",
        0xE038 => "Right Alt",
        0xE045 => "Num Lock",
        0xE047 => "Home",
        0xE048 => "Up",
        0xE049 => "Page Up",
        0xE04B => "Left",
        0xE04D => "Right",
        0xE04F => "End",
        0xE050 => "Down",
        0xE051 => "Page Down",
        0xE052 => "Insert",
        0xE053 => "Delete",
        0xE05B => "Left Windows",
        0xE05C => "Right Windows",
        0xE05D => "Menu",
        _ => return None,
    };
    Some(name)
}

fn us_character(key: Key) -> Option<char> {
    let rows: [(u16, &str); 6] = [
        (0x02, "1234567890-="),
        (0x10, "QWERTYUIOP[]"),
        (0x1E, "ASDFGHJKL;'`"),
        (0x2B, "\\ZXCVBNM,./"),
        (0x56, "\\"),
        (0x73, "/"),
    ];
    rows.into_iter().find_map(|(first, characters)| {
        let place = key.scan().checked_sub(first)?;
        characters.chars().nth(usize::from(place))
    })
}

pub fn input_name(input: Input) -> &'static str {
    match input {
        Input::Cross => "Cross",
        Input::Circle => "Circle",
        Input::Square => "Square",
        Input::Triangle => "Triangle",
        Input::DpadUp => "Up",
        Input::DpadDown => "Down",
        Input::DpadLeft => "Left",
        Input::DpadRight => "Right",
        Input::L1 => "L1",
        Input::R1 => "R1",
        Input::L2 => "L2",
        Input::R2 => "R2",
        Input::L3 => "L3",
        Input::R3 => "R3",
        Input::Create => "Create",
        Input::Options => "Options",
        Input::Ps => "PS",
        Input::Touchpad => "Touchpad",
        Input::TouchpadLeft => "Touchpad, left half",
        Input::TouchpadRight => "Touchpad, right half",
        Input::Mute => "Mute",
        Input::LeftFn => "Left Fn",
        Input::RightFn => "Right Fn",
        Input::LeftBack => "Left back",
        Input::RightBack => "Right back",
    }
}

pub fn output_name(output: Output) -> &'static str {
    match output {
        Output::Cross => "Cross",
        Output::Circle => "Circle",
        Output::Square => "Square",
        Output::Triangle => "Triangle",
        Output::DpadUp => "Up",
        Output::DpadDown => "Down",
        Output::DpadLeft => "Left",
        Output::DpadRight => "Right",
        Output::L1 => "L1",
        Output::R1 => "R1",
        Output::L3 => "L3",
        Output::R3 => "R3",
        Output::Create => "Create",
        Output::Options => "Options",
        Output::Ps => "PS",
        Output::LeftTrigger => "L2, pressed fully",
        Output::RightTrigger => "R2, pressed fully",
    }
}

pub fn cannot_read_import(path: &std::path::Path, error: &str) -> String {
    format!("Cannot read {}: {error}", path.display())
}

pub fn cannot_import(path: &std::path::Path, error: &ProfileError) -> String {
    let why = match error {
        ProfileError::NotJson(reason) => format!("it is not JSON ({reason})"),
        ProfileError::NotAProfile => "it is not a Profile".to_owned(),
    };
    format!(
        "Cannot import {}: {why}. Nothing was changed.",
        path.display()
    )
}

pub fn cannot_export(path: &std::path::Path, error: &str) -> String {
    format!("Cannot write {}: {error}", path.display())
}

pub fn binding_words(binding: Binding, label: &dyn Fn(Key) -> Option<String>) -> String {
    match binding {
        Binding::Default => DEFAULT.to_owned(),
        Binding::Nothing => BINDING_NOTHING.to_owned(),
        Binding::To(output) => output_name(output).to_owned(),
        Binding::Keys(chord) => format!("{KIND_KEY} {}", chord_words(chord, label)),
        Binding::Mouse(button) => {
            let button = mouse_button_name(button).to_lowercase();
            format!("{KIND_MOUSE} {button}")
        }
        Binding::Wheel(step) => format!("{ROW_WHEEL} {}", wheel_name(step).to_lowercase()),
        Binding::Macro => KIND_MACRO.to_owned(),
    }
}

pub mod log {
    use std::path::Path;

    use heka_core::drift::DriftError;
    use heka_core::hiding::{HidingChange, HidingError};
    use heka_core::lifecycle::{Change, Removal};
    use heka_core::link::{ControllerModel, LinkKind, LinkProblem};
    use heka_core::mac::MacAddress;
    use heka_core::radio::TurnOffError;
    use heka_core::release::{CheckError, Version};
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

    pub fn drift(mac: MacAddress, measurement: Result<[i32; 3], DriftError>) -> String {
        match measurement {
            Ok(_) => format!("measured the gyro drift of {mac}"),
            Err(DriftError::Moved) => {
                format!("cannot measure the gyro drift of {mac}: it moved")
            }
            Err(DriftError::NoInput) => {
                format!("cannot measure the gyro drift of {mac}: it sent no input")
            }
        }
    }

    pub fn cannot_send_input(error: &str) -> String {
        format!("cannot send keys or clicks: {error}")
    }

    pub fn cannot_listen_for_keys(error: &str) -> String {
        format!("cannot listen for keys: {error}")
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
                    Removal::TurnedOff => "the Controller was turned off",
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

    pub fn cannot_turn_off(mac: MacAddress, error: &TurnOffError) -> String {
        format!(
            "{mac}: could not turn off: {}",
            super::turn_off_error(error)
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

    pub fn release_check(found: &Result<Option<Version>, CheckError>) -> Option<String> {
        match found {
            Ok(None) => None,
            Ok(Some(newer)) => Some(format!(
                "heka {newer} is available: {}",
                newer.release_page()
            )),
            Err(error) => Some(format!(
                "cannot check for a new release: {}",
                release_error(error)
            )),
        }
    }

    fn release_error(error: &CheckError) -> String {
        match error {
            CheckError::NotFetched(error) => error.clone(),
            CheckError::Refused(status) => format!("GitHub answered with status {status}"),
            CheckError::Unreadable(error) => format!("GitHub's answer cannot be read: {error}"),
            CheckError::UnknownTag(tag) => {
                format!("the latest release's tag {tag:?} is not a version")
            }
        }
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

    pub fn settings_set_aside(path: &Path, copy: &Path) -> String {
        format!(
            "{} is not a settings file; a copy was kept as {}",
            path.display(),
            copy.display()
        )
    }

    pub fn cannot_save_settings(path: &Path, error: &str) -> String {
        format!("cannot save {}: {error}", path.display())
    }

    pub fn cannot_read_profiles(folder: &Path, error: &str) -> String {
        format!("cannot read the profiles in {}: {error}", folder.display())
    }

    pub fn cannot_read_profile(path: &Path, error: &str) -> String {
        format!("cannot read {}: {error}", path.display())
    }

    pub fn profile_skipped(path: &Path) -> String {
        format!("{} is not a profile and was left out", path.display())
    }

    pub fn cannot_remove_profile(path: &Path, error: &str) -> String {
        format!("cannot remove {}: {error}", path.display())
    }

    pub fn cannot_save_profile(path: &Path, error: &str) -> String {
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

    pub fn cannot_open_log_folder(error: &str) -> String {
        format!("cannot open the log folder: {error}")
    }

    pub fn cannot_open_settings_folder(error: &str) -> String {
        format!("cannot open the settings folder: {error}")
    }
}

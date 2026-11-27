use std::sync::Arc;

use heka_app::report::Reporter;
use heka_app::text;
use heka_core::bridge::LiveInput;
use heka_core::hider::HidingWarning;
use heka_core::hiding::{HidHideError, HidHideStatus, HidingError};
use heka_core::lifecycle::{Change, Phase, Removal};
use heka_core::link::ControllerModel;
use heka_core::mac::MacAddress;
use heka_core::preferences::Preferences;
use heka_core::radio::TurnOffError;
use heka_core::release::{CheckError, Version};
use heka_core::settings::Settings;
use heka_core::setup::{self, BusUnavailable, Drivers};
use heka_core::supervisor::{BridgeError, ControllerView, Supervisor};
use heka_core::virtual_controller::{OutputType, VirtualBus};

const MAC: MacAddress = MacAddress([0x02, 0, 0, 0, 0, 1]);

fn view(error: Option<BridgeError>) -> ControllerView {
    ControllerView {
        mac: MAC,
        model: ControllerModel::DualSense,
        phase: Phase::Waiting,
        output_type: OutputType::Xbox,
        nickname: None,
        preferences: Preferences::default(),
        active_link: None,
        has_bluetooth: false,
        idle_time: None,
        profile: None,
        applied_profile: None,
        rule_program: None,
        output_type_from_profile: false,
        live: Arc::new(LiveInput::default()),
        error,
        turn_off_error: None,
    }
}

fn refused(why: &str) -> Option<BridgeError> {
    Some(BridgeError::VirtualController(why.into()))
}

#[test]
fn a_bridge_error_is_reported_when_it_appears_or_changes() {
    let mut reporter = Reporter::default();

    assert_eq!(reporter.controllers(&[view(None)]), Vec::<String>::new());
    assert_eq!(
        reporter.controllers(&[view(refused("no free slot"))]),
        ["02:00:00:00:00:01: Could not Bridge: no free slot"]
    );
    assert!(
        reporter
            .controllers(&[view(refused("no free slot"))])
            .is_empty()
    );
    assert_eq!(
        reporter.controllers(&[view(refused("the bus went away"))]),
        ["02:00:00:00:00:01: Could not Bridge: the bus went away"]
    );
    assert!(reporter.controllers(&[view(None)]).is_empty());
    assert_eq!(
        reporter
            .controllers(&[view(refused("the bus went away"))])
            .len(),
        1
    );
}

#[test]
fn a_hiding_warning_is_reported_when_it_appears_and_when_it_goes() {
    let mut reporter = Reporter::default();
    let warning = HidingWarning::NotRestored(HidingError::HidHide(HidHideError::InUse));
    let words = text::hiding_warning(&warning);

    assert_eq!(
        reporter.hiding(std::slice::from_ref(&warning)),
        std::slice::from_ref(&words)
    );
    assert!(reporter.hiding(std::slice::from_ref(&warning)).is_empty());
    assert_eq!(reporter.hiding(&[]), [format!("No longer: {words}")]);
    assert!(reporter.hiding(&[]).is_empty());
}

#[test]
fn a_failure_to_look_for_controllers_is_reported_once_and_so_is_its_end() {
    let mut reporter = Reporter::default();

    assert_eq!(reporter.hid_error(None), None);
    assert_eq!(
        reporter.hid_error(Some("access denied")).as_deref(),
        Some("Cannot look for Controllers: access denied")
    );
    assert_eq!(reporter.hid_error(Some("access denied")), None);
    assert_eq!(
        reporter.hid_error(None).as_deref(),
        Some(text::log::LOOKING_AGAIN)
    );
}

struct NoBus;

impl Drivers for NoBus {
    fn open_vigem_bus(&self) -> Result<Box<dyn VirtualBus>, BusUnavailable> {
        Err(BusUnavailable::NotInstalled)
    }

    fn hidhide(&self) -> HidHideStatus {
        HidHideStatus::InUse
    }
}

#[test]
fn each_driver_is_logged_as_found_or_with_what_is_wrong() {
    let mut supervisor = Supervisor::without_bus(Settings::default());
    let (found, _) = setup::check(&NoBus, &mut supervisor, std::time::Instant::now());

    assert_eq!(
        text::log::drivers(&found),
        ["ViGEmBus is not installed or not running", "HidHide found"]
    );
}

#[test]
fn a_bridge_change_is_logged_with_the_controller_and_the_reason() {
    assert_eq!(
        text::log::change(&Change::Bridged {
            mac: MAC,
            output_type: OutputType::Ps4
        }),
        "02:00:00:00:00:01: Bridged to a Virtual Controller (PS4)"
    );
    assert_eq!(
        text::log::change(&Change::Removed {
            mac: MAC,
            why: Removal::Expired
        }),
        "02:00:00:00:00:01: Virtual Controller removed, the Held time is over"
    );
}

#[test]
fn a_failure_to_turn_off_is_reported_when_it_appears_or_changes() {
    let mut reporter = Reporter::default();
    let failed = |why: &str| {
        let mut failed = view(None);
        failed.turn_off_error = Some(TurnOffError::Failed(why.into()));
        failed
    };
    let words = ["02:00:00:00:00:01: could not turn off: access is denied"];

    assert_eq!(reporter.controllers(&[failed("access is denied")]), words);
    assert!(
        reporter
            .controllers(&[failed("access is denied")])
            .is_empty()
    );
    assert!(reporter.controllers(&[view(None)]).is_empty());
    assert_eq!(reporter.controllers(&[failed("access is denied")]), words);
}

#[test]
fn a_newer_release_is_logged_with_its_page_once() {
    let mut reporter = Reporter::default();
    let newer = Ok(Some(Version::parse("0.2.0").unwrap()));
    let words = "heka 0.2.0 is available: https://github.com/kouzoukian/heka/releases/tag/v0.2.0";

    assert_eq!(reporter.release_check(&Ok(None)), None);
    assert_eq!(reporter.release_check(&newer).as_deref(), Some(words));
    assert_eq!(reporter.release_check(&newer), None);
    assert_eq!(reporter.release_check(&Ok(None)), None);
    assert_eq!(reporter.release_check(&newer).as_deref(), Some(words));
}

#[test]
fn a_failed_release_check_is_logged_when_it_appears_or_changes() {
    let mut reporter = Reporter::default();
    let offline = Err(CheckError::NotFetched("WinHTTP error 12007".into()));
    let words = "cannot check for a new release: WinHTTP error 12007";

    assert_eq!(reporter.release_check(&offline).as_deref(), Some(words));
    assert_eq!(reporter.release_check(&offline), None);
    assert!(
        reporter
            .release_check(&Err(CheckError::Refused(403)))
            .is_some()
    );
    assert_eq!(reporter.release_check(&Ok(None)), None);
    assert_eq!(reporter.release_check(&offline).as_deref(), Some(words));
}

#[test]
fn every_failed_release_check_says_why() {
    for (error, why) in [
        (
            CheckError::NotFetched("WinHTTP error 12007".into()),
            "WinHTTP error 12007",
        ),
        (CheckError::Refused(404), "GitHub answered with status 404"),
        (
            CheckError::Unreadable("missing field `tag_name`".into()),
            "GitHub's answer cannot be read: missing field `tag_name`",
        ),
        (
            CheckError::UnknownTag("nightly".into()),
            "the latest release's tag \"nightly\" is not a version",
        ),
    ] {
        assert_eq!(
            Reporter::default().release_check(&Err(error)),
            Some(format!("cannot check for a new release: {why}"))
        );
    }
}

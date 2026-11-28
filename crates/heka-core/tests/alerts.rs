use std::time::{Duration, Instant};

use heka_core::alerts::{Alert, Alerts, Notification, Notifications, SETTLED_BRIDGE, Watched};
use heka_core::lifecycle::Phase;
use heka_core::mac::MacAddress;
use heka_core::state::{Battery, Charge};
use heka_core::supervisor::BridgeError;

const SOFA: MacAddress = MacAddress([0x02, 0, 0, 0, 0, 1]);
const DESK: MacAddress = MacAddress([0x02, 0, 0, 0, 0, 2]);

const EVERYTHING: Notifications = Notifications {
    low_battery: true,
    bridge_failed: true,
    connections: true,
};

fn controller(mac: MacAddress) -> Watched {
    Watched {
        mac,
        name: if mac == SOFA { "Sofa" } else { "Desk" }.to_owned(),
        phase: Phase::Bridged,
        battery: None,
        failure: None,
    }
}

fn charged(percent: u8, charge: Charge) -> Watched {
    Watched {
        battery: Some(Battery { percent, charge }),
        ..controller(SOFA)
    }
}

fn discharging(percent: u8) -> Watched {
    charged(percent, Charge::Discharging)
}

fn failing() -> Watched {
    Watched {
        phase: Phase::Waiting,
        failure: Some(BridgeError::VirtualController("refused".into())),
        ..controller(SOFA)
    }
}

fn about(mac: MacAddress, alert: Alert) -> Notification {
    Notification {
        mac,
        name: controller(mac).name,
        alert,
    }
}

struct Watching {
    alerts: Alerts,
    wanted: Notifications,
    now: Instant,
}

impl Watching {
    fn new() -> Self {
        Self::wanting(EVERYTHING)
    }

    fn wanting(wanted: Notifications) -> Self {
        Self {
            alerts: Alerts::default(),
            wanted,
            now: Instant::now(),
        }
    }

    fn sees(&mut self, controllers: &[Watched]) -> Vec<Alert> {
        self.notified(controllers)
            .into_iter()
            .map(|notification| notification.alert)
            .collect()
    }

    fn notified(&mut self, controllers: &[Watched]) -> Vec<Notification> {
        self.alerts.observe(controllers, self.wanted, self.now)
    }

    fn after(&mut self, time: Duration) -> &mut Self {
        self.now += time;
        self
    }
}

fn low(percent: u8) -> Alert {
    Alert::LowBattery { percent }
}

fn refused() -> Alert {
    Alert::BridgeFailed(BridgeError::VirtualController("refused".into()))
}

#[test]
fn a_battery_that_drops_to_twenty_percent_is_told_once() {
    let mut watching = Watching::new();

    assert_eq!(watching.sees(&[discharging(25)]), []);
    assert_eq!(watching.sees(&[discharging(20)]), [low(20)]);
    assert_eq!(watching.sees(&[discharging(20)]), []);
    assert_eq!(watching.sees(&[discharging(5)]), []);
}

#[test]
fn a_controller_that_arrives_low_is_told_of_it() {
    let mut watching = Watching::new();

    assert_eq!(
        watching.notified(&[discharging(15)]),
        [about(SOFA, low(15))]
    );
}

#[test]
fn a_level_wavering_around_the_limit_is_still_one_discharge() {
    let mut watching = Watching::new();
    watching.sees(&[discharging(15)]);

    assert_eq!(watching.sees(&[discharging(25)]), []);
    assert_eq!(watching.sees(&[discharging(15)]), []);
}

#[test]
fn charging_starts_a_new_discharge() {
    let mut watching = Watching::new();
    watching.sees(&[discharging(15)]);

    assert_eq!(watching.sees(&[charged(15, Charge::Charging)]), []);
    assert_eq!(watching.sees(&[discharging(15)]), [low(15)]);
}

#[test]
fn a_controller_charged_elsewhere_starts_a_new_discharge() {
    let mut watching = Watching::new();
    watching.sees(&[discharging(15)]);
    watching.sees(&[]);

    watching.sees(&[discharging(65)]);

    assert!(watching.sees(&[discharging(15)]).contains(&low(15)));
}

#[test]
fn a_low_controller_that_leaves_and_returns_is_not_told_again() {
    let mut watching = Watching::wanting(Notifications::default());
    watching.sees(&[discharging(15)]);

    watching.sees(&[]);

    assert_eq!(watching.sees(&[discharging(15)]), []);
}

#[test]
fn a_battery_without_a_level_says_nothing_and_forgets_nothing() {
    let mut watching = Watching::new();
    assert_eq!(watching.sees(&[charged(0, Charge::Unknown)]), []);
    assert_eq!(watching.sees(&[charged(0, Charge::NotCharging)]), []);
    watching.sees(&[discharging(15)]);

    watching.sees(&[controller(SOFA)]);
    watching.sees(&[charged(0, Charge::Unknown)]);

    assert_eq!(watching.sees(&[discharging(15)]), []);
}

#[test]
fn each_controller_has_its_own_discharge() {
    let mut watching = Watching::new();
    let other = Watched {
        mac: DESK,
        name: "Desk".into(),
        ..discharging(5)
    };

    assert_eq!(
        watching.notified(&[discharging(15), other]),
        [about(SOFA, low(15)), about(DESK, low(5))]
    );
}

#[test]
fn a_failed_bridge_is_told_once_however_often_it_is_tried_again() {
    let mut watching = Watching::new();

    assert_eq!(watching.sees(&[failing()]), [refused()]);
    assert_eq!(
        watching.after(Duration::from_secs(2)).sees(&[failing()]),
        []
    );
    watching.sees(&[controller(SOFA)]);
    assert_eq!(
        watching.after(Duration::from_secs(2)).sees(&[failing()]),
        []
    );
}

#[test]
fn a_failure_after_a_bridge_that_held_is_told_again() {
    let mut watching = Watching::new();
    watching.sees(&[failing()]);
    watching.sees(&[controller(SOFA)]);
    watching.after(SETTLED_BRIDGE).sees(&[controller(SOFA)]);

    assert_eq!(watching.sees(&[failing()]), [refused()]);
}

#[test]
fn a_failure_after_the_player_stopped_the_controller_is_told_again() {
    let mut watching = Watching::new();
    watching.sees(&[failing()]);
    let stopped = Watched {
        phase: Phase::Stopped,
        ..failing()
    };
    assert_eq!(watching.sees(&[stopped]), []);

    assert_eq!(watching.sees(&[failing()]), [refused()]);
}

#[test]
fn waiting_without_a_failure_is_not_a_failed_bridge() {
    let mut watching = Watching::new();
    let waiting = Watched {
        phase: Phase::Waiting,
        ..controller(SOFA)
    };

    assert_eq!(watching.sees(&[waiting]), []);
}

#[test]
fn controllers_already_there_at_the_start_are_not_announced() {
    let mut watching = Watching::new();

    assert_eq!(watching.sees(&[controller(SOFA)]), []);
}

#[test]
fn a_controller_that_connects_or_is_removed_is_named() {
    let mut watching = Watching::new();
    watching.sees(&[controller(SOFA)]);

    assert_eq!(
        watching.notified(&[controller(SOFA), controller(DESK)]),
        [about(DESK, Alert::Connected)]
    );
    assert_eq!(
        watching.notified(&[controller(DESK)]),
        [about(SOFA, Alert::Removed)]
    );
    assert_eq!(watching.sees(&[controller(DESK)]), []);
}

#[test]
fn a_held_controller_is_neither_removed_nor_connected_again() {
    let mut watching = Watching::new();
    watching.sees(&[controller(SOFA)]);
    let held = Watched {
        phase: Phase::Held {
            until: watching.now + Duration::from_secs(30),
        },
        ..controller(SOFA)
    };

    assert_eq!(watching.sees(&[held]), []);
    assert_eq!(watching.sees(&[controller(SOFA)]), []);
}

#[test]
fn connections_are_not_announced_until_chosen() {
    let mut watching = Watching::wanting(Notifications::default());
    watching.sees(&[]);

    assert_eq!(watching.sees(&[controller(SOFA)]), []);
    assert_eq!(watching.sees(&[]), []);
}

#[test]
fn a_kind_turned_off_is_not_told_even_after_it_is_turned_on_again() {
    let silent = Notifications {
        low_battery: false,
        bridge_failed: false,
        connections: false,
    };
    let mut watching = Watching::wanting(silent);
    assert_eq!(watching.sees(&[discharging(15)]), []);
    assert_eq!(watching.sees(&[failing()]), []);

    watching.wanted = EVERYTHING;

    assert_eq!(watching.sees(&[discharging(15)]), []);
    assert_eq!(watching.sees(&[failing()]), []);
}

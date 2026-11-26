//! Link and timer events in, Bridge state changes out. Time is a fake clock:
//! every event carries the `Instant` it happens at.

use std::time::{Duration, Instant};

use heka_core::lifecycle::{
    BRIDGE_RETRY_DELAY, Change, HOLD_TIME, Lifecycle, MAX_BRIDGES, Phase, Removal,
};
use heka_core::mac::MacAddress;
use heka_core::settings::Settings;
use heka_core::virtual_controller::OutputType;

fn mac(n: u8) -> MacAddress {
    MacAddress([0xA0, 0, 0, 0, 0, n])
}

fn bridged(n: u8) -> Change {
    Change::Bridged {
        mac: mac(n),
        output_type: OutputType::Xbox,
    }
}

fn removed(n: u8, why: Removal) -> Change {
    Change::Removed { mac: mac(n), why }
}

fn secs(s: f64) -> Duration {
    Duration::from_secs_f64(s)
}

fn phases(lifecycle: &Lifecycle) -> Vec<(MacAddress, Phase)> {
    lifecycle.controllers().collect()
}

#[test]
fn a_controller_that_appears_is_bridged_as_xbox() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());

    assert_eq!(lifecycle.link_arrived(mac(1), t0), [bridged(1)]);
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Bridged));
}

#[test]
fn four_controllers_bridge_and_a_fifth_is_listed_but_not_bridged() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    for n in 1..=4 {
        assert_eq!(lifecycle.link_arrived(mac(n), t0), [bridged(n)]);
    }
    assert_eq!(MAX_BRIDGES, 4);

    assert_eq!(lifecycle.link_arrived(mac(5), t0), []);
    assert_eq!(lifecycle.phase(mac(5)), Some(Phase::Waiting));
    assert_eq!(phases(&lifecycle).len(), 5);

    // Stopping one frees a slot for the fifth.
    assert_eq!(
        lifecycle.stop(mac(2), t0),
        [removed(2, Removal::Stopped), bridged(5)]
    );
}

#[test]
fn controllers_are_listed_in_arrival_order() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    for n in [3, 1, 2] {
        lifecycle.link_arrived(mac(n), t0);
    }
    let order: Vec<MacAddress> = lifecycle.controllers().map(|(mac, _)| mac).collect();
    assert_eq!(order, [mac(3), mac(1), mac(2)]);
}

#[test]
fn stopping_and_starting_a_controller() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);

    assert_eq!(lifecycle.stop(mac(1), t0), [removed(1, Removal::Stopped)]);
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Stopped));
    assert!(lifecycle.settings().controller(mac(1)).stopped);
    // Stopping twice changes nothing.
    assert_eq!(lifecycle.stop(mac(1), t0), []);

    assert_eq!(lifecycle.start(mac(1), t0), [bridged(1)]);
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Bridged));
    assert!(!lifecycle.settings().controller(mac(1)).stopped);
}

#[test]
fn a_stopped_controller_stays_stopped_after_reconnecting() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);
    lifecycle.stop(mac(1), t0);

    assert_eq!(lifecycle.link_lost(mac(1), t0), []);
    assert_eq!(lifecycle.phase(mac(1)), None);
    assert_eq!(lifecycle.link_arrived(mac(1), t0 + secs(1.0)), []);
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Stopped));
}

#[test]
fn a_stopped_controller_stays_stopped_after_an_app_restart() {
    let t0 = Instant::now();
    let mut first_run = Lifecycle::new(Settings::default());
    first_run.link_arrived(mac(1), t0);
    first_run.stop(mac(1), t0);
    let saved = first_run.settings().to_json();

    let mut second_run = Lifecycle::new(Settings::load(Some(&saved)));
    assert_eq!(second_run.link_arrived(mac(1), t0), []);
    assert_eq!(second_run.phase(mac(1)), Some(Phase::Stopped));
}

#[test]
fn a_controller_that_loses_its_link_is_held_for_30_seconds_then_removed() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);

    assert_eq!(HOLD_TIME, secs(30.0));
    assert_eq!(
        lifecycle.link_lost(mac(1), t0),
        [Change::Held { mac: mac(1) }]
    );
    assert_eq!(
        lifecycle.phase(mac(1)),
        Some(Phase::Held {
            until: t0 + HOLD_TIME
        })
    );
    assert_eq!(lifecycle.next_deadline(), Some(t0 + HOLD_TIME));

    assert_eq!(lifecycle.tick(t0 + secs(29.9)), []);
    assert_eq!(
        lifecycle.tick(t0 + HOLD_TIME),
        [removed(1, Removal::Expired)]
    );
    assert_eq!(lifecycle.phase(mac(1)), None);
    assert_eq!(lifecycle.next_deadline(), None);
}

#[test]
fn a_held_controller_that_returns_in_time_resumes() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);
    lifecycle.link_lost(mac(1), t0);

    assert_eq!(
        lifecycle.link_arrived(mac(1), t0 + secs(29.9)),
        [Change::Returned { mac: mac(1) }]
    );
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Bridged));
    // The old deadline no longer applies.
    assert_eq!(lifecycle.tick(t0 + secs(60.0)), []);
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Bridged));
}

#[test]
fn a_controller_returning_after_its_hold_expired_gets_a_new_bridge() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);
    lifecycle.link_lost(mac(1), t0);
    lifecycle.tick(t0 + HOLD_TIME);

    assert_eq!(
        lifecycle.link_arrived(mac(1), t0 + secs(31.0)),
        [bridged(1)]
    );
}

#[test]
fn a_held_controller_keeps_its_slot() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    for n in 1..=4 {
        lifecycle.link_arrived(mac(n), t0);
    }
    lifecycle.link_lost(mac(1), t0);
    assert_eq!(lifecycle.link_arrived(mac(5), t0), []);
    assert_eq!(lifecycle.phase(mac(5)), Some(Phase::Waiting));

    // Once the hold expires, the waiting Controller takes the slot.
    assert_eq!(
        lifecycle.tick(t0 + HOLD_TIME),
        [removed(1, Removal::Expired), bridged(5)]
    );
}

#[test]
fn stopping_a_held_controller_removes_it_at_once() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);
    lifecycle.link_lost(mac(1), t0);

    assert_eq!(lifecycle.stop(mac(1), t0), [removed(1, Removal::Stopped)]);
    assert_eq!(lifecycle.phase(mac(1)), None);
    assert!(lifecycle.settings().controller(mac(1)).stopped);
}

#[test]
fn a_waiting_controller_that_leaves_is_forgotten() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    for n in 1..=5 {
        lifecycle.link_arrived(mac(n), t0);
    }
    assert_eq!(lifecycle.link_lost(mac(5), t0), []);
    assert_eq!(lifecycle.phase(mac(5)), None);
}

#[test]
fn a_second_link_for_a_bridged_controller_changes_nothing() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);
    assert_eq!(lifecycle.link_arrived(mac(1), t0), []);
    assert_eq!(phases(&lifecycle), [(mac(1), Phase::Bridged)]);
}

#[test]
fn a_bridge_that_fails_is_retried_after_a_delay() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);

    assert_eq!(
        lifecycle.bridge_failed(mac(1), t0),
        [removed(1, Removal::Failed)]
    );
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Waiting));
    assert_eq!(lifecycle.next_deadline(), Some(t0 + BRIDGE_RETRY_DELAY));
    assert_eq!(lifecycle.tick(t0 + BRIDGE_RETRY_DELAY - secs(0.1)), []);
    assert_eq!(lifecycle.tick(t0 + BRIDGE_RETRY_DELAY), [bridged(1)]);
}

#[test]
fn quitting_removes_every_virtual_controller() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    for n in 1..=3 {
        lifecycle.link_arrived(mac(n), t0);
    }
    lifecycle.link_lost(mac(2), t0);
    lifecycle.stop(mac(3), t0);

    assert_eq!(
        lifecycle.quit(),
        [removed(1, Removal::Quit), removed(2, Removal::Quit)]
    );
    assert_eq!(phases(&lifecycle), []);
}

#[test]
fn a_controller_on_two_links_is_listed_once_and_held_only_when_both_are_lost() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    assert_eq!(lifecycle.link_arrived(mac(1), t0), [bridged(1)]);
    assert_eq!(lifecycle.link_arrived(mac(1), t0), []);
    assert_eq!(phases(&lifecycle), [(mac(1), Phase::Bridged)]);

    assert_eq!(lifecycle.link_lost(mac(1), t0), []);
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Bridged));
    assert_eq!(
        lifecycle.link_lost(mac(1), t0),
        [Change::Held { mac: mac(1) }]
    );

    // Either Link coming back ends the Hold; a second one changes nothing more.
    assert_eq!(
        lifecycle.link_arrived(mac(1), t0 + secs(1.0)),
        [Change::Returned { mac: mac(1) }]
    );
    assert_eq!(lifecycle.link_arrived(mac(1), t0 + secs(1.0)), []);
    assert_eq!(lifecycle.link_lost(mac(1), t0 + secs(2.0)), []);
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Bridged));
}

#[test]
fn a_stopped_controller_on_two_links_stays_listed_until_both_are_lost() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);
    lifecycle.link_arrived(mac(1), t0);
    lifecycle.stop(mac(1), t0);

    assert_eq!(lifecycle.link_lost(mac(1), t0), []);
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Stopped));
    assert_eq!(lifecycle.link_lost(mac(1), t0), []);
    assert_eq!(lifecycle.phase(mac(1)), None);
}

// ---- Output Type ----------------------------------------------------------

fn bridged_ps4(n: u8) -> Change {
    Change::Bridged {
        mac: mac(n),
        output_type: OutputType::Ps4,
    }
}

#[test]
fn changing_the_output_type_replaces_the_virtual_controller_and_is_remembered() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);

    assert_eq!(
        lifecycle.set_output_type(mac(1), OutputType::Ps4, t0),
        [removed(1, Removal::OutputTypeChanged), bridged_ps4(1)]
    );
    assert_eq!(lifecycle.phase(mac(1)), Some(Phase::Bridged));
    assert_eq!(
        lifecycle.settings().controller(mac(1)).output_type,
        OutputType::Ps4
    );
}

#[test]
fn choosing_the_output_type_already_in_use_changes_nothing() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);
    assert_eq!(lifecycle.set_output_type(mac(1), OutputType::Xbox, t0), []);
}

#[test]
fn a_stopped_controller_takes_its_new_output_type_when_started() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);
    lifecycle.stop(mac(1), t0);

    assert_eq!(lifecycle.set_output_type(mac(1), OutputType::Ps4, t0), []);
    assert_eq!(lifecycle.start(mac(1), t0), [bridged_ps4(1)]);
}

#[test]
fn a_held_controller_that_changes_output_type_loses_its_virtual_controller() {
    let t0 = Instant::now();
    let mut lifecycle = Lifecycle::new(Settings::default());
    lifecycle.link_arrived(mac(1), t0);
    lifecycle.link_lost(mac(1), t0);

    assert_eq!(
        lifecycle.set_output_type(mac(1), OutputType::Ps4, t0),
        [removed(1, Removal::OutputTypeChanged)]
    );
    assert_eq!(lifecycle.phase(mac(1)), None);
    // It comes back as a new Controller with the new Output Type.
    assert_eq!(
        lifecycle.link_arrived(mac(1), t0 + secs(1.0)),
        [bridged_ps4(1)]
    );
}

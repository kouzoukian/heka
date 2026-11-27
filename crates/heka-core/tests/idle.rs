use std::time::{Duration, Instant};

use heka_core::idle::{Activity, IdleTime, IdleTimer};
use heka_core::state::{Battery, Buttons, Charge, ControllerState, Finger, Motion, Stick};

const RESTING: ControllerState = ControllerState::NEUTRAL;
const LIMIT: Duration = Duration::from_secs(300);

fn counted(states: &[ControllerState]) -> Vec<bool> {
    let mut activity = Activity::default();
    activity.observe(&RESTING);
    states.iter().map(|state| activity.observe(state)).collect()
}

fn stick(x: u8, y: u8) -> ControllerState {
    ControllerState {
        left_stick: Stick { x, y },
        ..RESTING
    }
}

fn finger(id: u8, x: u16, y: u16) -> ControllerState {
    ControllerState {
        touch: [Some(Finger { id, x, y }), None],
        ..RESTING
    }
}

#[test]
fn the_first_state_is_where_activity_is_measured_from() {
    let mut activity = Activity::default();
    let pressed = ControllerState {
        buttons: Buttons::CROSS,
        ..RESTING
    };

    assert!(!activity.observe(&pressed));
    assert!(!activity.observe(&pressed));
    assert!(activity.observe(&RESTING));
}

#[test]
fn pressing_and_releasing_a_button_are_both_activity() {
    let pressed = ControllerState {
        buttons: Buttons::PS,
        ..RESTING
    };

    assert_eq!(counted(&[pressed, pressed, RESTING]), [true, false, true]);
}

#[test]
fn motion_and_battery_alone_are_not_activity() {
    let moved = ControllerState {
        motion: Motion {
            gyro: [90_000, -90_000, 4],
            accel: [8192, 0, -8192],
            timestamp: 1_000,
        },
        ..RESTING
    };
    let charged = ControllerState {
        battery: Some(Battery {
            percent: 55,
            charge: Charge::Charging,
        }),
        ..RESTING
    };

    assert_eq!(counted(&[moved, charged]), [false, false]);
}

#[test]
fn a_stick_counts_only_past_a_small_threshold_on_either_axis() {
    assert_eq!(
        counted(&[stick(136, 120), stick(137, 128), stick(137, 128)]),
        [false, true, false]
    );
    assert_eq!(counted(&[stick(128, 119)]), [true]);
    assert_eq!(
        counted(&[ControllerState {
            right_stick: Stick { x: 100, y: 128 },
            ..RESTING
        }]),
        [true]
    );
}

#[test]
fn a_stick_moved_slowly_counts_once_it_is_far_from_where_it_last_counted() {
    assert_eq!(
        counted(&[
            stick(133, 128),
            stick(136, 128),
            stick(139, 128),
            stick(141, 128)
        ]),
        [false, false, true, false]
    );
}

#[test]
fn either_trigger_counts_only_past_a_small_threshold() {
    let left = |left_trigger| ControllerState {
        left_trigger,
        ..RESTING
    };
    let right = ControllerState {
        right_trigger: 9,
        ..RESTING
    };

    assert_eq!(counted(&[left(8), left(9)]), [false, true]);
    assert_eq!(counted(&[right]), [true]);
}

#[test]
fn a_finger_landing_moving_or_lifting_is_activity_but_not_its_jitter() {
    assert_eq!(
        counted(&[
            finger(1, 500, 500),
            finger(1, 516, 484),
            finger(1, 517, 500),
            finger(1, 517, 483),
            finger(2, 517, 483),
            RESTING,
        ]),
        [true, false, true, true, true, true]
    );
    let second = ControllerState {
        touch: [None, Some(Finger { id: 3, x: 1, y: 1 })],
        ..RESTING
    };
    assert_eq!(counted(&[second]), [true]);
}

#[test]
fn a_timer_runs_out_once_the_limit_passes_without_activity() {
    let t0 = Instant::now();
    let mut timer = IdleTimer::default();

    assert!(!timer.runs_out(LIMIT, 7, t0));
    assert!(!timer.runs_out(LIMIT, 7, t0 + LIMIT - Duration::from_millis(1)));
    assert!(timer.runs_out(LIMIT, 7, t0 + LIMIT));
}

#[test]
fn activity_starts_a_timer_over() {
    let t0 = Instant::now();
    let mut timer = IdleTimer::default();
    timer.runs_out(LIMIT, 0, t0);

    assert!(!timer.runs_out(LIMIT, 1, t0 + LIMIT));
    assert!(!timer.runs_out(LIMIT, 1, t0 + LIMIT * 2 - Duration::from_millis(1)));
    assert!(timer.runs_out(LIMIT, 1, t0 + LIMIT * 2));
}

#[test]
fn a_timer_that_ran_out_or_was_restarted_waits_the_whole_limit_again() {
    let t0 = Instant::now();
    let mut timer = IdleTimer::default();
    timer.runs_out(LIMIT, 0, t0);
    assert!(timer.runs_out(LIMIT, 0, t0 + LIMIT));
    assert!(!timer.runs_out(LIMIT, 0, t0 + LIMIT + Duration::from_millis(250)));
    assert!(timer.runs_out(LIMIT, 0, t0 + LIMIT * 2));

    timer.restart();
    assert!(!timer.runs_out(LIMIT, 0, t0 + LIMIT * 4));
    assert!(timer.runs_out(LIMIT, 0, t0 + LIMIT * 5));
}

#[test]
fn the_idle_times_are_5_10_15_and_30_minutes() {
    assert_eq!(IdleTime::ALL.map(IdleTime::minutes), [5, 10, 15, 30]);
    assert_eq!(IdleTime::Fifteen.duration(), Duration::from_secs(15 * 60));
}

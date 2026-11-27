mod rig;

use std::time::Duration;

use heka_core::desktop::DesktopEvent;
use heka_core::drift::{DriftError, DriftMeter};
use heka_core::gyro::{GyroActivation, GyroAxis, GyroChange, GyroSettings, GyroUse};
use heka_core::profile::{Input, Mapper, Profile};
use heka_core::state::{Buttons, ControllerState, Motion, Stick};
use heka_core::xbox::XboxReport;
use rig::{Rig, moved};

const DEGREE: i32 = 1024;
const TICKS_A_MILLISECOND: u32 = 3000;

fn gyro(used_as: GyroUse, changes: &[GyroChange]) -> Profile {
    let mut profile = Profile::named("Aim");
    profile.change_gyro(GyroChange::Use(used_as));
    profile.change_gyro(GyroChange::Activation(GyroActivation::Always));
    profile.change_gyro(GyroChange::DeadZone(0));
    for &change in changes {
        profile.change_gyro(change);
    }
    profile
}

fn turning(pitch: i32, yaw: i32, roll: i32, at_millis: u32) -> ControllerState {
    ControllerState {
        motion: Motion {
            gyro: [pitch * DEGREE, yaw * DEGREE, roll * DEGREE],
            accel: [0; 3],
            timestamp: at_millis.wrapping_mul(TICKS_A_MILLISECOND),
        },
        ..ControllerState::NEUTRAL
    }
}

fn travel(rig: &mut Rig, millis: u32, step: u32, pitch: i32, yaw: i32, roll: i32) -> (i32, i32) {
    rig.report_after(0, turning(pitch, yaw, roll, 0));
    rig.take();
    for report in 1..=millis / step {
        rig.report_after(u64::from(step), turning(pitch, yaw, roll, report * step));
    }
    moved(&rig.take())
}

fn near(value: i32, expected: i32) -> bool {
    (value - expected).abs() <= 1
}

#[test]
fn the_gyro_does_nothing_until_chosen() {
    let mut rig = Rig::new(&Profile::default());

    assert_eq!(travel(&mut rig, 500, 4, 90, 90, 90), (0, 0));
}

#[test]
fn turning_right_and_tilting_up_moves_the_pointer_right_and_up() {
    let mut rig = Rig::new(&gyro(GyroUse::Mouse, &[]));

    let (x, y) = travel(&mut rig, 1000, 4, 10, -30, 0);

    assert!(near(x, 600), "{x}");
    assert!(near(y, -200), "{y}");
}

#[test]
fn the_pointer_moves_as_far_however_often_reports_arrive() {
    for step in [1, 4, 8, 11, 20] {
        let mut rig = Rig::new(&gyro(GyroUse::Mouse, &[]));

        let (x, _) = travel(&mut rig, 880, step, 0, -50, 0);

        assert!(near(x, 880), "{step}: {x}");
    }
}

#[test]
fn movement_follows_the_sensor_clock_not_the_time_of_arrival() {
    let mut rig = Rig::new(&gyro(GyroUse::Mouse, &[]));
    rig.report_after(0, turning(0, -100, 0, 0));
    rig.take();

    rig.report_after(1, turning(0, -100, 0, 10));
    rig.report_after(30, turning(0, -100, 0, 20));

    assert!(near(moved(&rig.take()).0, 40));
}

#[test]
fn a_wrapping_dualshock_4_clock_counts_on() {
    let mut rig = Rig::new(&gyro(GyroUse::Mouse, &[]));
    let at = |timestamp: u32| ControllerState {
        motion: Motion {
            gyro: [0, -100 * DEGREE, 0],
            accel: [0; 3],
            timestamp,
        },
        ..ControllerState::NEUTRAL
    };

    rig.report_after(0, at((1 << 20) - 15_000));
    rig.take();
    rig.report_after(10, at(15_000));

    assert!(near(moved(&rig.take()).0, 20));
}

#[test]
fn a_stall_in_the_reports_does_not_make_the_pointer_jump() {
    let mut rig = Rig::new(&gyro(GyroUse::Mouse, &[]));
    rig.report_after(0, turning(0, -100, 0, 0));
    rig.take();

    rig.report_after(1000, turning(0, -100, 0, 300));

    assert_eq!(moved(&rig.take()), (0, 0));
}

#[test]
fn slow_turning_adds_up() {
    let mut rig = Rig::new(&gyro(GyroUse::Mouse, &[]));

    let (x, _) = travel(&mut rig, 2000, 1, 0, -1, 0);

    assert!(near(x, 40), "{x}");
}

#[test]
fn sensitivity_and_inversion_are_set_for_each_axis() {
    let mut rig = Rig::new(&gyro(
        GyroUse::Mouse,
        &[
            GyroChange::SensitivityX(200),
            GyroChange::SensitivityY(50),
            GyroChange::InvertX(true),
            GyroChange::InvertY(true),
        ],
    ));

    let (x, y) = travel(&mut rig, 1000, 4, 20, -20, 0);

    assert!(near(x, -800), "{x}");
    assert!(near(y, 200), "{y}");
}

#[test]
fn the_dead_zone_swallows_small_turns_and_eases_in() {
    let still = travel(
        &mut Rig::new(&gyro(GyroUse::Mouse, &[GyroChange::DeadZone(5)])),
        1000,
        4,
        3,
        -3,
        0,
    );
    let (x, _) = travel(
        &mut Rig::new(&gyro(GyroUse::Mouse, &[GyroChange::DeadZone(5)])),
        1000,
        4,
        0,
        -15,
        0,
    );

    assert_eq!(still, (0, 0));
    assert!(near(x, 200), "{x}");
}

#[test]
fn rolling_right_can_move_the_pointer_in_place_of_turning() {
    let profile = gyro(GyroUse::Mouse, &[GyroChange::Axis(GyroAxis::Roll)]);

    assert_eq!(travel(&mut Rig::new(&profile), 500, 4, 0, -40, 0), (0, 0));
    let (x, _) = travel(&mut Rig::new(&profile), 500, 4, 0, 0, -40);
    assert!(near(x, 400), "{x}");
}

#[test]
fn the_measured_drift_is_removed() {
    let mut rig = Rig::new(&gyro(GyroUse::Mouse, &[]));
    rig.drift([2 * DEGREE, -3 * DEGREE, 0]);

    assert_eq!(travel(&mut rig, 1000, 4, 2, -3, 0), (0, 0));
}

fn holding(buttons: Buttons, at_millis: u32) -> ControllerState {
    ControllerState {
        buttons,
        ..turning(0, -100, 0, at_millis)
    }
}

fn moves(rig: &mut Rig, buttons: Buttons, from_millis: u32) -> bool {
    rig.report_after(10, holding(buttons, from_millis));
    rig.take();
    rig.report_after(10, holding(buttons, from_millis + 10));
    moved(&rig.take()) != (0, 0)
}

#[test]
fn the_gyro_can_work_only_while_a_button_is_held() {
    let mut rig = Rig::new(&gyro(
        GyroUse::Mouse,
        &[
            GyroChange::Activation(GyroActivation::WhileHeld),
            GyroChange::Button(Input::R1),
        ],
    ));

    assert!(!moves(&mut rig, Buttons::NONE, 0));
    assert!(moves(&mut rig, Buttons::R1, 20));
    assert!(!moves(&mut rig, Buttons::NONE, 40));
}

#[test]
fn the_button_can_toggle_the_gyro() {
    let mut rig = Rig::new(&gyro(
        GyroUse::Mouse,
        &[
            GyroChange::Activation(GyroActivation::Toggle),
            GyroChange::Button(Input::R1),
        ],
    ));

    assert!(!moves(&mut rig, Buttons::NONE, 0));
    assert!(moves(&mut rig, Buttons::R1, 20));
    assert!(moves(&mut rig, Buttons::NONE, 40));
    assert!(!moves(&mut rig, Buttons::R1, 60));
    assert!(!moves(&mut rig, Buttons::NONE, 80));
}

#[test]
fn a_changed_mapper_turns_a_toggled_gyro_off() {
    let profile = gyro(
        GyroUse::Mouse,
        &[
            GyroChange::Activation(GyroActivation::Toggle),
            GyroChange::Button(Input::R1),
        ],
    );
    let mut rig = Rig::new(&profile);
    assert!(moves(&mut rig, Buttons::R1, 0));

    rig.change(Mapper::new(&profile));

    assert!(!moves(&mut rig, Buttons::R1, 20));
}

fn right_stick(shown: ControllerState) -> (i16, i16) {
    let report = XboxReport::from_state(&shown);
    (report.thumb_rx, report.thumb_ry)
}

#[test]
fn as_a_stick_the_gyro_pushes_the_right_stick_by_how_fast_it_turns() {
    let mut rig = Rig::new(&gyro(GyroUse::RightStick, &[]));

    let shown = rig.report_after(0, turning(45, -90, 0, 0));

    let (x, y) = right_stick(shown);
    assert!((x - 16384).abs() < 300, "{x}");
    assert!((y - 8192).abs() < 300, "{y}");
    assert_eq!(shown.left_stick, Stick::CENTER);
    assert!(shown.right_stick.x > 180 && shown.right_stick.y < 100);
    assert_eq!(rig.take(), []);
}

#[test]
fn the_gyro_adds_to_the_real_stick_and_stays_in_range() {
    let mut rig = Rig::new(&gyro(GyroUse::RightStick, &[]));
    let pushed = |yaw| ControllerState {
        right_stick: Stick { x: 255, y: 128 },
        ..turning(0, yaw, 0, 0)
    };

    assert_eq!(right_stick(rig.report_after(0, pushed(-90))).0, i16::MAX);
    let back = right_stick(rig.report_after(4, pushed(90))).0;
    assert!((back - 16384).abs() < 300, "{back}");
    assert_eq!(rig.report_after(4, pushed(900)).right_stick.x, 0);
}

#[test]
fn a_still_gyro_leaves_the_sticks_untouched() {
    let mut rig = Rig::new(&gyro(GyroUse::RightStick, &[]));
    let state = ControllerState {
        right_stick: Stick { x: 33, y: 201 },
        ..turning(0, 0, 0, 0)
    };

    assert_eq!(rig.report_after(0, state), state);
}

#[test]
fn gyro_settings_survive_a_save_and_load_and_stay_out_of_a_plain_file() {
    let profile = gyro(
        GyroUse::RightStick,
        &[
            GyroChange::Activation(GyroActivation::Toggle),
            GyroChange::Button(Input::LeftBack),
            GyroChange::Axis(GyroAxis::Roll),
            GyroChange::SensitivityX(150),
            GyroChange::InvertY(true),
            GyroChange::DeadZone(5),
        ],
    );

    assert_eq!(Profile::load(&profile.to_json(), "x"), Some(profile));
    let plain: serde_json::Value =
        serde_json::from_str(&Profile::named("Plain").to_json()).unwrap();
    assert!(plain.get("gyro").is_none());
}

#[test]
fn broken_gyro_settings_fall_back_field_by_field() {
    let text = r#"{"gyro": {"use": "mouse", "sensitivity_x": 9000, "dead_zone": 90,
        "button": "elbow", "axis": 3}}"#;

    let profile = Profile::load(text, "Aim").unwrap();

    assert_eq!(
        *profile.gyro(),
        GyroSettings {
            used_as: GyroUse::Mouse,
            sensitivity_x: 300,
            dead_zone: 20,
            ..GyroSettings::default()
        }
    );
}

fn measure(readings: impl IntoIterator<Item = [i32; 3]>) -> Option<Result<[i32; 3], DriftError>> {
    let started = std::time::Instant::now();
    let mut meter = DriftMeter::new(started);
    for (report, gyro) in readings.into_iter().enumerate() {
        let report = u32::try_from(report).unwrap();
        let motion = Motion {
            gyro,
            accel: [0; 3],
            timestamp: report * 4 * TICKS_A_MILLISECOND,
        };
        let now = started + Duration::from_millis(u64::from(report) * 4);
        if let Some(result) = meter.add(&motion, now) {
            return Some(result);
        }
    }
    None
}

#[test]
fn a_second_at_rest_measures_the_drift() {
    let resting = |report: i32| [2000 + report % 3, -3000 - report % 5, 500];

    let measured = measure((0..300).map(resting)).unwrap().unwrap();

    assert!((measured[0] - 2001).abs() <= 1, "{measured:?}");
    assert!((measured[1] + 3002).abs() <= 1, "{measured:?}");
    assert_eq!(measured[2], 500);
    assert_eq!(measure((0..200).map(resting)), None);
}

#[test]
fn a_controller_that_moves_is_not_measured() {
    let shaken = (0..300).map(|report| [if report == 40 { 20 * DEGREE } else { 0 }, 0, 0]);
    let turning = std::iter::repeat_n([0, 40 * DEGREE, 0], 300);

    assert_eq!(measure(shaken), Some(Err(DriftError::Moved)));
    assert_eq!(measure(turning), Some(Err(DriftError::Moved)));
}

#[test]
fn a_measurement_gives_up_without_enough_reports() {
    let started = std::time::Instant::now();
    let meter = DriftMeter::new(started);

    assert!(!meter.is_overdue(started + Duration::from_secs(4)));
    assert!(meter.is_overdue(started + Duration::from_secs(6)));
}

#[test]
fn a_rig_sends_nothing_for_a_stick_gyro() {
    let events: [DesktopEvent; 0] = [];
    let mut rig = Rig::new(&gyro(GyroUse::RightStick, &[]));
    travel(&mut rig, 100, 4, 50, 50, 0);

    assert_eq!(rig.take(), events);
}

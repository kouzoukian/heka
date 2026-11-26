//! DualShock 4 USB input report in, Controller state out.
//!
//! No DualShock 4 is available, so every report is DERIVED from the layout in
//! Linux `hid-playstation.c` (see `common::ds4_usb`). Offsets here include the
//! report ID byte.

mod common;

use heka_core::calibration::Calibration;
use heka_core::dualshock4::{parse_usb_calibration, parse_usb_input_report};
use heka_core::link::ReportError;
use heka_core::ps4::Ps4Translator;
use heka_core::state::{Battery, Buttons, Charge, ControllerState, Finger, Stick};

fn parse(changes: &[(usize, u8)]) -> ControllerState {
    parse_usb_input_report(&common::ds4_usb(changes), &Calibration::UNCALIBRATED).unwrap()
}

#[test]
fn a_resting_report_has_nothing_pressed_and_no_finger_down() {
    let state = parse(&[]);
    assert_eq!(state.buttons, Buttons::NONE);
    assert_eq!(state.left_stick, Stick::CENTER);
    assert_eq!(state.right_stick, Stick::CENTER);
    assert_eq!((state.left_trigger, state.right_trigger), (0, 0));
    assert_eq!(state.touch, [None, None]);
}

#[test]
fn sticks_and_triggers_are_read_from_their_bytes() {
    let state = parse(&[(1, 1), (2, 2), (3, 3), (4, 4), (8, 5), (9, 6)]);
    assert_eq!(state.left_stick, Stick { x: 1, y: 2 });
    assert_eq!(state.right_stick, Stick { x: 3, y: 4 });
    assert_eq!((state.left_trigger, state.right_trigger), (5, 6));
}

#[test]
fn every_button_is_read_from_its_bit() {
    // `DS_BUTTONS0_*`, `DS_BUTTONS1_*` and `DS_BUTTONS2_*`. Share is `CREATE`.
    let bits = [
        (5, 4, Buttons::SQUARE),
        (5, 5, Buttons::CROSS),
        (5, 6, Buttons::CIRCLE),
        (5, 7, Buttons::TRIANGLE),
        (6, 0, Buttons::L1),
        (6, 1, Buttons::R1),
        (6, 2, Buttons::L2),
        (6, 3, Buttons::R2),
        (6, 4, Buttons::CREATE),
        (6, 5, Buttons::OPTIONS),
        (6, 6, Buttons::L3),
        (6, 7, Buttons::R3),
        (7, 0, Buttons::PS),
        (7, 1, Buttons::TOUCHPAD),
    ];
    for (byte, bit, button) in bits {
        // Byte 5 also holds the hat, which stays released (8).
        let hat = if byte == 5 { 0x08 } else { 0 };
        let state = parse(&[(byte, hat | (1 << bit))]);
        assert_eq!(state.buttons, button, "byte {byte} bit {bit}");
    }
}

#[test]
fn the_report_counter_beside_ps_and_touchpad_presses_nothing() {
    // Bits 2..=7 of byte 7 count reports on a DualShock 4, which has no mute button.
    assert_eq!(parse(&[(7, 0xFC)]).buttons, Buttons::NONE);
}

#[test]
fn the_hat_gives_the_dpad_clockwise_from_up() {
    use Buttons as B;
    let expected = [
        B::DPAD_UP,
        B::DPAD_UP | B::DPAD_RIGHT,
        B::DPAD_RIGHT,
        B::DPAD_DOWN | B::DPAD_RIGHT,
        B::DPAD_DOWN,
        B::DPAD_DOWN | B::DPAD_LEFT,
        B::DPAD_LEFT,
        B::DPAD_UP | B::DPAD_LEFT,
        B::NONE,
    ];
    for (hat, dpad) in (0u8..).zip(expected) {
        assert_eq!(parse(&[(5, hat)]).buttons, dpad, "hat {hat}");
    }
}

#[test]
fn other_reports_and_short_reads_are_refused() {
    let mut report = common::ds4_usb(&[]);
    let parse = |report: &[u8]| parse_usb_input_report(report, &Calibration::UNCALIBRATED);
    assert_eq!(parse(&report[..10]), Err(ReportError::TooShort { len: 10 }));
    report[0] = 0x11;
    assert_eq!(parse(&report), Err(ReportError::UnexpectedId { id: 0x11 }));
}

/// DERIVED: a USB calibration feature report (0x02, 37 bytes) from the fields
/// `dualshock4_get_calibration_data` reads, each a little-endian i16 starting at
/// byte 1.
fn calibration_report() -> Vec<u8> {
    let fields: [i16; 17] = [
        0, 0, 0, // gyro biases: pitch, yaw, roll
        1000, -1000, 2000, -2000, 4000, -4000, // gyro plus and minus, axis by axis
        500, 500, // gyro speed plus and minus
        8100, -8100, 8292, -8092, 4096, -4096, // accelerometer x, y, z plus and minus
    ];
    let mut report = vec![0x02];
    report.extend(fields.iter().flat_map(|field| field.to_le_bytes()));
    report.resize(37, 0);
    report
}

/// `(offset, value)` pairs writing little-endian `i16`s from `offset`.
fn le16s(offset: usize, values: [i16; 3]) -> Vec<(usize, u8)> {
    let bytes = values.iter().flat_map(|value| value.to_le_bytes());
    (offset..).zip(bytes).collect()
}

#[test]
fn motion_is_read_with_the_controllers_calibration() {
    let calibration = parse_usb_calibration(&calibration_report()).unwrap();
    // Gyro from byte 13, accelerometer from byte 19.
    let changes = [le16s(13, [100, 100, -100]), le16s(19, [8100, 100, -2048])].concat();
    let state = parse_usb_input_report(&common::ds4_usb(&changes), &calibration).unwrap();
    // Gyro: raw * (500 + 500) * 1024 / (plus - minus).
    assert_eq!(state.motion.gyro, [51_200, 25_600, -12_800]);
    // Accelerometer: (raw - middle) * 16384 / (plus - minus), y's middle being 100.
    assert_eq!(state.motion.accel, [8192, 0, -4096]);
}

#[test]
fn the_timestamp_counts_sixteen_dualsense_ticks_for_each_dualshock_4_tick() {
    // le16 sensor_timestamp at byte 10, in units of 16/3 microseconds.
    let state = parse(&[(10, 0x34), (11, 0x12)]);
    assert_eq!(state.motion.timestamp, 0x0001_2340);
}

#[test]
fn a_calibration_report_with_another_id_or_cut_short_is_refused() {
    let mut report = calibration_report();
    assert_eq!(
        parse_usb_calibration(&report[..36]),
        Err(ReportError::TooShort { len: 36 })
    );
    report[0] = 0x05;
    assert_eq!(
        parse_usb_calibration(&report),
        Err(ReportError::UnexpectedId { id: 0x05 })
    );
}

/// The 4 bytes of a finger: contact, x low, x high nibble and y low nibble, y high.
fn finger(offset: usize, contact: u8, x: u16, y: u16) -> Vec<(usize, u8)> {
    let bytes = [
        contact,
        x as u8,
        (x >> 8) as u8 | ((y & 0x0F) << 4) as u8,
        (y >> 4) as u8,
    ];
    (offset..).zip(bytes).collect()
}

#[test]
fn touch_is_reported_in_dualsense_touchpad_units() {
    // The fingers of the first touch report are at bytes 35 and 39. The DualShock
    // 4's pad is 942 high and the state's is 1080.
    let changes = [finger(35, 5, 1000, 500), finger(39, 6, 1919, 941)].concat();
    let at = |id, x, y| Some(Finger { id, x, y });
    assert_eq!(parse(&changes).touch, [at(5, 1000, 573), at(6, 1919, 1079)]);
}

#[test]
fn the_newest_touch_report_gives_the_touch() {
    // Three touch reports, oldest first, at bytes 34, 43 and 52: their first
    // fingers are at 35, 44 and 53.
    let changes = [
        vec![(33, 3)],
        finger(35, 1, 10, 0),
        finger(44, 1, 20, 0),
        finger(53, 1, 30, 0),
        vec![(57, 0x80)],
    ]
    .concat();
    let newest = Finger { id: 1, x: 30, y: 0 };
    assert_eq!(parse(&changes).touch, [Some(newest), None]);
}

#[test]
fn without_a_new_touch_report_the_first_one_is_read() {
    // As SDL does (`SDL_hidapi_ps4.c` reads the first touch report whatever the count).
    let changes = [vec![(33, 0)], finger(35, 2, 40, 0)].concat();
    let first = Finger { id: 2, x: 40, y: 0 };
    assert_eq!(parse(&changes).touch[0], Some(first));
}

#[test]
fn a_touch_reaches_a_ps4_virtual_controller_unchanged() {
    let mut translator = Ps4Translator::default();
    for y in 0..=941u16 {
        let input = common::ds4_usb(&finger(35, 3, 960, y));
        let state = parse_usb_input_report(&input, &Calibration::UNCALIBRATED).unwrap();
        // The virtual report has no report ID: its first finger is at byte 34.
        assert_eq!(translator.report(&state)[34..38], input[35..39], "y {y}");
    }
}

#[test]
fn the_battery_level_and_charge_come_from_the_first_status_byte() {
    // status[0] at byte 30: capacity in bits 0..=3, cable connected in bit 4.
    let battery = |status| parse(&[(30, status)]).battery.unwrap();
    let is = |percent, charge| Battery { percent, charge };
    assert_eq!(battery(0x00), is(5, Charge::Discharging));
    assert_eq!(battery(0x07), is(75, Charge::Discharging));
    assert_eq!(battery(0x0A), is(100, Charge::Discharging));
    // Without a cable the kernel reads anything past 9 as full.
    assert_eq!(battery(0x0B), is(100, Charge::Discharging));
    assert_eq!(battery(0x13), is(35, Charge::Charging));
    assert_eq!(battery(0x1A), is(100, Charge::Charging));
    assert_eq!(battery(0x1B), is(100, Charge::Full));
    assert_eq!(battery(0x1E), is(0, Charge::NotCharging));
    assert_eq!(battery(0x1F), is(0, Charge::Unknown));
}

//! Report fixtures shared by the integration tests.
//!
//! CAPTURED fixtures are raw bytes read from a real Controller and stored under
//! `tests/fixtures`. DERIVED reports are a captured report with bytes changed at
//! offsets taken from a cited layout (see `heka_core::dualsense`), because nobody
//! was pressing buttons during the capture. Tests name which kind they use.

#![allow(dead_code)]

/// Every report in a fixture file: one hex line per report, `#` starts a comment.
pub fn reports(fixture: &str) -> Vec<Vec<u8>> {
    fixture
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .map(|byte| u8::from_str_radix(byte, 16).expect("fixture byte is hex"))
                .collect()
        })
        .collect()
}

/// CAPTURED: 40 consecutive USB input reports from a resting DualSense.
pub fn captured_idle() -> Vec<Vec<u8>> {
    reports(include_str!("../fixtures/dualsense_usb_idle.hex"))
}

/// CAPTURED: the first idle report, the base every DERIVED report starts from.
pub fn captured_baseline() -> Vec<u8> {
    captured_idle().remove(0)
}

/// DERIVED: the captured baseline with `(offset, value)` bytes overwritten.
pub fn derived(changes: &[(usize, u8)]) -> Vec<u8> {
    let mut report = captured_baseline();
    for &(offset, value) in changes {
        report[offset] = value;
    }
    report
}

/// DERIVED: a resting DualShock 4's USB input report with `(offset, value)` bytes
/// overwritten. No DualShock 4 capture exists, so it is written from `struct
/// dualshock4_input_report_usb` in Linux `hid-playstation.c`: report ID 0x01, 64
/// bytes, sticks at 1..=4, buttons at 5..=7 (hat 8 = released), triggers at 8 and
/// 9, the touch report count at 33, then touch reports of 9 bytes from 34
/// (timestamp, then two 4-byte fingers). One touch report, both fingers lifted.
pub fn ds4_usb(changes: &[(usize, u8)]) -> Vec<u8> {
    let mut report = vec![0u8; 64];
    let resting = [
        (0, 0x01),
        (1, 0x80),
        (2, 0x80),
        (3, 0x80),
        (4, 0x80),
        (5, 0x08),
        (33, 1),
        (35, 0x80),
        (39, 0x80),
    ];
    for &(offset, value) in resting.iter().chain(changes) {
        report[offset] = value;
    }
    report
}

/// DERIVED: the DualShock 4 Bluetooth input report carrying the same input as a
/// USB report, from `struct dualshock4_input_report_bt`: report ID 0x11, 78
/// bytes, the common block from byte 3 (byte 1 over USB), the touch report count
/// at 35, four touch reports from 36 (USB holds three; the fourth is left with
/// both fingers lifted), and a CRC32 seeded with 0xA1 in the last 4 bytes.
pub fn ds4_bluetooth_from_usb(usb: &[u8]) -> Vec<u8> {
    let mut report = vec![0u8; 78];
    report[0] = 0x11;
    report[1] = 0xC0;
    report[3..63].copy_from_slice(&usb[1..61]);
    report[64] = 0x80;
    report[68] = 0x80;
    seal(0xA1, &mut report);
    report
}

/// Writes [`reference_crc`] of everything before the last 4 bytes into them.
pub fn seal(seed: u8, report: &mut [u8]) {
    let split = report.len() - 4;
    let crc = reference_crc(seed, &report[..split]);
    report[split..].copy_from_slice(&crc.to_le_bytes());
}

/// A bit-at-a-time CRC-32 (IEEE, reflected), independent of `heka_core::crc`,
/// over `seed` then `bytes`, as Bluetooth reports carry it (`ps_check_crc32`).
pub fn reference_crc(seed: u8, bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in std::iter::once(&seed).chain(bytes) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// DERIVED: the Bluetooth input report carrying the same input as a USB report.
///
/// Layout from Linux `hid-playstation.c`: report ID 0x31, 78 bytes, `struct
/// dualsense_input_report` from byte 2 (the USB report has it from byte 1), and a
/// CRC32 seeded with 0xA1 in the last 4 bytes (`dualsense_parse_report`).
pub fn bluetooth_from_usb(usb: &[u8]) -> Vec<u8> {
    let mut report = vec![0u8; 78];
    report[0] = 0x31;
    report[2..65].copy_from_slice(&usb[1..64]);
    let crc = reference_crc(0xA1, &report[..74]);
    report[74..].copy_from_slice(&crc.to_le_bytes());
    report
}

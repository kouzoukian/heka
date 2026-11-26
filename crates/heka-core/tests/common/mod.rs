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

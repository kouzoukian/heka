#![allow(dead_code)]

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

pub fn captured_idle() -> Vec<Vec<u8>> {
    reports(include_str!("../fixtures/dualsense_usb_idle.hex"))
}

pub fn captured_baseline() -> Vec<u8> {
    captured_idle().remove(0)
}

pub fn derived(changes: &[(usize, u8)]) -> Vec<u8> {
    let mut report = captured_baseline();
    for &(offset, value) in changes {
        report[offset] = value;
    }
    report
}

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

pub fn seal(seed: u8, report: &mut [u8]) {
    let split = report.len() - 4;
    let crc = reference_crc(seed, &report[..split]);
    report[split..].copy_from_slice(&crc.to_le_bytes());
}

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

pub fn bluetooth_from_usb(usb: &[u8]) -> Vec<u8> {
    let mut report = vec![0u8; 78];
    report[0] = 0x31;
    report[2..65].copy_from_slice(&usb[1..64]);
    let crc = reference_crc(0xA1, &report[..74]);
    report[74..].copy_from_slice(&crc.to_le_bytes());
    report
}

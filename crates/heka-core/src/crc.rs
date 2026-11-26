//! The CRC32 that DualSense and DualShock 4 reports carry over Bluetooth.
//!
//! Source: Linux `drivers/hid/hid-playstation.c` (`PS_*_CRC32_SEED`,
//! `ps_check_crc32`, `dualsense_send_output_report`) and SDL
//! `SDL_hidapi_ps5.c` (`VerifyCRC`, which notes that the HIDP header byte is part
//! of the calculation). The CRC is the standard CRC-32 (IEEE, reflected,
//! polynomial `0xEDB88320`) over a one-byte seed followed by the report, report
//! ID included and the trailing 4 CRC bytes excluded, stored little-endian.

/// Seed of an input report: the HIDP header of a DATA | INPUT transaction.
pub const BT_INPUT_SEED: u8 = 0xA1;
/// Seed of an output report: the HIDP header of a DATA | OUTPUT transaction.
pub const BT_OUTPUT_SEED: u8 = 0xA2;
/// Seed of a feature report.
pub const BT_FEATURE_SEED: u8 = 0xA3;

/// Length of the CRC at the end of a Bluetooth report.
pub const CRC_LEN: usize = 4;

const POLYNOMIAL: u32 = 0xEDB8_8320;

const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ POLYNOMIAL
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

fn update(mut crc: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        crc = TABLE[((crc ^ u32::from(byte)) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc
}

/// The standard CRC-32 of `bytes`.
pub fn crc32(bytes: &[u8]) -> u32 {
    !update(!0, bytes)
}

/// The CRC of a Bluetooth report: over `seed`, then `report` (which excludes the CRC bytes).
pub fn report_crc(seed: u8, report: &[u8]) -> u32 {
    !update(update(!0, &[seed]), report)
}

/// True when the last [`CRC_LEN`] bytes of `report` are its CRC for this seed.
pub fn has_valid_crc(seed: u8, report: &[u8]) -> bool {
    let Some(split) = report.len().checked_sub(CRC_LEN) else {
        return false;
    };
    let (body, crc) = report.split_at(split);
    report_crc(seed, body) == u32::from_le_bytes([crc[0], crc[1], crc[2], crc[3]])
}

/// Writes the CRC of everything before the last [`CRC_LEN`] bytes into them.
pub fn seal(seed: u8, report: &mut [u8]) {
    let split = report.len() - CRC_LEN;
    let crc = report_crc(seed, &report[..split]);
    report[split..].copy_from_slice(&crc.to_le_bytes());
}

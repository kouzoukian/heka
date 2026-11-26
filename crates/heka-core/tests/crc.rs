//! The CRC32 that PlayStation Controllers put at the end of Bluetooth reports.
//!
//! Expected values come from Python's `zlib.crc32`, an independent CRC-32
//! (IEEE 802.3) implementation, and from the CRC-32 catalogue check value.

use heka_core::crc::{BT_INPUT_SEED, BT_OUTPUT_SEED, crc32, report_crc};

#[test]
fn crc32_matches_the_catalogue_check_value() {
    // CRC-32/ISO-HDLC check value for "123456789".
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn a_report_crc_covers_the_seed_byte_then_the_report() {
    // The kernel (`ps_check_crc32`, `dualsense_send_output_report`) runs the CRC
    // over the one-byte seed, then the report without its last 4 bytes.
    // zlib.crc32(bytes([0xA2])) == 0xEADA2D49
    assert_eq!(report_crc(BT_OUTPUT_SEED, &[]), 0xEADA_2D49);
    // zlib.crc32(bytes([0xA1, 0x31, 0x00, 0x7F])) == 0x31804276
    assert_eq!(report_crc(BT_INPUT_SEED, &[0x31, 0x00, 0x7F]), 0x3180_4276);
}

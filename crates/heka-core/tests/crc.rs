use heka_core::crc::{BT_INPUT_SEED, BT_OUTPUT_SEED, crc32, report_crc};

#[test]
fn crc32_matches_the_catalogue_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn a_report_crc_covers_the_seed_byte_then_the_report() {
    assert_eq!(report_crc(BT_OUTPUT_SEED, &[]), 0xEADA_2D49);
    assert_eq!(report_crc(BT_INPUT_SEED, &[0x31, 0x00, 0x7F]), 0x3180_4276);
}

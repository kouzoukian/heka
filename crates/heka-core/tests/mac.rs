//! Controllers are identified by MAC address.

use heka_core::dualsense;
use heka_core::link::ReportError;
use heka_core::mac::MacAddress;

/// DERIVED: a pairing-info feature report laid out as `hid-playstation.c` reads it
/// (`dualsense_get_mac_address`): report ID 0x09, then the MAC in little-endian order.
/// The remaining bytes are not read. No real Controller's MAC is stored in the repo.
fn pairing_info() -> Vec<u8> {
    let mut report = vec![0u8; dualsense::PAIRING_INFO_LEN];
    report[0] = dualsense::PAIRING_INFO_REPORT_ID;
    report[1..7].copy_from_slice(&[0x66, 0x55, 0x44, 0x33, 0x22, 0x11]);
    report
}

#[test]
fn the_mac_address_is_read_from_the_pairing_info_report_in_label_order() {
    let mac = dualsense::parse_pairing_info(&pairing_info()).unwrap();
    assert_eq!(mac.to_string(), "11:22:33:44:55:66");
}

#[test]
fn a_short_or_foreign_pairing_info_report_is_refused() {
    assert_eq!(
        dualsense::parse_pairing_info(&pairing_info()[..5]),
        Err(ReportError::TooShort { len: 5 })
    );
    let mut other = pairing_info();
    other[0] = 0x05;
    assert_eq!(
        dualsense::parse_pairing_info(&other),
        Err(ReportError::UnexpectedId { id: 0x05 })
    );
}

#[test]
fn a_mac_address_round_trips_through_its_text_form() {
    let mac: MacAddress = "a1:B2:c3:D4:e5:F6".parse().unwrap();
    assert_eq!(mac.to_string(), "A1:B2:C3:D4:E5:F6");
    assert!("a1:b2:c3".parse::<MacAddress>().is_err());
    assert!("zz:b2:c3:d4:e5:f6".parse::<MacAddress>().is_err());
}

use heka_core::dualshock4::{parse_bluetooth_serial, parse_pairing_info};
use heka_core::link::{ControllerModel, ReportError, controller_model};
use heka_core::mac::{InvalidMacAddress, MacAddress};

#[test]
fn both_dualshock_4_revisions_and_both_dualsenses_are_controllers() {
    let model = |product| controller_model(0x054C, product);
    assert_eq!(model(0x05C4), Some(ControllerModel::DualShock4));
    assert_eq!(model(0x09CC), Some(ControllerModel::DualShock4));
    assert_eq!(model(0x0CE6), Some(ControllerModel::DualSense));
    assert_eq!(model(0x0DF2), Some(ControllerModel::DualSense));
    assert_eq!(model(0x0BA0), None);
    assert_eq!(controller_model(0x045E, 0x05C4), None);
}

fn pairing_info(mac: [u8; 6]) -> [u8; 16] {
    let mut report = [0u8; 16];
    report[0] = 0x12;
    report[1..7].copy_from_slice(&mac);
    report[1..7].reverse();
    report
}

const MAC: MacAddress = MacAddress([0xA4, 0xAE, 0x11, 0x02, 0x03, 0x04]);

#[test]
fn the_mac_address_is_read_from_the_pairing_info_report() {
    assert_eq!(parse_pairing_info(&pairing_info(MAC.0)), Ok(MAC));
}

#[test]
fn a_pairing_info_report_with_another_id_or_cut_short_is_refused() {
    let mut report = pairing_info(MAC.0);
    assert_eq!(
        parse_pairing_info(&report[..6]),
        Err(ReportError::TooShort { len: 6 })
    );
    report[0] = 0x09;
    assert_eq!(
        parse_pairing_info(&report),
        Err(ReportError::UnexpectedId { id: 0x09 })
    );
}

#[test]
fn the_bluetooth_serial_number_names_the_same_controller_as_usb() {
    assert_eq!(parse_bluetooth_serial("a4ae11020304"), Ok(MAC));
    assert_eq!(parse_bluetooth_serial("A4AE11020304"), Ok(MAC));
}

#[test]
fn a_serial_number_that_is_not_an_address_is_refused() {
    for serial in [
        "",
        "a4ae1102030",
        "a4ae110203045",
        "a4ae1102030g",
        "+4ae11020304",
        "a4:ae:11:02:03:04",
        "000000000000",
        "é4ae1102030",
    ] {
        assert_eq!(
            parse_bluetooth_serial(serial),
            Err(InvalidMacAddress),
            "{serial:?}"
        );
    }
}

#[test]
fn a_vigem_bus_virtual_dualshock_4_is_told_apart_by_its_pairing_info() {
    let report = [
        0x12, 0x56, 0x34, 0x12, 0x37, 0x13, 0xC0, 0x08, 0x25, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];
    let mac = parse_pairing_info(&report).unwrap();
    assert_eq!(mac, MacAddress([0xC0, 0x13, 0x37, 0x12, 0x34, 0x56]));
    assert!(mac.is_virtual_controller());
    assert!(!MAC.is_virtual_controller());
}

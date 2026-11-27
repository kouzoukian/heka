mod common;

use heka_core::calibration::Calibration;
use heka_core::dualsense::{PRODUCT_ID_DUALSENSE, PRODUCT_ID_DUALSENSE_EDGE};
use heka_core::link::{ControllerModel, LinkKind, ReportFormat};
use heka_core::ps4::Ps4Translator;
use heka_core::state::Buttons;
use heka_core::xbox::XboxReport;

const EDGE_BUTTONS: [(u8, Buttons); 4] = [
    (0x10, Buttons::LEFT_FN),
    (0x20, Buttons::RIGHT_FN),
    (0x40, Buttons::LEFT_BACK),
    (0x80, Buttons::RIGHT_BACK),
];

fn format(product_id: u16, kind: LinkKind) -> ReportFormat {
    ReportFormat::dualsense(product_id, kind, Calibration::UNCALIBRATED)
}

fn pressed(product_id: u16, kind: LinkKind, byte: u8) -> Buttons {
    let usb = common::derived(&[(10, byte)]);
    let report = match kind {
        LinkKind::Usb => usb,
        LinkKind::Bluetooth => common::bluetooth_from_usb(&usb),
    };
    format(product_id, kind).parse(&report).unwrap().buttons
}

#[test]
fn derived_edge_reports_show_each_fn_and_back_button_over_either_link() {
    for kind in [LinkKind::Usb, LinkKind::Bluetooth] {
        for (byte, expected) in EDGE_BUTTONS {
            let buttons = pressed(PRODUCT_ID_DUALSENSE_EDGE, kind, byte);

            assert_eq!(buttons, expected, "{kind:?} {byte:#04x}");
        }
    }
}

#[test]
fn derived_edge_reports_keep_the_other_buttons_of_the_same_byte() {
    for kind in [LinkKind::Usb, LinkKind::Bluetooth] {
        let buttons = pressed(PRODUCT_ID_DUALSENSE_EDGE, kind, 0xF7);

        let expected = EDGE_BUTTONS.into_iter().fold(
            Buttons::PS | Buttons::TOUCHPAD | Buttons::MUTE,
            |held, (_, button)| held | button,
        );
        assert_eq!(buttons, expected, "{kind:?}");
    }
}

#[test]
fn derived_reports_of_a_standard_dualsense_show_no_edge_button() {
    for kind in [LinkKind::Usb, LinkKind::Bluetooth] {
        let buttons = pressed(PRODUCT_ID_DUALSENSE, kind, 0xF0);

        assert_eq!(buttons, Buttons::NONE, "{kind:?}");
    }
}

#[test]
fn derived_edge_buttons_drive_nothing_on_either_virtual_controller() {
    let state = |byte| {
        format(PRODUCT_ID_DUALSENSE_EDGE, LinkKind::Usb)
            .parse(&common::derived(&[(10, byte)]))
            .unwrap()
    };
    let (released, held) = (state(0x00), state(0xF0));

    assert_eq!(
        XboxReport::from_state(&held),
        XboxReport::from_state(&released)
    );
    assert_eq!(
        Ps4Translator::default().report(&held),
        Ps4Translator::default().report(&released)
    );
}

#[test]
fn either_dualsense_is_a_dualsense_on_the_link_it_was_found_on() {
    for product_id in [PRODUCT_ID_DUALSENSE, PRODUCT_ID_DUALSENSE_EDGE] {
        for kind in [LinkKind::Usb, LinkKind::Bluetooth] {
            let format = format(product_id, kind);

            assert_eq!(format.model(), ControllerModel::DualSense);
            assert_eq!(format.kind(), kind);
        }
    }
}

use heka_core::link::{LinkKind, active_link};

use LinkKind::{Bluetooth, Usb};

#[test]
fn usb_wins_over_bluetooth_whatever_the_order() {
    assert_eq!(active_link(&[Usb, Bluetooth]), Some(0));
    assert_eq!(active_link(&[Bluetooth, Usb]), Some(1));
}

#[test]
fn among_links_of_one_kind_the_oldest_is_read() {
    assert_eq!(active_link(&[Bluetooth, Bluetooth]), Some(0));
    assert_eq!(active_link(&[Bluetooth, Usb, Usb]), Some(1));
}

#[test]
fn a_controller_without_links_has_no_active_link() {
    assert_eq!(active_link(&[]), None);
}

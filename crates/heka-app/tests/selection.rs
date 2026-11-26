//! Which Controller's pane shows.

use heka_app::ui::model::{ControllerEntry, EntryState, Selection};
use heka_core::mac::MacAddress;
use heka_core::virtual_controller::OutputType;

fn mac(last: u8) -> MacAddress {
    MacAddress([0x02, 0, 0, 0, 0, last])
}

/// The controllers of the Controllers ending in `macs`, in that order.
fn controllers(macs: &[u8]) -> Vec<ControllerEntry> {
    macs.iter()
        .map(|&last| ControllerEntry {
            mac: mac(last),
            name: "DualSense",
            experimental: false,
            active_link: None,
            battery: None,
            output_type: OutputType::Xbox,
            state: EntryState::Stopped,
            bus_missing: false,
            error: None,
            pad: None,
        })
        .collect()
}

/// The selection after following each list of Controllers in turn.
fn after(selection: &mut Selection, lists: &[&[u8]]) -> Option<MacAddress> {
    let mut shown = None;
    for list in lists {
        let controllers = controllers(list);
        selection.follow(&controllers);
        shown = selection.shown(&controllers).map(|entry| entry.mac);
    }
    shown
}

#[test]
fn the_first_controller_is_selected_at_the_start() {
    assert_eq!(
        after(&mut Selection::default(), &[&[1, 2, 3]]),
        Some(mac(1))
    );
}

#[test]
fn nothing_is_selected_without_a_controller() {
    assert_eq!(after(&mut Selection::default(), &[&[]]), None);
}

#[test]
fn the_selection_follows_the_chosen_controller_wherever_it_moves() {
    let mut selection = Selection::default();
    selection.choose(mac(2), &controllers(&[1, 2, 3]));

    assert_eq!(after(&mut selection, &[&[2, 3]]), Some(mac(2)));
    assert_eq!(after(&mut selection, &[&[4, 3, 2]]), Some(mac(2)));
}

#[test]
fn a_new_controller_does_not_take_the_selection() {
    assert_eq!(
        after(&mut Selection::default(), &[&[1], &[2, 1]]),
        Some(mac(1))
    );
}

#[test]
fn when_the_selected_controller_leaves_the_one_in_its_place_is_selected() {
    let mut selection = Selection::default();
    selection.choose(mac(2), &controllers(&[1, 2, 3]));

    assert_eq!(after(&mut selection, &[&[1, 3]]), Some(mac(3)));
    // The last one left: the new last one.
    assert_eq!(after(&mut selection, &[&[1]]), Some(mac(1)));
}

#[test]
fn after_every_controller_left_the_first_to_return_is_selected() {
    let mut selection = Selection::default();
    selection.choose(mac(3), &controllers(&[1, 2, 3]));

    assert_eq!(after(&mut selection, &[&[], &[5, 6]]), Some(mac(5)));
}

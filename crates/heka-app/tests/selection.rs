use heka_app::ui::model::{ControllerEntry, EntryState, Selection};
use heka_app::ui::view::Page;
use heka_core::mac::MacAddress;
use heka_core::preferences::Preferences;
use heka_core::virtual_controller::OutputType;

fn mac(last: u8) -> MacAddress {
    MacAddress([0x02, 0, 0, 0, 0, last])
}

fn controllers(macs: &[u8]) -> Vec<ControllerEntry> {
    macs.iter()
        .map(|&last| ControllerEntry {
            mac: mac(last),
            name: "DualSense",
            nickname: None,
            has_player_lights: true,
            preferences: Preferences::default(),
            experimental: false,
            active_link: None,
            has_bluetooth: false,
            idle_time: None,
            battery: None,
            raw_sticks: None,
            raw_triggers: None,
            raw_touch: None,
            report_rate: None,
            output_type: OutputType::Xbox,
            profile: None,
            applied_profile: None,
            rule_program: None,
            output_type_from_profile: false,
            state: EntryState::Stopped,
            bus_missing: false,
            error: None,
            pad: None,
        })
        .collect()
}

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
    assert_eq!(after(&mut selection, &[&[1]]), Some(mac(1)));
}

#[test]
fn after_every_controller_left_the_first_to_return_is_selected() {
    let mut selection = Selection::default();
    selection.choose(mac(3), &controllers(&[1, 2, 3]));

    assert_eq!(after(&mut selection, &[&[], &[5, 6]]), Some(mac(5)));
}

#[test]
fn a_status_bar_button_opens_its_page_and_closes_it_again() {
    assert_eq!(Page::Controller.toggled(Page::Log), Page::Log);
    assert_eq!(Page::Log.toggled(Page::Log), Page::Controller);
    assert_eq!(Page::Log.toggled(Page::About), Page::About);
    assert_eq!(
        Page::AppSettings.toggled(Page::AppSettings),
        Page::Controller
    );
}

#[test]
fn choosing_a_controller_leaves_the_pages_about_heka_and_keeps_its_settings() {
    assert_eq!(Page::Log.of_a_controller(), Page::Controller);
    assert_eq!(Page::About.of_a_controller(), Page::Controller);
    assert_eq!(Page::AppSettings.of_a_controller(), Page::Controller);
    assert_eq!(
        Page::ControllerSettings.of_a_controller(),
        Page::ControllerSettings
    );
}

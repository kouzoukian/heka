use heka_core::live_mapper::LiveMapper;
use heka_core::profile::{Binding, Input, Mapper, Output, Profile};
use heka_core::state::{Buttons, ControllerState};

fn cross() -> ControllerState {
    ControllerState {
        buttons: Buttons::CROSS,
        ..ControllerState::NEUTRAL
    }
}

fn circle_for_cross() -> Mapper {
    let mut profile = Profile::default();
    profile.bind(Input::Cross, Binding::To(Output::Circle));
    Mapper::new(&profile)
}

#[test]
fn a_new_live_mapper_leaves_states_as_they_are() {
    let live = LiveMapper::default();

    assert_eq!(live.follow().map(&cross()), cross());
}

#[test]
fn a_follower_sees_a_mapper_set_before_it_started() {
    let live = LiveMapper::default();
    live.set(circle_for_cross());

    assert_eq!(live.follow().map(&cross()).buttons, Buttons::CIRCLE);
}

#[test]
fn a_follower_picks_up_a_change_on_its_next_state() {
    let live = LiveMapper::default();
    let mut follower = live.follow();
    assert_eq!(follower.map(&cross()).buttons, Buttons::CROSS);

    live.set(circle_for_cross());
    assert_eq!(follower.map(&cross()).buttons, Buttons::CIRCLE);

    live.set(Mapper::default());
    assert_eq!(follower.map(&cross()).buttons, Buttons::CROSS);
}

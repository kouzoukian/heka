use heka_core::live_mapper::{FollowedMapper, LiveMapper};
use heka_core::profile::{Binding, Input, Mapper, Output, Profile};
use heka_core::state::{Buttons, ControllerState};

fn cross() -> ControllerState {
    ControllerState {
        buttons: Buttons::CROSS,
        ..ControllerState::NEUTRAL
    }
}

fn shown(follower: &mut FollowedMapper<'_>) -> ControllerState {
    follower.refresh();
    follower.current().map(&cross())
}

fn circle_for_cross() -> Mapper {
    let mut profile = Profile::default();
    profile.bind(Input::Cross, Binding::To(Output::Circle));
    Mapper::new(&profile)
}

#[test]
fn a_new_live_mapper_leaves_states_as_they_are() {
    let live = LiveMapper::default();

    assert_eq!(shown(&mut live.follow()), cross());
}

#[test]
fn a_follower_sees_a_mapper_set_before_it_started() {
    let live = LiveMapper::default();
    live.set(circle_for_cross());

    assert_eq!(shown(&mut live.follow()).buttons, Buttons::CIRCLE);
}

#[test]
fn a_follower_picks_up_a_change_on_its_next_state() {
    let live = LiveMapper::default();
    let mut follower = live.follow();
    assert_eq!(shown(&mut follower).buttons, Buttons::CROSS);

    live.set(circle_for_cross());
    assert_eq!(shown(&mut follower).buttons, Buttons::CIRCLE);

    live.set(Mapper::default());
    assert_eq!(shown(&mut follower).buttons, Buttons::CROSS);
}

#[test]
fn only_a_mapper_that_differs_counts_as_a_change() {
    let live = LiveMapper::default();
    let mut follower = live.follow();

    live.set(Mapper::default());
    assert!(!follower.refresh());

    live.set(circle_for_cross());
    assert!(follower.refresh());
    assert!(!follower.refresh());
}

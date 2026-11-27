use heka_app::text;
use heka_app::ui::triggers::{
    HAIR_POINTS, PRESS_POINTS, point_choices, point_share, readout, share,
};
use heka_core::tuning::TriggerSide;

#[test]
fn a_bar_is_empty_at_rest_and_full_at_full_travel() {
    assert_eq!(share(0), 0.);
    assert_eq!(share(255), 1.);
    assert!((share(51) - 0.2).abs() < 1e-6);
}

#[test]
fn a_point_is_a_share_of_the_bar() {
    assert_eq!(point_share(100), 1.);
    assert!((point_share(25) - 0.25).abs() < 1e-6);
}

#[test]
fn no_point_offers_just_the_presets_and_a_point_outside_them_is_offered_too() {
    assert_eq!(point_choices(&HAIR_POINTS, None), HAIR_POINTS);
    assert_eq!(point_choices(&HAIR_POINTS, Some(30)), HAIR_POINTS);
    assert_eq!(
        point_choices(&HAIR_POINTS, Some(25)),
        [10, 20, 25, 30, 50, 70]
    );
    assert_eq!(
        point_choices(&PRESS_POINTS, Some(90)),
        [20, 30, 40, 50, 60, 70, 80, 90]
    );
}

#[test]
fn the_presets_are_valid_points_in_order() {
    for presets in [&HAIR_POINTS[..], &PRESS_POINTS[..]] {
        assert!(presets.iter().all(|&point| (1..=100).contains(&point)));
        assert!(presets.windows(2).all(|pair| pair[0] < pair[1]));
    }
}

#[test]
fn the_readout_shows_the_chosen_triggers_own_travel_and_the_tuned_one() {
    let raw = Some([40, 200]);
    let shown = Some((60, 220));

    let left = readout(raw, shown, TriggerSide::Left);
    let right = readout(raw, shown, TriggerSide::Right);

    assert_eq!((left.raw, left.tuned), (40, 60));
    assert_eq!((right.raw, right.tuned), (200, 220));
}

#[test]
fn without_a_controller_the_bars_are_empty() {
    let nothing = readout(None, None, TriggerSide::Left);

    assert_eq!((nothing.raw, nothing.tuned), (0, 0));
}

#[test]
fn a_travel_is_written_as_a_whole_percent() {
    assert_eq!(text::travel_percent(0), "0%");
    assert_eq!(text::travel_percent(128), "50%");
    assert_eq!(text::travel_percent(255), "100%");
}

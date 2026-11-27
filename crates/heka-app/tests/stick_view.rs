use heka_app::ui::sticks::{
    ANTI_DEAD_ZONES, DEAD_ZONES, MAX_ZONES, SENSITIVITIES, choices, plot_offset, readout,
};
use heka_core::state::Stick;
use heka_core::tuning::{MAX_DEAD_ZONE, MAX_SENSITIVITY, MIN_MAX_ZONE, MIN_SENSITIVITY, StickSide};

const CENTRE: Stick = Stick::CENTER;

fn stick(x: u8, y: u8) -> Stick {
    Stick { x, y }
}

#[test]
fn a_value_outside_the_presets_is_offered_too() {
    assert_eq!(choices(&DEAD_ZONES, 12), [0, 5, 10, 12, 15, 20, 30]);
    assert_eq!(choices(&MAX_ZONES, 85), [70, 80, 85, 90, 95, 100]);
}

#[test]
fn a_preset_value_is_not_offered_twice() {
    assert_eq!(choices(&DEAD_ZONES, 10), DEAD_ZONES);
    assert_eq!(choices(&SENSITIVITIES, 100), SENSITIVITIES);
}

#[test]
fn the_presets_stay_within_what_the_tuning_accepts() {
    assert!(
        DEAD_ZONES
            .iter()
            .all(|&zone| zone <= u16::from(MAX_DEAD_ZONE))
    );
    assert!(ANTI_DEAD_ZONES.iter().all(|&zone| zone <= 50));
    assert!(
        MAX_ZONES
            .iter()
            .all(|&zone| zone >= u16::from(MIN_MAX_ZONE) && zone <= 100)
    );
    assert!(
        SENSITIVITIES
            .iter()
            .all(|&value| (MIN_SENSITIVITY..=MAX_SENSITIVITY).contains(&value))
    );
}

#[test]
fn the_neutral_values_are_among_the_presets() {
    assert!(DEAD_ZONES.contains(&0));
    assert!(ANTI_DEAD_ZONES.contains(&0));
    assert!(MAX_ZONES.contains(&100));
    assert!(SENSITIVITIES.contains(&100));
}

#[test]
fn the_plot_puts_the_centre_in_the_middle_and_the_edges_on_the_circle() {
    assert_eq!(plot_offset(CENTRE), (0., 0.));
    assert_eq!(plot_offset(stick(255, 128)), (1., 0.));
    assert_eq!(plot_offset(stick(0, 128)), (-1., 0.));
    assert_eq!(plot_offset(stick(128, 255)), (0., 1.));
    assert_eq!(plot_offset(stick(128, 0)), (0., -1.));
}

#[test]
fn the_readout_shows_the_controllers_own_stick_and_the_tuned_one() {
    let raw = [stick(10, 20), stick(200, 210)];
    let shown = (stick(30, 40), stick(220, 230));

    let left = readout(Some(raw), Some(shown), StickSide::Left, false);
    let right = readout(Some(raw), Some(shown), StickSide::Right, false);

    assert_eq!((left.raw, left.tuned), (stick(10, 20), stick(30, 40)));
    assert_eq!((right.raw, right.tuned), (stick(200, 210), stick(220, 230)));
}

#[test]
fn with_the_sticks_swapped_a_stick_is_tuned_into_the_other_side() {
    let raw = [stick(10, 20), stick(200, 210)];
    let shown = (stick(30, 40), stick(220, 230));

    let left = readout(Some(raw), Some(shown), StickSide::Left, true);

    assert_eq!(left.raw, stick(10, 20));
    assert_eq!(left.tuned, stick(220, 230));
}

#[test]
fn without_a_controller_both_dots_rest_at_the_centre() {
    let nothing = readout(None, None, StickSide::Left, false);

    assert_eq!((nothing.raw, nothing.tuned), (CENTRE, CENTRE));
}

//! How large the live drawing is in the room it gets.

use heka_app::ui::drawing::fitted_width;

#[test]
fn the_drawing_is_as_wide_as_its_room_allows_with_a_margin() {
    // The width decides, leaving 20 on each side.
    assert_eq!(fitted_width(500., 900.), 460.);
    // The height decides, leaving 12 above and below: 276 high is 414 wide.
    assert_eq!(fitted_width(900., 300.), 414.);
}

#[test]
fn the_drawing_stays_between_360_and_560_wide() {
    assert_eq!(fitted_width(2000., 2000.), 560.);
    assert_eq!(fitted_width(400., 264.), 360.);
    assert_eq!(fitted_width(100., 100.), 360.);
}

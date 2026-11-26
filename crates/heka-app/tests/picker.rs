//! Lightbar colours in and out of the window's picker.

use heka_app::ui::picker::{drawn, picked};
use heka_core::feedback::Lightbar;

#[test]
fn a_colour_comes_back_from_the_picker_as_it_went_in() {
    // Every 5th step of each channel, both ends included.
    let steps = || (0..=255u8).step_by(5);
    for red in steps() {
        for green in steps() {
            for blue in steps() {
                let colour = Lightbar { red, green, blue };
                assert_eq!(picked(drawn(colour)), colour);
            }
        }
    }
}

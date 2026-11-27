use heka_app::ui::picker::{drawn, picked, typed};
use heka_core::feedback::Lightbar;

const TYPED: Lightbar = Lightbar {
    red: 0x12,
    green: 0xAB,
    blue: 0x9F,
};

#[test]
fn a_colour_comes_back_from_the_picker_as_it_went_in() {
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

#[test]
fn a_whole_typed_hex_colour_is_taken_in_either_case() {
    assert_eq!(typed("#12AB9F"), Some(TYPED));
    assert_eq!(typed("#12ab9f"), Some(TYPED));
}

#[test]
fn a_typed_transparency_is_dropped() {
    assert_eq!(typed("#12AB9F80"), Some(TYPED));
}

#[test]
fn a_colour_still_being_typed_is_not_taken() {
    for text in ["", "#", "#1", "#12", "#12A", "#12AB", "#12AB9", "#12AB9F8"] {
        assert_eq!(typed(text), None, "{text}");
    }
}

#[test]
fn text_that_is_not_a_hex_colour_is_not_taken() {
    for text in [
        "12AB9F",
        "12AB9F80",
        "#12AB9G",
        "#+1+2+3",
        "#12AB9F8G",
        "#12AB9F801",
        "##12AB9",
        "#12ABé",
    ] {
        assert_eq!(typed(text), None, "{text}");
    }
}

use gpui::{Bounds, Hsla, PaintQuad, Path, Pixels, point, px};
use heka_app::log;
use heka_app::ui::drawing::{Palette, Surface, fitted_width, paint};
use heka_app::ui::pad::{PadModel, TouchDot};
use heka_core::state::{Buttons, Stick};
use heka_core::virtual_controller::OutputType;

#[test]
fn the_drawing_is_as_wide_as_its_room_allows_with_a_margin() {
    assert_eq!(fitted_width(500., 900.), 460.);
    assert_eq!(fitted_width(900., 300.), 414.);
}

#[test]
fn the_drawing_stays_between_360_and_560_wide() {
    assert_eq!(fitted_width(2000., 2000.), 560.);
    assert_eq!(fitted_width(400., 264.), 360.);
    assert_eq!(fitted_width(100., 100.), 360.);
}

#[derive(Default)]
struct Recorded {
    shapes: usize,
    unbuildable: Vec<String>,
}

impl Surface for Recorded {
    fn path(&mut self, _: Path, _: Hsla) {
        self.shapes += 1;
    }

    fn quad(&mut self, _: PaintQuad) {}

    fn clipped(&mut self, _: Bounds<Pixels>, paint: &mut dyn FnMut(&mut dyn Surface)) {
        paint(self);
    }

    fn unbuildable(&mut self, error: String) {
        self.unbuildable.push(error);
    }
}

fn recorded(output_type: OutputType, pad: &PadModel, scale: f32) -> Recorded {
    let mut surface = Recorded::default();
    let origin = point(px(0.), px(0.));
    paint(
        &mut surface,
        origin,
        scale,
        Palette::default(),
        output_type,
        pad,
    );
    surface
}

fn everything_held() -> PadModel {
    let buttons = Buttons::NAMED
        .iter()
        .fold(Buttons::NONE, |mut held, &(button, _)| {
            held |= button;
            held
        });
    PadModel {
        buttons,
        left_stick: Stick { x: 0, y: 255 },
        right_stick: Stick { x: 255, y: 0 },
        left_trigger: 255,
        right_trigger: 1,
        touch: [
            Some(TouchDot::from_units(0, 0, 1919, 1079)),
            Some(TouchDot::from_units(1919, 1079, 1919, 1079)),
        ],
    }
}

#[test]
fn every_shape_of_either_pad_can_be_built() {
    for output_type in [OutputType::Xbox, OutputType::Ps4] {
        for pad in [PadModel::NEUTRAL, everything_held()] {
            for scale in [1., 560. / 360.] {
                let recorded = recorded(output_type, &pad, scale);

                assert_eq!(
                    recorded.unbuildable,
                    Vec::<String>::new(),
                    "{output_type:?}"
                );
                assert!(recorded.shapes > 20, "{output_type:?}");
            }
        }
    }
}

#[test]
fn a_repeated_error_is_logged_only_the_first_time() {
    let errors = log::Once::new();

    assert!(errors.first("one"));
    assert!(!errors.first("one"));
    assert!(errors.first("two"));
}

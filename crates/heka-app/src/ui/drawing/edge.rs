use heka_core::state::Buttons;

use super::WIDTH;
use super::shape::{Label, Painter, Palette, Xy};
use crate::text;
use crate::ui::pad::PadModel;

const FN_FROM_STICK: Xy = (32., 20.);
const FN_SIZE: Xy = (18., 10.);
const BACK_FROM_MIDDLE: Xy = (140., 191.);
const BACK_SIZE: Xy = (12., 30.);

struct Mark {
    button: Buttons,
    centre: Xy,
    size: Xy,
    text: Option<&'static str>,
}

fn held_marks(pad: &PadModel, sticks: [Xy; 2]) -> impl Iterator<Item = Mark> {
    let [left, right] = sticks;
    let function = |button, (x, y): Xy, outwards: f32| Mark {
        button,
        centre: (x + outwards * FN_FROM_STICK.0, y + FN_FROM_STICK.1),
        size: FN_SIZE,
        text: Some(text::FN),
    };
    let back = |button, outwards: f32| Mark {
        button,
        centre: (
            WIDTH / 2. + outwards * BACK_FROM_MIDDLE.0,
            BACK_FROM_MIDDLE.1,
        ),
        size: BACK_SIZE,
        text: None,
    };
    [
        function(Buttons::LEFT_FN, left, -1.),
        function(Buttons::RIGHT_FN, right, 1.),
        back(Buttons::LEFT_BACK, -1.),
        back(Buttons::RIGHT_BACK, 1.),
    ]
    .into_iter()
    .filter(move |mark| pad.pressed(mark.button))
}

pub fn paint(painter: &mut Painter, pad: &PadModel, sticks: [Xy; 2]) {
    let lit = painter.palette.pressed;
    for Mark {
        centre: (x, y),
        size: (width, height),
        ..
    } in held_marks(pad, sticks)
    {
        let place = (x - width / 2., y - height / 2., width, height);
        painter.rect(place, width.min(height) / 2., lit, (1., lit));
    }
}

pub fn labels(pad: &PadModel, palette: &Palette, sticks: [Xy; 2]) -> impl Iterator<Item = Label> {
    let color = palette.pressed_label;
    held_marks(pad, sticks).filter_map(move |mark| {
        mark.text.map(|text| Label {
            at: mark.centre,
            text,
            color,
        })
    })
}

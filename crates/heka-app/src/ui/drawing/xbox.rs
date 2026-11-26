//! The Xbox Output Type, drawn as an Xbox pad. Its buttons are lettered and placed
//! as the Virtual Controller maps them ([`heka_core::xbox`]).

use gpui::{Hsla, transparent_black};
use heka_core::state::Buttons;

use super::shape::{Label, Painter, Palette, Shape, Xy};
use crate::text;
use crate::ui::pad::PadModel;

const GUIDE: Xy = (180., 66.);
const VIEW: Xy = (157., 94.);
const MENU: Xy = (203., 94.);
const LEFT_STICK: Xy = (88., 92.);
const DPAD: Xy = (132., 146.);
const RIGHT_STICK: Xy = (228., 146.);
const FACE: Xy = (272., 92.);
const FACE_SPREAD: f32 = 20.;

/// The face buttons: the Controller button, where it sits and its letter.
const FACE_BUTTONS: [(Buttons, Xy, &str); 4] = [
    (Buttons::TRIANGLE, (0., -1.), text::Y),
    (Buttons::CIRCLE, (1., 0.), text::B),
    (Buttons::CROSS, (0., 1.), text::A),
    (Buttons::SQUARE, (-1., 0.), text::X),
];

pub fn paint(painter: &mut Painter, pad: &PadModel) {
    let palette = painter.palette;
    // Its lower edge is the middle of the bumper's upper one, 2 above it, so
    // that it leans as the shoulder does.
    let left_trigger = Shape::new(
        (66., 27.1),
        &[
            ((63., 15.5), (73., 4.), (89., 4.)),
            ((104., 4.), (112., 14.), (113.8, 26.3)),
            ((107., 24.), (99., 23.), (90., 23.)),
            ((80., 23.), (72., 24.), (66., 27.1)),
        ],
    );
    painter.trigger(&left_trigger, pad.left_trigger, pad.pressed(Buttons::L2));
    painter.trigger(
        &left_trigger.mirrored(),
        pad.right_trigger,
        pad.pressed(Buttons::R2),
    );
    // Its lower edge follows the body's upper one.
    let left_bumper = Shape::new(
        (54., 46.),
        &[
            ((58., 29.), (70., 25.), (90., 25.)),
            ((108., 25.), (122., 29.), (130., 39.1)),
            ((119.1, 36.), (108., 33.7), (94., 34.4)),
            ((77.6, 34.9), (64.1, 39.1), (54., 46.)),
        ],
    );
    painter.control(&left_bumper, pad.pressed(Buttons::L1));
    painter.control(&left_bumper.mirrored(), pad.pressed(Buttons::R1));

    let body = Shape::symmetric(
        (180., 46.),
        &[
            ((196., 46.), (206., 46.), (216., 45.)),
            ((232., 40.), (246., 35.), (266., 36.)),
            ((296., 37.), (316., 50.), (324., 70.)),
            ((340., 108.), (355., 158.), (355., 192.)),
            ((355., 220.), (343., 236.), (325., 236.)),
            ((306., 236.), (294., 218.), (278., 200.)),
            ((264., 184.), (252., 176.), (232., 176.)),
            ((214., 176.), (196., 176.), (180., 176.)),
        ],
    );
    painter.fill(&body, palette.body);
    painter.stroke(&body, 1.2, palette.outline);

    guide(painter, pad.pressed(Buttons::PS));
    view_and_menu(painter, pad);
    // The share button, which a Virtual Controller does not have.
    painter.rect(
        (172., 108., 16., 8.),
        4.,
        palette.control,
        (1., palette.outline),
    );

    painter.stick(LEFT_STICK, pad.left_stick, pad.pressed(Buttons::L3));
    dpad(painter, pad);
    painter.stick(RIGHT_STICK, pad.right_stick, pad.pressed(Buttons::R3));

    let (cx, cy) = FACE;
    for (button, (dx, dy), _) in FACE_BUTTONS {
        let centre = (cx + dx * FACE_SPREAD, cy + dy * FACE_SPREAD);
        let color = face_color(button, &palette);
        painter.button(centre, 10.5, pad.pressed(button), color);
    }
}

pub fn labels(pad: &PadModel, palette: &Palette) -> Vec<Label> {
    let trigger = |travel: u8| {
        if travel > 0 {
            palette.bright
        } else {
            palette.label
        }
    };
    let label = |at: Xy, text: &'static str, color: Hsla| Label { at, text, color };
    let mut labels = vec![
        label((89., 13.5), text::LT, trigger(pad.left_trigger)),
        label((271., 13.5), text::RT, trigger(pad.right_trigger)),
        label((90., 30.), text::LB, palette.on(pad.pressed(Buttons::L1))),
        label((270., 30.), text::RB, palette.on(pad.pressed(Buttons::R1))),
    ];
    let (cx, cy) = FACE;
    labels.extend(FACE_BUTTONS.map(|(button, (dx, dy), letter)| {
        let color = if pad.pressed(button) {
            palette.pressed_label
        } else {
            face_color(button, palette)
        };
        label(
            (cx + dx * FACE_SPREAD, cy + dy * FACE_SPREAD),
            letter,
            color,
        )
    }));
    labels
}

/// The colour of the Xbox button that `button` drives.
fn face_color(button: Buttons, palette: &Palette) -> Hsla {
    match button {
        Buttons::TRIANGLE => palette.yellow,
        Buttons::CIRCLE => palette.red,
        Buttons::CROSS => palette.green,
        _ => palette.blue,
    }
}

/// The round button with the cross-shaped mark.
fn guide(painter: &mut Painter, pressed: bool) {
    let palette = painter.palette;
    painter.button(GUIDE, 12., pressed, palette.pressed);
    let ink = if pressed {
        palette.pressed_label
    } else {
        palette.label
    };
    let (x, y) = GUIDE;
    painter.line((x - 6., y - 6.), (x + 6., y + 6.), 2., ink);
    painter.line((x + 6., y - 6.), (x - 6., y + 6.), 2., ink);
}

/// The two small buttons: View with two windows on it, Menu with three lines.
fn view_and_menu(painter: &mut Painter, pad: &PadModel) {
    let palette = painter.palette;
    for (centre, button) in [(VIEW, Buttons::CREATE), (MENU, Buttons::OPTIONS)] {
        painter.button(centre, 8., pad.pressed(button), palette.pressed);
    }

    let ink = palette.on(pad.pressed(Buttons::CREATE));
    let (x, y) = VIEW;
    for (left, top) in [(x - 4., y - 4.), (x - 1.5, y - 1.5)] {
        painter.rect((left, top, 5.5, 5.5), 1., transparent_black(), (1., ink));
    }

    let ink = palette.on(pad.pressed(Buttons::OPTIONS));
    let (x, y) = MENU;
    for dy in [-3., 0., 3.] {
        painter.line((x - 4., y + dy), (x + 4., y + dy), 1., ink);
    }
}

/// A cross of four arms that meet in the middle.
fn dpad(painter: &mut Painter, pad: &PadModel) {
    painter.well(DPAD, 27.);
    // The up arm around the d-pad's middle; the others are it turned.
    let arm = Shape::polygon(&[
        (0., 0.),
        (-7., -7.),
        (-7., -20.),
        (-5., -22.),
        (5., -22.),
        (7., -20.),
        (7., -7.),
        (0., 0.),
    ]);
    painter.dpad(DPAD, &arm, pad.buttons);
}

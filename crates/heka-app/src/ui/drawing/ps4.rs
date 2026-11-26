use gpui::{Hsla, transparent_black};
use heka_core::state::Buttons;

use super::shape::{Label, Painter, Palette, Shape, Xy};
use crate::text;
use crate::ui::pad::PadModel;

const TOUCHPAD: (f32, f32, f32, f32) = (126., 56., 108., 54.);
const FINGER: f32 = 12.;
const SHARE: (f32, f32, f32, f32) = (113.5, 56., 8., 17.);
const OPTIONS: (f32, f32, f32, f32) = (238.5, 56., 8., 17.);
const PS: Xy = (180., 146.);
const DPAD: Xy = (78., 94.);
const FACE: Xy = (282., 94.);
const FACE_SPREAD: f32 = 20.;
const LEFT_STICK: Xy = (132., 144.);
const RIGHT_STICK: Xy = (228., 144.);

pub fn paint(painter: &mut Painter, pad: &PadModel) {
    let palette = painter.palette;
    let left_trigger = Shape::new(
        (58.3, 31.1),
        &[
            ((54.5, 20.), (63., 5.), (81., 5.)),
            ((99., 5.), (107.5, 20.), (103.7, 31.5)),
            ((98.7, 28.9), (90.9, 28.), (81., 28.)),
            ((71.1, 28.), (63.3, 28.9), (58.3, 31.1)),
        ],
    );
    painter.trigger(&left_trigger, pad.left_trigger, pad.pressed(Buttons::L2));
    painter.trigger(
        &left_trigger.mirrored(),
        pad.right_trigger,
        pad.pressed(Buttons::R2),
    );
    let left_bumper = Shape::new(
        (52., 41.4),
        &[
            ((52., 33.), (63., 30.), (81., 30.)),
            ((99., 30.), (110., 33.), (110., 43.4)),
            ((102.2, 40.2), (91.5, 38.4), (76., 38.4)),
            ((66.6, 38.4), (58.7, 39.4), (52., 41.4)),
        ],
    );
    painter.control(&left_bumper, pad.pressed(Buttons::L1));
    painter.control(&left_bumper.mirrored(), pad.pressed(Buttons::R1));

    let body = Shape::symmetric(
        (180., 48.),
        &[
            ((210., 48.), (232., 48.), (244., 48.)),
            ((252., 43.), (264., 40.), (284., 40.)),
            ((310., 40.), (325., 48.), (332., 62.)),
            ((345., 90.), (358., 148.), (356., 192.)),
            ((355., 220.), (344., 236.), (328., 236.)),
            ((312., 236.), (303., 222.), (294., 202.)),
            ((285., 184.), (274., 174.), (258., 174.)),
            ((240., 180.), (222., 182.), (208., 176.)),
            ((200., 172.), (190., 172.), (180., 172.)),
        ],
    );
    painter.fill(&body, palette.body);
    painter.stroke(&body, 1.2, palette.outline);

    painter.rect(
        (156., 50.5, 48., 2.5),
        1.25,
        palette.pressed.opacity(0.7),
        (0., transparent_black()),
    );
    touchpad(painter, pad);

    for (place, button) in [(SHARE, Buttons::CREATE), (OPTIONS, Buttons::OPTIONS)] {
        let pressed = pad.pressed(button);
        let (fill, edge) = if pressed {
            (palette.pressed, palette.pressed)
        } else {
            (palette.control, palette.outline)
        };
        painter.rect(place, 4., fill, (1., edge));
    }

    dpad(painter, pad);
    face_buttons(painter, pad);

    for (row, count) in [(119., 5), (125., 4)] {
        for hole in 0..count {
            let x = 180. + (hole as f32 - (count - 1) as f32 / 2.) * 6.;
            painter.circle((x, row), 1.4, palette.outline, (0., transparent_black()));
        }
    }

    painter.stick(LEFT_STICK, pad.left_stick, pad.pressed(Buttons::L3));
    painter.stick(RIGHT_STICK, pad.right_stick, pad.pressed(Buttons::R3));
    painter.button(PS, 9.5, pad.pressed(Buttons::PS), palette.pressed);
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
    vec![
        label((81., 16.5), text::L2, trigger(pad.left_trigger)),
        label((279., 16.5), text::R2, trigger(pad.right_trigger)),
        label((81., 34.), text::L1, palette.on(pad.pressed(Buttons::L1))),
        label((279., 34.), text::R1, palette.on(pad.pressed(Buttons::R1))),
        label(PS, text::PS, palette.on(pad.pressed(Buttons::PS))),
    ]
}

fn touchpad(painter: &mut Painter, pad: &PadModel) {
    let palette = painter.palette;
    let (left, top, width, height) = TOUCHPAD;
    let (fill, edge) = if pad.pressed(Buttons::TOUCHPAD) {
        (palette.pressed.opacity(0.3), palette.pressed)
    } else {
        (palette.control, palette.outline)
    };
    painter.rect(TOUCHPAD, 7., fill, (1., edge));
    for finger in pad.touch.into_iter().flatten() {
        let (x, y) = finger.share();
        let centre = (
            left + 1. + FINGER / 2. + x * (width - FINGER - 2.),
            top + 1. + FINGER / 2. + y * (height - FINGER - 2.),
        );
        painter.circle(
            centre,
            FINGER / 2.,
            palette.pressed,
            (0., transparent_black()),
        );
    }
}

fn dpad(painter: &mut Painter, pad: &PadModel) {
    painter.well(DPAD, 31.);
    let key = Shape::polygon(&[
        (-8., -23.),
        (-5., -26.),
        (5., -26.),
        (8., -23.),
        (8., -13.),
        (0., -5.),
        (-8., -13.),
        (-8., -23.),
    ]);
    painter.dpad(DPAD, &key, pad.buttons);
}

fn face_buttons(painter: &mut Painter, pad: &PadModel) {
    let palette = painter.palette;
    painter.well(FACE, 33.);
    let (cx, cy) = FACE;
    for (button, (dx, dy), color) in [
        (Buttons::TRIANGLE, (0., -1.), palette.green),
        (Buttons::CIRCLE, (1., 0.), palette.red),
        (Buttons::CROSS, (0., 1.), palette.blue),
        (Buttons::SQUARE, (-1., 0.), palette.magenta),
    ] {
        let (x, y) = (cx + dx * FACE_SPREAD, cy + dy * FACE_SPREAD);
        let pressed = pad.pressed(button);
        painter.button((x, y), 10., pressed, color);
        let ink = if pressed {
            palette.pressed_label
        } else {
            color
        };
        match button {
            Buttons::TRIANGLE => painter.stroke(
                &Shape::polygon(&[
                    (x, y - 5.5),
                    (x + 5., y + 3.),
                    (x - 5., y + 3.),
                    (x, y - 5.5),
                ]),
                1.4,
                ink,
            ),
            Buttons::CIRCLE => painter.circle((x, y), 5., transparent_black(), (1.4, ink)),
            Buttons::CROSS => {
                painter.line((x - 4., y - 4.), (x + 4., y + 4.), 1.4, ink);
                painter.line((x + 4., y - 4.), (x - 4., y + 4.), 1.4, ink);
            }
            _ => painter.rect(
                (x - 4.5, y - 4.5, 9., 9.),
                0.5,
                transparent_black(),
                (1.4, ink),
            ),
        }
    }
}

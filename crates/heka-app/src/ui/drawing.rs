//! The live drawing of a Controller: buttons, sticks, triggers and touchpad.
//!
//! A fixed-size picture with every control placed on it. Positions are in pixels
//! from the drawing's top left corner.

use gpui::{
    App, Bounds, Div, Hsla, IntoElement, ParentElement, PathBuilder, Pixels, Point, Styled, Window,
    canvas, div, point, prelude::FluentBuilder as _, px, relative,
};
use gpui_component::{ActiveTheme, Theme};
use heka_core::state::{Buttons, Stick};

use crate::text;
use crate::ui::pad::{PadModel, stick_offset, trigger_fill};

pub const WIDTH: f32 = 360.;
pub const HEIGHT: f32 = 216.;

/// A control's place: left, top, width and height.
type Place = (f32, f32, f32, f32);

const L2: Place = (46., 2., 60., 18.);
const R2: Place = (254., 2., 60., 18.);
const L1: Place = (46., 24., 60., 11.);
const R1: Place = (254., 24., 60., 11.);
const TOUCHPAD: Place = (116., 46., 128., 70.);
const CREATE: Place = (100., 50., 9., 18.);
const OPTIONS: Place = (251., 50., 9., 18.);
const PS: Place = (170., 130., 20., 20.);
const MUTE: Place = (170., 159., 20., 8.);

const DPAD_CENTRE: (f32, f32) = (70., 86.);
const DPAD_ARM: f32 = 15.;

const FACE_CENTRE: (f32, f32) = (290., 86.);
const FACE_SIZE: f32 = 20.;
const FACE_SPREAD: f32 = 22.;

const LEFT_STICK_CENTRE: (f32, f32) = (126., 146.);
const RIGHT_STICK_CENTRE: (f32, f32) = (234., 146.);
const STICK_RING: f32 = 48.;
const STICK_KNOB: f32 = 30.;
/// How far the knob moves from the centre at full travel.
const STICK_TRAVEL: f32 = 11.;

const FINGER: f32 = 12.;

/// The colours of one drawing.
#[derive(Clone, Copy)]
struct Palette {
    body: Hsla,
    outline: Hsla,
    control: Hsla,
    label: Hsla,
    pressed: Hsla,
    pressed_label: Hsla,
}

impl Palette {
    fn new(theme: &Theme) -> Self {
        Self {
            body: theme.secondary,
            outline: theme.border,
            control: theme.background,
            label: theme.muted_foreground,
            pressed: theme.primary,
            pressed_label: theme.primary_foreground,
        }
    }
}

/// The drawing for a Controller's live input, dimmed and at rest when there is none.
pub fn drawing(pad: Option<PadModel>, cx: &App) -> impl IntoElement {
    let palette = Palette::new(cx.theme());
    let live = pad.is_some();
    let pad = pad.unwrap_or(PadModel::NEUTRAL);
    // A control at `place`, filled when `button` is pressed.
    let button = |place: Place, button: Buttons, label: &'static str| {
        let pressed = pad.pressed(button);
        control(place, palette)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(9.))
            .line_height(relative(1.))
            .text_color(palette.label)
            .when(pressed, |this| {
                this.bg(palette.pressed)
                    .border_color(palette.pressed)
                    .text_color(palette.pressed_label)
            })
            .child(label)
    };
    let dpad = |dx: f32, dy: f32, direction: Buttons| {
        let (x, y) = DPAD_CENTRE;
        let place = (
            x - DPAD_ARM / 2. + dx * DPAD_ARM,
            y - DPAD_ARM / 2. + dy * DPAD_ARM,
            DPAD_ARM,
            DPAD_ARM,
        );
        button(place, direction, "").rounded(px(3.))
    };
    let face = |dx: f32, dy: f32, which: Buttons, symbol: &'static str| {
        let (x, y) = FACE_CENTRE;
        let place = (
            x - FACE_SIZE / 2. + dx * FACE_SPREAD,
            y - FACE_SIZE / 2. + dy * FACE_SPREAD,
            FACE_SIZE,
            FACE_SIZE,
        );
        button(place, which, symbol)
            .rounded_full()
            .text_size(px(11.))
    };

    div()
        .relative()
        .flex_none()
        .w(px(WIDTH))
        .h(px(HEIGHT))
        .when(!live, |this| this.opacity(0.45))
        .child(body(palette))
        .child(trigger(
            L2,
            text::L2,
            pad.left_trigger,
            pad.pressed(Buttons::L2),
            palette,
        ))
        .child(trigger(
            R2,
            text::R2,
            pad.right_trigger,
            pad.pressed(Buttons::R2),
            palette,
        ))
        .child(button(L1, Buttons::L1, text::L1).rounded(px(5.)))
        .child(button(R1, Buttons::R1, text::R1).rounded(px(5.)))
        .child(touchpad(&pad, palette))
        .child(button(CREATE, Buttons::CREATE, "").rounded_full())
        .child(button(OPTIONS, Buttons::OPTIONS, "").rounded_full())
        .child(dpad(0., -1., Buttons::DPAD_UP))
        .child(dpad(0., 1., Buttons::DPAD_DOWN))
        .child(dpad(-1., 0., Buttons::DPAD_LEFT))
        .child(dpad(1., 0., Buttons::DPAD_RIGHT))
        .child(face(0., -1., Buttons::TRIANGLE, text::TRIANGLE_SYMBOL))
        .child(face(0., 1., Buttons::CROSS, text::CROSS_SYMBOL))
        .child(face(-1., 0., Buttons::SQUARE, text::SQUARE_SYMBOL))
        .child(face(1., 0., Buttons::CIRCLE, text::CIRCLE_SYMBOL))
        .children(stick(
            LEFT_STICK_CENTRE,
            pad.left_stick,
            pad.pressed(Buttons::L3),
            palette,
        ))
        .children(stick(
            RIGHT_STICK_CENTRE,
            pad.right_stick,
            pad.pressed(Buttons::R3),
            palette,
        ))
        .child(button(PS, Buttons::PS, text::PS).rounded_full())
        .child(button(MUTE, Buttons::MUTE, "").rounded_full())
}

/// An empty control at `place`.
fn control((left, top, width, height): Place, palette: Palette) -> Div {
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(width))
        .h(px(height))
        .bg(palette.control)
        .border_1()
        .border_color(palette.outline)
}

/// A bar that fills with the trigger's travel, outlined once it clicks.
fn trigger(place: Place, label: &'static str, travel: u8, clicked: bool, palette: Palette) -> Div {
    control(place, palette)
        .rounded(px(5.))
        .overflow_hidden()
        .when(clicked, |this| this.border_color(palette.pressed))
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .h_full()
                .w(relative(trigger_fill(travel)))
                .bg(palette.pressed.opacity(0.55)),
        )
        .child(
            div()
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(9.))
                .line_height(relative(1.))
                .text_color(palette.label)
                .when(travel > 0, |this| this.text_color(gpui::white()))
                .child(label),
        )
}

/// The touchpad, outlined when clicked, with a dot under each finger.
fn touchpad(pad: &PadModel, palette: Palette) -> Div {
    let (_, _, width, height) = TOUCHPAD;
    let clicked = pad.pressed(Buttons::TOUCHPAD);
    control(TOUCHPAD, palette)
        .rounded(px(9.))
        .when(clicked, |this| {
            this.bg(palette.pressed.opacity(0.3))
                .border_color(palette.pressed)
        })
        .children(pad.touch.into_iter().flatten().map(|finger| {
            let (x, y) = finger.share();
            // Kept inside the pad: the dot's travel is the pad less its own size.
            div()
                .absolute()
                .left(px(x * (width - FINGER - 2.)))
                .top(px(y * (height - FINGER - 2.)))
                .size(px(FINGER))
                .rounded_full()
                .bg(palette.pressed)
        }))
}

/// A stick's ring and its knob, which fills when the stick is clicked.
fn stick((x, y): (f32, f32), at: Stick, clicked: bool, palette: Palette) -> [Div; 2] {
    let centred = |x: f32, y: f32, size: f32| {
        control((x - size / 2., y - size / 2., size, size), palette).rounded_full()
    };
    let (dx, dy) = stick_offset(at);
    let moved = at != Stick::CENTER;
    let knob = centred(x + dx * STICK_TRAVEL, y + dy * STICK_TRAVEL, STICK_KNOB)
        .bg(palette.body)
        .border_2()
        .when(moved, |this| this.border_color(palette.label))
        .when(clicked, |this| {
            this.bg(palette.pressed).border_color(palette.pressed)
        });
    [centred(x, y, STICK_RING), knob]
}

/// The Controller's silhouette behind the controls.
fn body(palette: Palette) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            paint_silhouette(bounds, PathBuilder::fill(), palette.body, window);
            paint_silhouette(bounds, PathBuilder::stroke(px(1.)), palette.outline, window);
        },
    )
    .absolute()
    .size_full()
}

fn paint_silhouette(
    bounds: Bounds<Pixels>,
    mut path: PathBuilder,
    color: Hsla,
    window: &mut Window,
) {
    let at = |x: f32, y: f32| -> Point<Pixels> { bounds.origin + point(px(x), px(y)) };
    // Clockwise from the top left shoulder: each segment is its end and two
    // control points.
    let segments = [
        // Right shoulder, outer grip, grip end and inner grip.
        ((338., 68.), (324., 40.), (334., 50.)),
        ((356., 168.), (346., 98.), (354., 138.)),
        ((328., 213.), (360., 198.), (348., 214.)),
        ((270., 180.), (308., 212.), (294., 190.)),
        // Bottom edge.
        ((90., 180.), (240., 176.), (120., 176.)),
        // Left grip and shoulder.
        ((32., 213.), (66., 190.), (52., 212.)),
        ((4., 168.), (12., 214.), (0., 198.)),
        ((22., 68.), (6., 138.), (14., 98.)),
        ((64., 40.), (26., 50.), (36., 40.)),
    ];
    path.move_to(at(64., 40.));
    path.line_to(at(296., 40.));
    for ((x, y), (ax, ay), (bx, by)) in segments {
        path.cubic_bezier_to(at(x, y), at(ax, ay), at(bx, by));
    }
    path.close();
    // A silhouette that cannot be built is only missing its backdrop.
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

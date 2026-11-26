//! The live drawing of a Controller, shaped like its Output Type: an Xbox pad or a
//! DualShock 4. An Xbox pad has no touchpad or mute button, which drive nothing on
//! its Virtual Controller either.
//!
//! The pads are heka's own drawings. Positions are in pixels from the top left
//! corner of a drawing [`WIDTH`] wide, and the whole drawing is scaled to the room
//! it gets. Shapes are painted on one canvas, with their lettering over them.

mod ps4;
mod shape;
mod xbox;

use gpui::{
    App, IntoElement, ParentElement, Pixels, Point, SharedString, Styled, TextAlign, TextRun,
    Window, canvas, div, point, prelude::FluentBuilder as _, px,
};
use gpui_component::ActiveTheme;
use heka_core::virtual_controller::OutputType;

use crate::ui::pad::PadModel;
use shape::{Label, Painter, Palette};

pub const WIDTH: f32 = 360.;
pub const HEIGHT: f32 = 240.;
const MAX_WIDTH: f32 = 560.;

/// The room left beside the drawing, and above and below it.
const MARGIN: (f32, f32) = (20., 12.);

/// The size of a label's lettering.
const LABEL: f32 = 9.;

/// How wide the drawing is in a room `width` by `height`: as wide as both allow,
/// within its limits.
pub fn fitted_width(width: f32, height: f32) -> f32 {
    let beside = width - 2. * MARGIN.0;
    let below = (height - 2. * MARGIN.1) * WIDTH / HEIGHT;
    beside.min(below).clamp(WIDTH, MAX_WIDTH)
}

/// The drawing for a Controller's live input, dimmed and at rest when there is
/// none. It takes the rest of its column, and never less than it needs at its
/// smallest.
pub fn drawing(output_type: OutputType, pad: Option<PadModel>, cx: &App) -> impl IntoElement {
    let palette = Palette::new(cx.theme());
    let live = pad.is_some();
    let pad = pad.unwrap_or(PadModel::NEUTRAL);
    let picture = canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            let room = bounds.size;
            let width = fitted_width(room.width.into(), room.height.into());
            let scale = width / WIDTH;
            let centred = |room: Pixels, side: f32| (room - px(side * scale)) / 2.;
            let origin =
                bounds.origin + point(centred(room.width, WIDTH), centred(room.height, HEIGHT));
            let mut painter = Painter::new(origin, scale, palette, window);
            let labels = match output_type {
                OutputType::Xbox => {
                    xbox::paint(&mut painter, &pad);
                    xbox::labels(&pad, &palette)
                }
                OutputType::Ps4 => {
                    ps4::paint(&mut painter, &pad);
                    ps4::labels(&pad, &palette)
                }
            };
            for label in labels {
                letter(label, origin, scale, window, cx);
            }
        },
    )
    .size_full();

    div()
        .flex_1()
        .w_full()
        .min_h(px(HEIGHT + 2. * MARGIN.1))
        .when(!live, |this| this.opacity(0.45))
        .child(picture)
}

/// Writes a label centred on its place.
fn letter(
    Label { at, text, color }: Label,
    origin: Point<Pixels>,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let text = SharedString::from(text);
    let size = px(LABEL * scale);
    let run = TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window.text_system().shape_line(text, size, &[run], None);
    let (x, y) = at;
    let corner = origin + point(px(x * scale) - line.width / 2., px(y * scale) - size / 2.);
    // A label that cannot be drawn is only missing from the picture.
    let _ = line.paint(corner, size, TextAlign::Left, None, window, cx);
}

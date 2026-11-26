//! The live drawing of a Controller, shaped like its Output Type: an Xbox pad or a
//! DualShock 4. An Xbox pad has no touchpad or mute button, which drive nothing on
//! its Virtual Controller either.
//!
//! The pads are heka's own drawings. Positions are in pixels from the drawing's top
//! left corner: shapes are painted on one canvas, with their lettering laid over it.

mod ps4;
mod shape;
mod xbox;

use gpui::{
    App, IntoElement, ParentElement, Styled, canvas, div, prelude::FluentBuilder as _, px, relative,
};
use gpui_component::ActiveTheme;
use heka_core::virtual_controller::OutputType;

use crate::ui::pad::PadModel;
use shape::{Label, Painter, Palette};

pub const WIDTH: f32 = 360.;
pub const HEIGHT: f32 = 240.;

/// The size of a label's box, wide and tall enough for any label.
const LABEL: (f32, f32) = (28., 14.);

/// The drawing for a Controller's live input, dimmed and at rest when there is none.
pub fn drawing(output_type: OutputType, pad: Option<PadModel>, cx: &App) -> impl IntoElement {
    let palette = Palette::new(cx.theme());
    let live = pad.is_some();
    let pad = pad.unwrap_or(PadModel::NEUTRAL);
    let labels = match output_type {
        OutputType::Xbox => xbox::labels(&pad, &palette),
        OutputType::Ps4 => ps4::labels(&pad, &palette),
    };
    let shapes = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let mut painter = Painter::new(bounds.origin, palette, window);
            match output_type {
                OutputType::Xbox => xbox::paint(&mut painter, &pad),
                OutputType::Ps4 => ps4::paint(&mut painter, &pad),
            }
        },
    )
    .absolute()
    .size_full();

    div()
        .relative()
        .flex_none()
        .w(px(WIDTH))
        .h(px(HEIGHT))
        .when(!live, |this| this.opacity(0.45))
        .child(shapes)
        .children(labels.into_iter().map(label))
}

/// A label centred on its place.
fn label(Label { at, text, color }: Label) -> impl IntoElement {
    let (x, y) = at;
    let (width, height) = LABEL;
    div()
        .absolute()
        .left(px(x - width / 2.))
        .top(px(y - height / 2.))
        .w(px(width))
        .h(px(height))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(9.))
        .line_height(relative(1.))
        .text_color(color)
        .child(text)
}

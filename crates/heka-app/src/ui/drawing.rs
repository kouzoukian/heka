mod edge;
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
use crate::{log, text};
use shape::Painter;

pub use shape::{Label, Palette, Surface};

pub const WIDTH: f32 = 360.;
pub const HEIGHT: f32 = 240.;
const MAX_WIDTH: f32 = 560.;

const MARGIN: (f32, f32) = (20., 12.);

const LABEL_SIZE: f32 = 9.;

static DRAWING_ERRORS: log::Once = log::Once::new();

pub fn fitted_width(width: f32, height: f32) -> f32 {
    let beside = width - 2. * MARGIN.0;
    let below = (height - 2. * MARGIN.1) * WIDTH / HEIGHT;
    beside.min(below).clamp(WIDTH, MAX_WIDTH)
}

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
            let labels = paint(window, origin, scale, palette, output_type, &pad);
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

pub fn paint(
    surface: &mut dyn Surface,
    origin: Point<Pixels>,
    scale: f32,
    palette: Palette,
    output_type: OutputType,
    pad: &PadModel,
) -> Vec<Label> {
    let mut painter = Painter::new(origin, scale, palette, surface);
    let (mut labels, sticks) = match output_type {
        OutputType::Xbox => {
            xbox::paint(&mut painter, pad);
            (xbox::labels(pad, &palette), xbox::STICKS)
        }
        OutputType::Ps4 => {
            ps4::paint(&mut painter, pad);
            (ps4::labels(pad, &palette), ps4::STICKS)
        }
    };
    edge::paint(&mut painter, pad, sticks);
    labels.extend(edge::labels(pad, &palette, sticks));
    labels
}

fn letter(
    Label { at, text, color }: Label,
    origin: Point<Pixels>,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let text = SharedString::from(text);
    let size = px(LABEL_SIZE * scale);
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
    if let Err(error) = line.paint(corner, size, TextAlign::Left, None, window, cx) {
        DRAWING_ERRORS.line(&text::cannot_draw(&error.to_string()));
    }
}

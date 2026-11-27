use gpui::{
    Anchor, App, Div, Entity, Hsla, IntoElement, ParentElement, Rgba, Styled, div, hsla,
    linear_color_stop, linear_gradient, prelude::FluentBuilder as _, px, rgb,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::color_picker::ColorPickerState;
use gpui_component::input::Input;
use gpui_component::popover::Popover;
use gpui_component::slider::{Slider, SliderState};
use gpui_component::{ActiveTheme, Selectable, Sizable, Theme};
use gpui_kit_assets::IconName;
use heka_core::feedback::Lightbar;

use crate::text;
use crate::ui::metrics::{CONTROL, GAP, INSET, SMALL_TEXT, icon};
use crate::ui::segmented::segment_height;

const WIDTH: f32 = 280.;
const LABEL_WIDTH: f32 = 76.;
const HUE_STEPS: u16 = 60;

pub fn drawn(colour: Lightbar) -> Hsla {
    let Lightbar { red, green, blue } = colour;
    rgb(u32::from_be_bytes([0, red, green, blue])).into()
}

pub fn picked(colour: Hsla) -> Lightbar {
    let Rgba { r, g, b, .. } = colour.into();
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    Lightbar {
        red: channel(r),
        green: channel(g),
        blue: channel(b),
    }
}

pub fn typed(text: &str) -> Option<Lightbar> {
    let digits = text.strip_prefix('#')?;
    if !matches!(digits.len(), 6 | 8) || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let [_, red, green, blue] = u32::from_str_radix(&digits[..6], 16).ok()?.to_be_bytes();
    Some(Lightbar { red, green, blue })
}

pub fn swatch(colour: Lightbar) -> Div {
    div().size(px(10.)).rounded_full().bg(drawn(colour))
}

pub fn rainbow_swatch() -> Div {
    div().flex().size(px(10.)).children((0..5u8).map(|stripe| {
        let hue = f32::from(stripe) / 5.;
        div().flex_1().h_full().bg(hsla(hue, 1., 0.5, 1.))
    }))
}

pub fn picker(state: &Entity<ColorPickerState>, custom: Option<Lightbar>, cx: &App) -> Popover {
    let theme = cx.theme();
    let trigger = Button::new("custom-colour")
        .ghost()
        .selected(custom.is_some())
        .h(segment_height())
        .px(GAP)
        .rounded(px(3.))
        .map(|this| match custom {
            Some(colour) => this.bg(theme.accent).child(swatch(colour)),
            None => this
                .text_color(theme.muted_foreground)
                .child(icon(IconName::Palette)),
        })
        .accessibility_label(text::CUSTOM_COLOUR)
        .tooltip(text::CUSTOM_COLOUR);
    let open = state.read(cx).is_open();
    let toggled = state.clone();
    Popover::new("colour-picker")
        .anchor(Anchor::TopRight)
        .open(open)
        .on_open_change(move |open: &bool, _, cx| {
            toggled.update(cx, |state, cx| state.set_open(*open, cx));
        })
        .trigger(trigger)
        .when(open, |this| this.child(panel(state, cx)))
}

fn panel(state: &Entity<ColorPickerState>, cx: &App) -> Div {
    let theme = cx.theme();
    let state = state.read(cx);
    let shown = state
        .displayed_color()
        .unwrap_or_else(|| drawn(Lightbar::DEFAULT));
    let Hsla { h, s, l, .. } = shown;
    let sliders = state.sliders();
    let hues = div()
        .flex()
        .size_full()
        .children((0..HUE_STEPS).map(|step| {
            let hue = f32::from(step) / f32::from(HUE_STEPS - 1);
            div().flex_1().h_full().bg(hsla(hue, 1., 0.5, 1.))
        }));
    let saturations = between(hsla(h, 0., l, 1.), hsla(h, 1., l, 1.));
    let lightnesses = div()
        .flex()
        .size_full()
        .child(between(hsla(h, s, 0., 1.), hsla(h, s, 0.5, 1.)).flex_1())
        .child(between(hsla(h, s, 0.5, 1.), hsla(h, s, 1., 1.)).flex_1());
    div()
        .flex()
        .flex_col()
        .gap(GAP)
        .w(px(WIDTH))
        .text_size(SMALL_TEXT)
        .text_color(theme.muted_foreground)
        .child(
            div()
                .flex()
                .items_center()
                .gap(INSET)
                .child(
                    div()
                        .flex_none()
                        .size(CONTROL)
                        .rounded(theme.radius)
                        .border_1()
                        .border_color(theme.border)
                        .bg(shown),
                )
                .child(
                    div()
                        .flex_1()
                        .text_color(theme.foreground)
                        .child(Input::new(state.hex_input()).small()),
                ),
        )
        .child(slider(text::HUE, sliders.hue(), hues, theme))
        .child(slider(
            text::SATURATION,
            sliders.saturation(),
            saturations,
            theme,
        ))
        .child(slider(
            text::LIGHTNESS,
            sliders.lightness(),
            lightnesses,
            theme,
        ))
}

fn between(from: Hsla, to: Hsla) -> Div {
    div().size_full().bg(linear_gradient(
        90.,
        linear_color_stop(from, 0.),
        linear_color_stop(to, 1.),
    ))
}

fn slider(
    label: &'static str,
    state: &Entity<SliderState>,
    track: impl IntoElement,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(GAP)
        .h(CONTROL)
        .child(div().flex_none().w(px(LABEL_WIDTH)).child(label))
        .child(
            div()
                .relative()
                .flex()
                .flex_1()
                .items_center()
                .h_full()
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .h(px(8.))
                        .overflow_hidden()
                        .child(track),
                )
                .child(
                    Slider::new(state)
                        .flex_1()
                        .bg(theme.transparent)
                        .text_color(theme.foreground)
                        .accessibility_label(label),
                ),
        )
}

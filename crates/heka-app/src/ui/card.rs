//! One Controller's card: its name, Link, battery, Output Type switch, state,
//! Stop or Start, and the live drawing.

use std::rc::Rc;

use gpui::{
    App, Div, FontWeight, Hsla, ParentElement, SharedString, Styled, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::tag::Tag;
use gpui_component::{ActiveTheme, Icon, Selectable, Sizable};
use gpui_kit_assets::IconName;
use heka_core::virtual_controller::OutputType;

use crate::text;
use crate::ui::assets::{battery_icon, link_icon};
use crate::ui::drawing::{self, drawing};
use crate::ui::model::{ActiveLink, CardModel, CardState, Command};

/// The drawing and a margin on each side.
pub const WIDTH: f32 = drawing::WIDTH + 2. * PADDING + 2.;
const PADDING: f32 = 12.;

/// Where a card's buttons send what the player asks for.
pub type Send = Rc<dyn Fn(Command)>;

pub fn card(model: &CardModel, send: &Send, cx: &App) -> Div {
    let theme = cx.theme();
    // Element ids are per card, so that each keeps its own hover and focus.
    let id = SharedString::from(model.mac.to_string());
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(WIDTH))
        .bg(theme.group_box)
        .border_1()
        .border_color(theme.sidebar_border)
        .rounded(theme.radius_lg)
        .child(header(model, id.clone(), send, cx))
        .child(facts(model, id, send, cx))
        .child(
            div()
                .flex()
                .justify_center()
                .px(px(PADDING))
                .pt_1()
                .pb(px(PADDING))
                .child(drawing(model.pad, cx)),
        )
        .children(model.detail().map(|detail| {
            let color = if model.error.is_some() {
                theme.danger
            } else {
                theme.muted_foreground
            };
            div()
                .px(px(PADDING))
                .py_2()
                .border_t_1()
                .border_color(theme.sidebar_border)
                .text_xs()
                .text_color(color)
                .child(detail)
        }))
}

/// Name, state and Stop or Start.
fn header(model: &CardModel, id: SharedString, send: &Send, cx: &App) -> Div {
    let theme = cx.theme();
    let command = model.toggle();
    let (label, icon) = match command {
        Command::Start(_) => (text::START, IconName::Play),
        _ => (text::STOP, IconName::Square),
    };
    let send = Rc::clone(send);
    div()
        .flex()
        .items_center()
        .gap_2()
        .px(px(PADDING))
        .h(px(40.))
        .border_b_1()
        .border_color(theme.sidebar_border)
        .child(
            Icon::new(IconName::Gamepad2)
                .small()
                .text_color(theme.muted_foreground),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(model.name),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(model.mac.to_string()),
        )
        .child(state_tag(model.state))
        .child(
            Button::new(SharedString::from(format!("toggle-{id}")))
                .xsmall()
                .icon(Icon::new(icon))
                .label(label)
                .on_click(move |_, _, _| send(command)),
        )
}

fn state_tag(state: CardState) -> Tag {
    let (tag, label) = match state {
        CardState::Bridged => (Tag::success(), text::BRIDGED.into()),
        CardState::Held { seconds_left } => (Tag::warning(), text::held(seconds_left)),
        CardState::Stopped => (Tag::secondary(), text::STOPPED.into()),
        CardState::NotBridged => (Tag::secondary(), text::NOT_BRIDGED.into()),
    };
    tag.outline().small().child(label)
}

/// Active Link, battery and the Output Type switch.
fn facts(model: &CardModel, id: SharedString, send: &Send, cx: &App) -> Div {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let link = match model.active_link {
        Some(ActiveLink::Usb) => text::USB,
        Some(ActiveLink::Bluetooth) => text::BLUETOOTH,
        None => text::NO_LINK,
    };
    let battery = match model.battery {
        Some(battery) => text::battery(battery.percent, battery.charging),
        None => text::BATTERY_UNKNOWN.into(),
    };
    div()
        .flex()
        .items_center()
        .gap_4()
        .px(px(PADDING))
        .pt_2()
        .text_xs()
        .child(fact(link_icon(model.active_link), link.into(), muted))
        .child(fact(battery_icon(model.battery), battery, muted))
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_color(muted).child(text::OUTPUT_TYPE))
                .child(output_switch(model, id, send, cx)),
        )
}

fn fact(icon: IconName, label: String, muted: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).small().text_color(muted))
        .child(label)
}

/// A segmented control with one segment per Output Type.
fn output_switch(model: &CardModel, id: SharedString, send: &Send, cx: &App) -> Div {
    let theme = cx.theme();
    let segment = |output_type: OutputType| {
        let (key, label) = match output_type {
            OutputType::Xbox => ("xbox", text::XBOX),
            OutputType::Ps4 => ("ps4", text::PS4),
        };
        let selected = model.output_type == output_type;
        let command = model.choose(output_type);
        let send = Rc::clone(send);
        Button::new(SharedString::from(format!("{key}-{id}")))
            .xsmall()
            .ghost()
            .selected(selected)
            .label(label)
            .when(selected, |this| this.text_color(theme.primary))
            .on_click(move |_, _, _| {
                if let Some(command) = command {
                    send(command);
                }
            })
    };
    div()
        .flex()
        .gap(px(2.))
        .p(px(2.))
        .bg(theme.background)
        .border_1()
        .border_color(theme.sidebar_border)
        .rounded(theme.radius_lg)
        .child(segment(OutputType::Xbox))
        .child(segment(OutputType::Ps4))
}

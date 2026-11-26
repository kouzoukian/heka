//! The selected Controller's pane: its tab with Stop or Start, heka's notices,
//! its properties and the live drawing. Without a Controller, the empty state.
//!
//! Everything above the drawing is a whole number of rows high at any width, so
//! that it stays level with the panel's rows.

use std::rc::Rc;

use gpui::{
    App, Div, InteractiveElement, IntoElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, Selectable, Theme};
use gpui_kit_assets::IconName;
use heka_core::virtual_controller::OutputType;

use crate::text;
use crate::ui::assets::{battery_icon, link_icon};
use crate::ui::banners::{clamped, notice_banner, setup_banners, strip};
use crate::ui::drawing::drawing;
use crate::ui::metrics::{CONTROL, GAP, INSET, ROW, SMALL_TEXT, icon};
use crate::ui::model::{Command, ControllerEntry, EntryState, Screen, SendCommand};
use crate::ui::panel::{entry_color, faint};

/// The pane for `entry`, the selected Controller, if there is one.
pub fn pane(screen: &Screen, entry: Option<&ControllerEntry>, send: &SendCommand, cx: &App) -> Div {
    let body = div()
        .id("pane")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .children(setup_banners(&screen.missing, send, cx));
    let body = match entry {
        Some(entry) => body
            .child(properties(entry, send, cx))
            .children(entry.detail().map(|detail| note(entry, detail, cx)))
            .child(drawing(entry.output_type, entry.pad, cx)),
        None => body.child(empty_state(screen.starting, cx)),
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(tab_strip(entry, send, cx))
        // heka's notices stay in view while the rest scrolls.
        .children(
            screen
                .notices
                .iter()
                .enumerate()
                .map(|(place, notice)| notice_banner(place, notice, cx)),
        )
        .child(body)
}

/// One tab, for the selected Controller, and its Stop or Start button.
fn tab_strip(entry: Option<&ControllerEntry>, send: &SendCommand, cx: &App) -> Div {
    let theme = cx.theme();
    let tab = entry.map(|entry| {
        div()
            .flex()
            .flex_none()
            .items_center()
            .h_full()
            .gap(GAP)
            .px(INSET)
            .border_r_1()
            .border_color(theme.border)
            .text_color(theme.foreground)
            .child(icon(IconName::Gamepad2))
            .child(entry.name)
            .child(
                div()
                    .text_size(SMALL_TEXT)
                    .text_color(faint(theme))
                    .child(entry.short_mac()),
            )
    });
    // The same box and line as the panel's header, so that the two read as one row.
    div()
        .flex()
        .flex_none()
        .h(ROW)
        .border_b_1()
        .border_color(theme.border)
        .children(tab)
        .child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_end()
                .pr(GAP)
                .children(entry.map(|entry| toggle(entry, send))),
        )
}

fn toggle(entry: &ControllerEntry, send: &SendCommand) -> Button {
    let command = entry.toggle();
    let (label, name) = match command {
        Command::Start(_) => (text::START, IconName::Play),
        _ => (text::STOP, IconName::Square),
    };
    let send = Rc::clone(send);
    // One id per Controller, so that each keeps its own hover and tooltip.
    Button::new(SharedString::from(format!("toggle-{}", entry.mac)))
        .ghost()
        .size(CONTROL)
        .p_0()
        // A child, not `.icon`, which would resize it.
        .child(icon(name))
        .accessibility_label(label)
        .tooltip(label)
        .on_click(move |_, _, _| send(command))
}

fn properties(entry: &ControllerEntry, send: &SendCommand, cx: &App) -> Div {
    let theme = cx.theme();
    let state = [
        div()
            .size(px(8.))
            .rounded_full()
            .bg(entry_color(entry, theme))
            .into_any_element(),
        state_label(entry.state).into_any_element(),
    ];
    let experimental = entry.experimental.then(|| {
        row(theme)
            .child(text::ROW_SUPPORT)
            .child(div().text_color(theme.warning).child(text::EXPERIMENTAL))
    });
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(
            row(theme)
                .child(text::ROW_STATE)
                .child(value(theme).children(state)),
        )
        .child(facts(entry, send, theme))
        .child(
            row(theme)
                .child(text::ROW_ADDRESS)
                .child(value(theme).child(entry.mac.to_string())),
        )
        .children(experimental)
}

/// A full-width property: its label on the left and its value on the right, as in
/// Zed's settings.
fn row(theme: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .gap(INSET)
        .h(ROW)
        .px(INSET)
        .border_b_1()
        .border_color(theme.border)
}

fn value(theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(GAP)
        .text_color(theme.foreground)
}

/// The Active Link, the battery and the Output Type switch side by side, with
/// dividers between them. Their icons and words name them; a tooltip says so.
fn facts(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .h(ROW)
        .border_b_1()
        .border_color(theme.border)
        .text_color(theme.foreground)
        .child(
            cell("link", text::ACTIVE_LINK.into())
                .flex_none()
                .border_r_1()
                .border_color(theme.border)
                .child(icon(link_icon(entry.active_link)))
                .child(text::link(entry.active_link)),
        )
        .child(
            cell("battery", text::battery_in_full(entry.battery).into())
                .flex_1()
                .min_w_0()
                .border_r_1()
                .border_color(theme.border)
                .child(icon(battery_icon(entry.battery)))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(text::battery(entry.battery)),
                ),
        )
        .child(
            cell("output", text::OUTPUT_TYPE.into())
                .flex_none()
                .child(output_switch(entry, send, theme)),
        )
}

/// One of the facts that share a row, with `tip` in its tooltip.
fn cell(id: &'static str, tip: SharedString) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(GAP)
        .h_full()
        .px(INSET)
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
}

fn state_label(state: EntryState) -> SharedString {
    match state {
        EntryState::Bridged => text::BRIDGED.into(),
        EntryState::Held { seconds_left } => text::held(seconds_left).into(),
        EntryState::Stopped => text::STOPPED.into(),
        EntryState::NotBridged => text::NOT_BRIDGED.into(),
    }
}

/// A segmented control with one segment per Output Type.
fn output_switch(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> Div {
    let segment = |output_type: OutputType| {
        let (key, label) = match output_type {
            OutputType::Xbox => ("xbox", text::XBOX),
            OutputType::Ps4 => ("ps4", text::PS4),
        };
        let selected = entry.output_type == output_type;
        let command = entry.choose(output_type);
        let send = Rc::clone(send);
        Button::new(SharedString::from(format!("{key}-{}", entry.mac)))
            .ghost()
            .selected(selected)
            // The switch's border and padding bring it to a control's height.
            .h(CONTROL - px(6.))
            .px(GAP)
            .rounded(px(3.))
            .map(|this| {
                if selected {
                    this.bg(theme.accent).text_color(theme.accent_foreground)
                } else {
                    this.text_color(theme.muted_foreground)
                }
            })
            .accessibility_label(label)
            .child(div().text_size(SMALL_TEXT).child(label))
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
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radius)
        .child(segment(OutputType::Xbox))
        .child(segment(OutputType::Ps4))
}

/// The line that explains the Controller's state.
fn note(entry: &ControllerEntry, detail: String, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let words = SharedString::from(detail);
    strip("note", 1, theme)
        .when(entry.error.is_some(), |this| this.text_color(theme.danger))
        .child(clamped(words.clone()))
        .tooltip(move |window, cx| Tooltip::new(words.clone()).build(window, cx))
}

/// Shown while there is no Controller.
fn empty_state(starting: bool, cx: &App) -> Div {
    let theme = cx.theme();
    let (title, hint) = if starting {
        (text::STARTING, None)
    } else {
        (text::NO_CONTROLLER_CONNECTED, Some(text::CONNECT_HINT))
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(GAP)
        .p(px(32.))
        .text_center()
        .child(icon(IconName::Gamepad2).size(px(28.)))
        .child(title)
        .children(hint.map(|hint| {
            div()
                .max_w(px(320.))
                .text_size(SMALL_TEXT)
                .line_height(px(18.))
                .text_color(faint(theme))
                .child(hint)
        }))
}

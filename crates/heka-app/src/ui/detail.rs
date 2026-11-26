use std::rc::Rc;

use gpui::{
    App, Div, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, Selectable, Theme};
use gpui_kit_assets::IconName;
use heka_core::virtual_controller::OutputType;

use crate::log::Recent;
use crate::text;
use crate::ui::assets::{battery_icon, link_icon};
use crate::ui::banners::{clamped, notice_banner, setup_banners, strip};
use crate::ui::drawing::drawing;
use crate::ui::log;
use crate::ui::metrics::{CONTROL, GAP, INSET, ROW, SMALL_TEXT, icon};
use crate::ui::model::{Command, ControllerEntry, EntryState, Screen, SendCommand};
use crate::ui::panel::{entry_color, faint};
use crate::ui::segmented::{Segment, segmented};
use crate::ui::settings::{Fields, settings};

pub enum Showing<'a> {
    Live,
    Settings(Fields<'a>),
    Log(&'a Recent),
}

pub type ToggleSettings = Rc<dyn Fn(&mut Window, &mut App)>;

pub fn pane(
    screen: &Screen,
    entry: Option<&ControllerEntry>,
    showing: Showing,
    toggle_settings: &ToggleSettings,
    send: &SendCommand,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let in_settings = matches!(showing, Showing::Settings(_));
    let tabs = match &showing {
        Showing::Live | Showing::Settings(_) => tab_strip(
            entry.map(|entry| controller_tab(entry, theme)),
            entry.into_iter().flat_map(|entry| {
                [
                    settings_button(in_settings, toggle_settings, theme),
                    toggle(entry, send),
                ]
            }),
            theme,
        ),
        Showing::Log(recent) => tab_strip(
            Some(page_tab(IconName::ScrollText, text::LOG, theme)),
            log::buttons(recent, send),
            theme,
        ),
    };
    let scrolling = || {
        div()
            .id("pane")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
    };
    let controller = |body: Stateful<Div>| body.children(setup_banners(&screen.missing, send, cx));
    let body = match (entry, showing) {
        (_, Showing::Log(recent)) => log::entries(recent, cx).into_any_element(),
        (Some(entry), Showing::Live) => controller(scrolling())
            .child(properties(entry, send, cx))
            .children(entry.detail().map(|detail| note(entry, detail, cx)))
            .child(drawing(entry.output_type, entry.pad, cx))
            .into_any_element(),
        (Some(entry), Showing::Settings(fields)) => controller(scrolling())
            .child(settings(entry, fields, send, cx))
            .into_any_element(),
        (None, _) => controller(scrolling())
            .child(empty_state(screen.starting, cx))
            .into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(tabs)
        .children(
            screen
                .notices
                .iter()
                .enumerate()
                .map(|(place, notice)| notice_banner(place, notice, cx)),
        )
        .child(body)
}

fn tab_strip(tab: Option<Div>, buttons: impl IntoIterator<Item = Button>, theme: &Theme) -> Div {
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
                .gap(GAP)
                .pr(GAP)
                .children(buttons),
        )
}

fn tab(name: IconName, theme: &Theme) -> Div {
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
        .child(icon(name))
}

fn page_tab(name: IconName, title: &'static str, theme: &Theme) -> Div {
    tab(name, theme).child(title)
}

fn controller_tab(entry: &ControllerEntry, theme: &Theme) -> Div {
    tab(IconName::Gamepad2, theme)
        .child(SharedString::from(entry.title().to_owned()))
        .when(entry.nickname.is_none(), |this| {
            this.child(
                div()
                    .text_size(SMALL_TEXT)
                    .text_color(faint(theme))
                    .child(entry.short_mac()),
            )
        })
}

pub(super) fn tool_button(id: impl Into<ElementId>, name: IconName, label: &'static str) -> Button {
    Button::new(id)
        .ghost()
        .size(CONTROL)
        .p_0()
        .child(icon(name))
        .accessibility_label(label)
        .tooltip(label)
}

pub(super) fn selected_button(button: Button, selected: bool, theme: &Theme) -> Button {
    button.selected(selected).when(selected, |this| {
        this.bg(theme.accent).text_color(theme.accent_foreground)
    })
}

fn settings_button(in_settings: bool, toggle: &ToggleSettings, theme: &Theme) -> Button {
    let toggle = Rc::clone(toggle);
    let button = tool_button("settings", IconName::Settings, text::SETTINGS);
    selected_button(button, in_settings, theme).on_click(move |_, window, cx| toggle(window, cx))
}

fn toggle(entry: &ControllerEntry, send: &SendCommand) -> Button {
    let command = entry.toggle();
    let (label, name) = match command {
        Command::Start(_) => (text::START, IconName::Play),
        _ => (text::STOP, IconName::Square),
    };
    let send = Rc::clone(send);
    let id = SharedString::from(format!("toggle-{}", entry.mac));
    tool_button(id, name, label).on_click(move |_, _, _| send(command.clone()))
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

pub(super) fn row(theme: &Theme) -> Div {
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

fn output_switch(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> Div {
    let segment = |output_type: OutputType| {
        let (key, label) = match output_type {
            OutputType::Xbox => ("xbox", text::XBOX),
            OutputType::Ps4 => ("ps4", text::PS4),
        };
        Segment::words(
            format!("{key}-{}", entry.mac),
            label,
            entry.output_type == output_type,
            entry.choose(output_type),
        )
    };
    segmented(
        [segment(OutputType::Xbox), segment(OutputType::Ps4)],
        send,
        theme,
    )
}

fn note(entry: &ControllerEntry, detail: String, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let words = SharedString::from(detail);
    strip("note", 1, theme)
        .when(entry.error.is_some(), |this| this.text_color(theme.danger))
        .child(clamped(words.clone()))
        .tooltip(move |window, cx| Tooltip::new(words.clone()).build(window, cx))
}

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

//! The panel on the left: one row per Controller, as in Zed's project panel.

use std::rc::Rc;

use gpui::{
    App, Div, Hsla, InteractiveElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{ActiveTheme, Theme};
use gpui_kit_assets::IconName;
use heka_core::mac::MacAddress;

use crate::text;
use crate::ui::assets::link_icon;
use crate::ui::metrics::{GAP, INSET, ROW, SMALL_TEXT, icon};
use crate::ui::model::{ControllerEntry, EntryState, Screen};

pub const WIDTH: f32 = 240.;

/// What a click on a row does.
pub type Choose = Rc<dyn Fn(MacAddress, &mut App)>;

pub fn panel(screen: &Screen, selected: Option<MacAddress>, choose: &Choose, cx: &App) -> Div {
    let theme = cx.theme();
    let empty = (screen.controllers.is_empty() && !screen.starting).then(|| {
        div()
            .flex()
            .items_center()
            .h(ROW)
            .px(INSET)
            .border_b_1()
            .border_color(theme.border)
            .text_size(SMALL_TEXT)
            .text_color(faint(theme))
            .child(text::NO_CONTROLLERS)
    });
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(WIDTH))
        .h_full()
        .bg(theme.sidebar)
        .border_r_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .h(ROW)
                .px(INSET)
                .border_b_1()
                .border_color(theme.border)
                .child(text::CONTROLLERS),
        )
        // The rows start right under the header, level with the pane's.
        .child(
            div()
                .id("controllers")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(empty)
                .children(screen.controllers.iter().map(|entry| {
                    row(entry, Some(entry.mac) == selected, Rc::clone(choose), theme)
                })),
        )
}

/// A Controller's state in colour, its name and its Active Link.
fn row(entry: &ControllerEntry, selected: bool, choose: Choose, theme: &Theme) -> Stateful<Div> {
    let mac = entry.mac;
    let end = match entry.state {
        EntryState::Held { seconds_left } => div()
            .text_size(SMALL_TEXT)
            .text_color(theme.warning)
            .child(text::held_short(seconds_left)),
        _ => div().child(icon(link_icon(entry.active_link))),
    };
    let hover = theme.accent.opacity(0.6);
    div()
        .id(SharedString::from(format!("controller-{mac}")))
        .flex()
        .flex_none()
        .items_center()
        .gap(GAP)
        .h(ROW)
        .px(INSET)
        // The same divider as under the pane's rows, so that each line runs the
        // whole width of the window.
        .border_b_1()
        .border_color(theme.border)
        .cursor_pointer()
        .map(|this| {
            if selected {
                this.bg(theme.accent).text_color(theme.foreground)
            } else {
                this.hover(move |this| this.bg(hover))
            }
        })
        .child(icon(IconName::Gamepad2).text_color(entry_color(entry, theme)))
        .map(|this| match &entry.nickname {
            // The player's name tells it apart without its address.
            Some(nickname) => this.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(nickname.clone())),
            ),
            None => this.child(entry.name).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(SMALL_TEXT)
                    .text_color(faint(theme))
                    .child(entry.short_mac()),
            ),
        })
        .child(end)
        .on_click(move |_, _, cx| choose(mac, cx))
}

/// The colour that stands for how a Controller is doing: its state's, or the
/// colour of errors if its Bridge failed.
pub fn entry_color(entry: &ControllerEntry, theme: &Theme) -> Hsla {
    if entry.error.is_some() {
        return theme.danger;
    }
    match entry.state {
        EntryState::Bridged => theme.success,
        EntryState::Held { .. } => theme.warning,
        EntryState::Stopped => faint(theme),
        EntryState::NotBridged => theme.muted_foreground,
    }
}

/// Text that matters less than the rest.
pub fn faint(theme: &Theme) -> Hsla {
    theme.muted_foreground.opacity(0.7)
}

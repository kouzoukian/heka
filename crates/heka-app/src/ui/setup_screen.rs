//! The setup screen: which drivers are missing, what that costs and where to get
//! them. It only links to the official installers: heka never downloads or runs one.
//!
//! It shows above the Controller list, which stays in view.

use std::rc::Rc;

use gpui::{App, Div, FontWeight, ParentElement, SharedString, Styled, div};
use gpui_component::button::Button;
use gpui_component::link::Link;
use gpui_component::{ActiveTheme, Icon, Sizable};
use gpui_kit_assets::IconName;
use heka_core::setup::Missing;

use crate::text::setup as text;
use crate::ui::card;
use crate::ui::model::Command;

/// The setup screen, or nothing when no driver is missing.
pub fn setup_screen(missing: &[Missing], send: &card::Send, cx: &App) -> Option<Div> {
    if missing.is_empty() {
        return None;
    }
    let send = Rc::clone(send);
    Some(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(missing.iter().map(|missing| driver_panel(missing, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(text::AFTER_INSTALLING),
                    )
                    .child(
                        Button::new("check-drivers")
                            .small()
                            .icon(Icon::new(IconName::RefreshCw))
                            .label(text::CHECK_AGAIN)
                            .on_click(move |_, _, _| send(Command::CheckDrivers)),
                    ),
            ),
    )
}

fn driver_panel(missing: &Missing, cx: &App) -> Div {
    let theme = cx.theme();
    let url = missing.driver.installer_url();
    div()
        .flex()
        .items_start()
        .gap_2()
        .p_3()
        .rounded(theme.radius_lg)
        .bg(theme.warning.opacity(0.1))
        .border_1()
        .border_color(theme.warning.opacity(0.4))
        .child(
            Icon::new(IconName::TriangleAlert)
                .small()
                .text_color(theme.warning),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_1()
                .text_xs()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(text::heading(missing))),
                )
                .child(text::consequence(missing.driver))
                .child(Link::new(text::name(missing.driver)).href(url).child(url)),
        )
}

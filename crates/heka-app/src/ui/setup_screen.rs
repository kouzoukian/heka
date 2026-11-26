//! The setup screen: which drivers are missing, what that costs and where to get
//! them. It only links to the official installers: heka never downloads or runs one.
//!
//! It shows as thin banners above the selected Controller's pane, which stays in
//! view. heka's other notices are banners of the same kind.

use std::rc::Rc;

use gpui::{App, Div, Hsla, IntoElement, ParentElement, SharedString, Styled, div, px};
use gpui_component::ActiveTheme;
use gpui_component::button::Button;
use gpui_component::link::Link;
use gpui_kit_assets::IconName;
use heka_core::setup::Missing;

use crate::text::setup as text;
use crate::ui::metrics::{CONTROL, GAP, INSET, LINE, SMALL_TEXT, icon};
use crate::ui::model::{Command, Level, Notice, SendCommand};

/// The banners for the missing drivers and the button that looks for them again,
/// or nothing when no driver is missing.
pub fn setup_banners(missing: &[Missing], send: &SendCommand, cx: &App) -> Option<Div> {
    if missing.is_empty() {
        return None;
    }
    let theme = cx.theme();
    let send = Rc::clone(send);
    Some(
        div()
            .flex()
            .flex_col()
            .flex_none()
            .children(missing.iter().map(|missing| driver_banner(missing, cx)))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .gap(INSET)
                    .px(INSET)
                    .py(px(6.))
                    .border_b_1()
                    .border_color(theme.border)
                    .text_size(SMALL_TEXT)
                    .line_height(LINE)
                    .child(div().flex_1().min_w_0().child(text::AFTER_INSTALLING))
                    .child(
                        Button::new("check-drivers")
                            .outline()
                            .h(CONTROL)
                            .text_size(SMALL_TEXT)
                            .icon(icon(IconName::RefreshCw))
                            .label(text::CHECK_AGAIN)
                            .on_click(move |_, _, _| send(Command::CheckDrivers)),
                    ),
            ),
    )
}

fn driver_banner(missing: &Missing, cx: &App) -> Div {
    let theme = cx.theme();
    let url = missing.driver.installer_url();
    banner(
        IconName::TriangleAlert,
        theme.warning,
        div()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(GAP)
                    .child(
                        div()
                            .min_w_0()
                            .child(SharedString::from(text::heading(missing))),
                    )
                    .child(text::SEPARATOR)
                    .child(
                        Link::new(text::name(missing.driver))
                            .href(url)
                            .child(SharedString::from(text::short_url(url))),
                    ),
            )
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .child(text::consequence(missing.driver)),
            ),
        cx,
    )
}

/// One of heka's notices.
pub fn notice_banner(notice: &Notice, cx: &App) -> Div {
    let theme = cx.theme();
    let (icon, color) = match notice.level {
        Level::Warning => (IconName::TriangleAlert, theme.warning),
        Level::Error => (IconName::CircleX, theme.danger),
    };
    banner(icon, color, div().child(notice.text.clone()), cx)
}

/// A strip as wide as the pane, tinted with `color`.
fn banner(name: IconName, color: Hsla, content: impl IntoElement, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_none()
        .items_start()
        .gap(GAP)
        .px(INSET)
        .py(px(6.))
        .bg(color.opacity(0.08))
        .border_b_1()
        .border_color(theme.border)
        .text_size(SMALL_TEXT)
        .line_height(LINE)
        .text_color(theme.foreground)
        .child(
            // Centred on the first line of text.
            div()
                .flex()
                .flex_none()
                .items_center()
                .h(LINE)
                .child(icon(name).text_color(color)),
        )
        .child(div().flex_1().min_w_0().child(content))
}

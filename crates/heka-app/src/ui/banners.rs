use std::rc::Rc;

use gpui::{
    App, Div, ElementId, Hsla, InteractiveElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div,
};
use gpui_component::button::Button;
use gpui_component::link::Link;
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, Theme};
use gpui_kit_assets::IconName;
use heka_core::setup::Missing;

use crate::text::setup as text;
use crate::ui::metrics::{CONTROL, GAP, INSET, LINE, ROW, SMALL_TEXT, icon};
use crate::ui::model::{Command, Level, Notice, SendCommand};

pub fn strip(id: impl Into<SharedString>, rows: u8, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .gap(GAP)
        .h(ROW * f32::from(rows))
        .px(INSET)
        .overflow_hidden()
        .border_b_1()
        .border_color(theme.border)
        .text_size(SMALL_TEXT)
        .line_height(LINE)
}

pub fn clamped(words: SharedString) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .line_clamp(2)
        .text_ellipsis()
        .child(words)
}

pub fn notice_banner(place: usize, notice: &Notice, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let (name, color) = match notice.level {
        Level::Warning => (IconName::TriangleAlert, theme.warning),
        Level::Error => (IconName::CircleX, theme.danger),
    };
    let words = SharedString::from(notice.text.clone());
    tinted(strip(format!("notice-{place}"), 1, theme), color, theme)
        .child(icon(name).text_color(color))
        .child(clamped(words.clone()))
        .tooltip(move |window, cx| Tooltip::new(words.clone()).build(window, cx))
}

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
                strip("check-drivers-strip", 1, theme)
                    .child(clamped(text::AFTER_INSTALLING.into()))
                    .child(
                        text_button("check-drivers", IconName::RefreshCw, text::CHECK_AGAIN)
                            .on_click(move |_, _, _| send(Command::CheckDrivers)),
                    ),
            ),
    )
}

pub fn text_button(id: impl Into<ElementId>, name: IconName, words: &'static str) -> Button {
    Button::new(id)
        .outline()
        .h(CONTROL)
        .px(GAP)
        .accessibility_label(words)
        .child(
            div()
                .flex()
                .items_center()
                .gap(GAP)
                .text_size(SMALL_TEXT)
                .child(icon(name))
                .child(words),
        )
}

fn driver_banner(missing: &Missing, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let url = missing.driver.installer_url();
    let name = text::name(missing.driver);
    let heading = SharedString::from(text::heading(missing));
    let consequence = text::consequence(missing.driver);
    let tip = SharedString::from(format!("{heading}. {consequence}"));
    let content = div()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_x(GAP)
                .child(div().min_w_0().truncate().child(heading))
                .child(text::SEPARATOR)
                .child(
                    Link::new(name)
                        .href(url)
                        .child(SharedString::from(text::short_url(url))),
                ),
        )
        .child(clamped(consequence.into()).text_color(theme.muted_foreground));
    tinted(strip(name, 2, theme), theme.warning, theme)
        .child(icon(IconName::TriangleAlert).text_color(theme.warning))
        .child(content)
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
}

fn tinted(strip: Stateful<Div>, color: Hsla, theme: &Theme) -> Stateful<Div> {
    strip.bg(color.opacity(0.08)).text_color(theme.foreground)
}

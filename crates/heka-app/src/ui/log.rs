use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, ClipboardItem, Div, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div, px, uniform_list,
};
use gpui_component::button::Button;
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, Theme};
use gpui_kit_assets::IconName;

use crate::log::{Entry, Recent, copied};
use crate::text;
use crate::ui::banners::{clamped, strip};
use crate::ui::detail::tool_button;
use crate::ui::model::{Command, SendCommand};
use crate::ui::panel::faint;

const TIME_WIDTH: f32 = 60.;

pub fn buttons(recent: &Recent, send: &SendCommand) -> [Button; 2] {
    let recent = Arc::clone(recent);
    let send = Rc::clone(send);
    [
        tool_button("copy-log", IconName::Copy, text::COPY_LOG).on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(copied(&recent)));
        }),
        tool_button(
            "open-log-folder",
            IconName::FolderOpen,
            text::OPEN_LOG_FOLDER,
        )
        .on_click(move |_, _, _| send(Command::OpenLogFolder)),
    ]
}

pub fn entries(recent: &Recent, cx: &App) -> Div {
    let body = div().flex().flex_col().flex_1().min_h_0();
    if recent.is_empty() {
        let empty = strip("log-empty", 1, cx.theme())
            .text_color(faint(cx.theme()))
            .child(text::NOTHING_LOGGED);
        return body.child(empty);
    }
    let recent = Arc::clone(recent);
    body.child(
        uniform_list("log", recent.len(), move |range, _, cx| {
            range
                .filter_map(|place| {
                    let newest_first = recent.len().checked_sub(place + 1)?;
                    let entry = recent.get(newest_first)?;
                    Some(line(newest_first, entry, cx.theme()))
                })
                .collect()
        })
        .flex_1(),
    )
}

fn line(place: usize, entry: &Entry, theme: &Theme) -> impl IntoElement + use<> {
    let whole = SharedString::from(entry.line());
    strip(format!("log-{place}"), 1, theme)
        .w_full()
        .child(
            div()
                .flex_none()
                .w(px(TIME_WIDTH))
                .text_color(faint(theme))
                .child(entry.at.time_of_day()),
        )
        .child(clamped(entry.message.clone().into()).text_color(theme.foreground))
        .tooltip(move |window, cx| Tooltip::new(whole.clone()).build(window, cx))
}

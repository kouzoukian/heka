use std::rc::Rc;

use gpui::{App, Div, ParentElement, Styled, div};
use gpui_component::{ActiveTheme, Theme};
use gpui_kit_assets::IconName;
use heka_core::app_settings::{AppChange, CloseAction};

use crate::text;
use crate::ui::banners::text_button;
use crate::ui::detail::row;
use crate::ui::model::{AppEntry, Command, SendCommand};
use crate::ui::segmented::{Segment, on_off, segmented};
use crate::ui::settings::{explained, group};

pub fn app_settings(app: AppEntry, send: &SendCommand, cx: &App) -> Div {
    let theme = cx.theme();
    let line =
        |label: &'static str, tip: &'static str| row(theme).child(explained(label, tip, theme));
    let open = Rc::clone(send);
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(
            line(text::START_WITH_WINDOWS, text::START_WITH_WINDOWS_TIP).child(on_off(
                "start-with-windows",
                app.start_with_windows,
                Command::SetStartWithWindows,
                send,
                theme,
            )),
        )
        .child(
            line(text::ROW_START_IN_TRAY, text::START_IN_TRAY_TIP).child(on_off(
                "start-in-tray",
                app.start_in_tray,
                |on| Command::ChangeAppSetting(AppChange::StartInTray(on)),
                send,
                theme,
            )),
        )
        .child(
            line(text::ROW_CLOSING, text::CLOSING_TIP).child(closing_switch(
                app.on_close,
                send,
                theme,
            )),
        )
        .child(group(text::FILES, theme))
        .child(
            row(theme).child(text::ROW_SETTINGS_FOLDER).child(
                text_button(
                    "open-settings-folder",
                    IconName::FolderOpen,
                    text::OPEN_FOLDER,
                )
                .on_click(move |_, _, _| open(Command::OpenSettingsFolder)),
            ),
        )
}

fn closing_switch(chosen: CloseAction, send: &SendCommand, theme: &Theme) -> Div {
    let choices = [
        ("closing-keep", text::KEEP_RUNNING, CloseAction::KeepRunning),
        ("closing-quit", text::QUIT, CloseAction::Quit),
    ];
    let segments = choices.map(|(id, words, action)| {
        Segment::words(
            id,
            words,
            chosen == action,
            Some(Command::ChangeAppSetting(AppChange::OnClose(action))),
        )
    });
    segmented(segments, send, theme)
}

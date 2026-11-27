use std::rc::Rc;

use gpui::{App, Div, ParentElement, Styled, div};
use gpui_component::{ActiveTheme, Theme};
use gpui_kit_assets::IconName;
use heka_core::app_settings::{AppChange, CloseAction, ThemeChoice};

use crate::text;
use crate::ui::banners::text_button;
use crate::ui::detail::row;
use crate::ui::games::{GamesView, games_section};
use crate::ui::model::{AppEntry, Command, Screen, SendCommand};
use crate::ui::segmented::{Segment, on_off, segmented};
use crate::ui::settings::{explained, group};
use crate::ui::shortcuts::{ShortcutsView, shortcuts_section};

pub struct AppView<'a> {
    pub entry: AppEntry,
    pub games: GamesView<'a>,
    pub shortcuts: ShortcutsView,
}

pub fn app_settings(view: &AppView<'_>, screen: &Screen, send: &SendCommand, cx: &App) -> Div {
    let app = view.entry;
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
        .child(line(text::ROW_THEME, text::THEME_TIP).child(theme_switch(app.theme, send, theme)))
        .child(group(text::UPDATES, theme))
        .child(
            line(text::ROW_CHECK_FOR_RELEASES, text::CHECK_FOR_RELEASES_TIP).child(on_off(
                "check-for-releases",
                app.check_for_releases,
                |on| Command::ChangeAppSetting(AppChange::CheckForReleases(on)),
                send,
                theme,
            )),
        )
        .child(shortcuts_section(&view.shortcuts, screen, send, theme))
        .child(games_section(&view.games, screen, send, theme))
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

fn theme_switch(chosen: ThemeChoice, send: &SendCommand, theme: &Theme) -> Div {
    let choices = [
        ("theme-dark", text::DARK, ThemeChoice::Dark),
        ("theme-light", text::LIGHT, ThemeChoice::Light),
    ];
    let segments = choices.map(|(id, words, choice)| {
        Segment::words(
            id,
            words,
            chosen == choice,
            Some(Command::ChangeAppSetting(AppChange::Theme(choice))),
        )
    });
    segmented(segments, send, theme)
}

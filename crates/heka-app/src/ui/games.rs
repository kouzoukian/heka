use std::rc::Rc;

use gpui::{App, Div, ParentElement, SharedString, Styled, Window, div};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Selectable, Theme};
use gpui_kit_assets::IconName;
use heka_core::game_rules::GameRule;

use crate::text;
use crate::ui::banners::{clamped, strip, text_button};
use crate::ui::detail::{row, tool_button};
use crate::ui::metrics::{GAP, INSET, SMALL_TEXT, icon};
use crate::ui::model::{Command, Screen, SendCommand, profile_name, sorted_profiles};
use crate::ui::panel::faint;
use crate::ui::settings::group;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GamesAction {
    OpenRule(String),
    OpenAdd,
    CloseAdd,
    ChooseFile,
}

pub type GamesAct = Rc<dyn Fn(GamesAction, &mut Window, &mut App)>;

pub struct GamesView<'a> {
    pub rule_open: Option<&'a str>,
    pub programs: Option<&'a [String]>,
    pub act: GamesAct,
}

pub fn programs_to_offer(programs: &[String], rules: &[GameRule]) -> Vec<String> {
    programs
        .iter()
        .filter(|program| rules.iter().all(|rule| rule.program != **program))
        .cloned()
        .collect()
}

pub fn games_section(
    view: &GamesView<'_>,
    screen: &Screen,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let notes = div()
        .flex()
        .flex_col()
        .flex_none()
        .child(strip("games-note", 2, theme).child(clamped(text::GAMES_NOTE.into())))
        .child(strip("games-admin-note", 2, theme).child(clamped(text::GAMES_ADMIN_NOTE.into())));
    let unreadable = screen.foreground_unreadable.then(|| {
        strip("games-unreadable", 2, theme)
            .bg(theme.warning.opacity(0.08))
            .child(icon(IconName::TriangleAlert).text_color(theme.warning))
            .child(clamped(text::FOREGROUND_UNREADABLE.into()))
    });
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(group(text::GROUP_GAMES, theme))
        .child(notes)
        .children(unreadable)
        .children(
            screen
                .game_rules
                .iter()
                .map(|rule| rule_block(rule, view, screen, send, theme)),
        )
        .child(add_block(view, screen, send, theme))
}

fn rule_block(
    rule: &GameRule,
    view: &GamesView<'_>,
    screen: &Screen,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let open = view.rule_open == Some(rule.program.as_str());
    let applies = screen.rule_applies.as_deref() == Some(rule.program.as_str());
    let profile_words: SharedString = profile_name(&screen.profiles, &rule.profile)
        .unwrap_or(text::RULE_PROFILE_MISSING)
        .to_owned()
        .into();
    let toggle = {
        let act = Rc::clone(&view.act);
        let program = rule.program.clone();
        move |_: &_, window: &mut Window, cx: &mut App| {
            act(GamesAction::OpenRule(program.clone()), window, cx);
        }
    };
    let delete = {
        let send = Rc::clone(send);
        let program = rule.program.clone();
        move |_: &_, _: &mut Window, _: &mut App| send(Command::RemoveGameRule(program.clone()))
    };
    let choose = Button::new(SharedString::from(format!("rule-profile-{}", rule.program)))
        .ghost()
        .selected(open)
        .tooltip(text::RULE_PROFILE_TIP)
        .accessibility_label(profile_words.clone())
        .child(div().text_size(SMALL_TEXT).child(profile_words))
        .on_click(toggle);
    let remove = tool_button(
        SharedString::from(format!("rule-delete-{}", rule.program)),
        IconName::Trash,
        text::DELETE_RULE,
    )
    .on_click(delete);
    let line = row(theme)
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(GAP)
                .text_color(theme.foreground)
                .child(icon(IconName::AppWindow))
                .child(SharedString::from(rule.program.clone())),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(GAP)
                .when_applies(applies, theme)
                .child(choose)
                .child(remove),
        );
    let block = div().flex().flex_col().flex_none().child(line);
    if open {
        block.child(profile_options(rule, screen, view, send, theme))
    } else {
        block
    }
}

trait AppliesMarker {
    fn when_applies(self, applies: bool, theme: &Theme) -> Self;
}

impl AppliesMarker for Div {
    fn when_applies(self, applies: bool, theme: &Theme) -> Self {
        if applies {
            self.child(
                div()
                    .text_size(SMALL_TEXT)
                    .text_color(theme.info)
                    .child(text::RULE_APPLIES_NOW),
            )
        } else {
            self
        }
    }
}

fn profile_options(
    rule: &GameRule,
    screen: &Screen,
    view: &GamesView<'_>,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let buttons = sorted_profiles(&screen.profiles)
        .into_iter()
        .map(|profile| {
            let send = Rc::clone(send);
            let act = Rc::clone(&view.act);
            let program = rule.program.clone();
            let id = profile.id.clone();
            let words = profile.profile.name.clone();
            Button::new(SharedString::from(format!(
                "rule-{}-use-{}",
                rule.program, profile.id
            )))
            .outline()
            .selected(rule.profile == profile.id)
            .child(div().text_size(SMALL_TEXT).child(words))
            .on_click(move |_, window, cx| {
                send(Command::SetGameRule(program.clone(), id.clone()));
                act(GamesAction::OpenRule(program.clone()), window, cx);
            })
        });
    options_bar(theme).children(buttons)
}

fn options_bar(theme: &Theme) -> Div {
    div()
        .flex()
        .flex_wrap()
        .gap(GAP)
        .px(INSET)
        .py(GAP)
        .border_b_1()
        .border_color(theme.border)
}

fn add_block(view: &GamesView<'_>, screen: &Screen, send: &SendCommand, theme: &Theme) -> Div {
    let Some(first) = sorted_profiles(&screen.profiles)
        .first()
        .map(|first| first.id.clone())
    else {
        return div().flex().flex_col().flex_none().child(
            strip("rule-no-profiles", 1, theme).child(clamped(text::NO_PROFILES_FOR_RULE.into())),
        );
    };
    let toggle = {
        let act = Rc::clone(&view.act);
        move |_: &_, window: &mut Window, cx: &mut App| act(GamesAction::OpenAdd, window, cx)
    };
    let add = row(theme)
        .justify_start()
        .child(text_button("add-rule", IconName::Plus, text::ADD_RULE).on_click(toggle));
    let block = div().flex().flex_col().flex_none().child(add);
    match view.programs {
        None => block,
        Some(programs) => block.child(programs_panel(&first, programs, view, screen, send, theme)),
    }
}

fn programs_panel(
    profile: &str,
    programs: &[String],
    view: &GamesView<'_>,
    screen: &Screen,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let offered = programs_to_offer(programs, &screen.game_rules);
    let buttons = offered.into_iter().map(|program| {
        let send = Rc::clone(send);
        let act = Rc::clone(&view.act);
        let profile = profile.to_owned();
        let chosen = program.clone();
        Button::new(SharedString::from(format!("rule-add-{program}")))
            .outline()
            .child(div().text_size(SMALL_TEXT).child(program))
            .on_click(move |_, window, cx| {
                send(Command::AddGameRule(chosen.clone(), profile.clone()));
                act(GamesAction::CloseAdd, window, cx);
            })
    });
    let file = {
        let act = Rc::clone(&view.act);
        text_button("rule-choose-file", IconName::FolderOpen, text::CHOOSE_FILE)
            .on_click(move |_, window, cx| act(GamesAction::ChooseFile, window, cx))
    };
    let empty = programs_to_offer(programs, &screen.game_rules).is_empty();
    options_bar(theme)
        .child(
            div()
                .w_full()
                .text_size(SMALL_TEXT)
                .text_color(faint(theme))
                .child(if empty {
                    text::NO_RUNNING_PROGRAMS
                } else {
                    text::RUNNING_PROGRAMS
                }),
        )
        .children(buttons)
        .child(file)
}

use std::rc::Rc;

use gpui::{
    App, Div, ParentElement, SharedString, Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, Selectable, Theme};
use heka_core::profile::Input;
use heka_core::shortcuts::{ShortcutAction, ShortcutChange, Shortcuts, can_be_chorded};

use crate::text;
use crate::ui::banners::{clamped, strip};
use crate::ui::detail::row;
use crate::ui::metrics::{GAP, INSET, SMALL_TEXT};
use crate::ui::model::{Command, Screen, SendCommand};
use crate::ui::segmented::on_off;
use crate::ui::settings::group;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutsAction {
    Toggle(ShortcutAction),
}

pub type ShortcutsAct = Rc<dyn Fn(ShortcutsAction, &mut Window, &mut App)>;

pub struct ShortcutsView {
    pub open: Option<ShortcutAction>,
    pub act: ShortcutsAct,
}

pub fn choosable() -> Vec<Input> {
    Input::ALL
        .into_iter()
        .filter(|&input| can_be_chorded(input))
        .collect()
}

pub fn available(shortcuts: &Shortcuts, action: ShortcutAction, input: Input) -> bool {
    shortcuts
        .taken_by(input)
        .is_none_or(|owner| owner == action)
}

pub fn action_name(action: ShortcutAction) -> &'static str {
    match action {
        ShortcutAction::NextProfile => text::ROW_NEXT_PROFILE,
        ShortcutAction::PreviousProfile => text::ROW_PREVIOUS_PROFILE,
        ShortcutAction::TurnOff => text::ROW_SHORTCUT_TURN_OFF,
    }
}

pub fn shortcuts_section(
    view: &ShortcutsView,
    screen: &Screen,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let shortcuts = screen.shortcuts;
    let note =
        |id: &'static str, words: &'static str| strip(id, 1, theme).child(clamped(words.into()));
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(group(text::GROUP_SHORTCUTS, theme))
        .child(row(theme).child(text::ROW_SHORTCUTS).child(on_off(
            "shortcuts-enabled",
            shortcuts.enabled,
            |on| Command::ChangeShortcut(ShortcutChange::Enabled(on)),
            send,
            theme,
        )))
        .child(note("shortcuts-note", text::SHORTCUTS_NOTE))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .when(!shortcuts.enabled, |this| this.opacity(0.5))
                .children(
                    ShortcutAction::ALL
                        .map(|action| action_block(action, &shortcuts, view, send, theme)),
                ),
        )
        .child(note(
            "shortcuts-turn-off-note",
            text::SHORTCUTS_TURN_OFF_NOTE,
        ))
        .child(note("shortcuts-hidden-note", text::SHORTCUTS_HIDDEN_NOTE))
}

fn action_block(
    action: ShortcutAction,
    shortcuts: &Shortcuts,
    view: &ShortcutsView,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let open = view.open == Some(action);
    let words: SharedString = text::shortcut_choice(shortcuts.button(action)).into();
    let toggle = {
        let act = Rc::clone(&view.act);
        move |_: &_, window: &mut Window, cx: &mut App| {
            act(ShortcutsAction::Toggle(action), window, cx);
        }
    };
    let current = Button::new(SharedString::from(format!("shortcut-{action:?}")))
        .ghost()
        .selected(open)
        .accessibility_label(words.clone())
        .child(div().text_size(SMALL_TEXT).child(words))
        .on_click(toggle);
    let block = div()
        .flex()
        .flex_col()
        .flex_none()
        .child(row(theme).child(action_name(action)).child(current));
    if open {
        block.child(options(action, shortcuts, view, send, theme))
    } else {
        block
    }
}

fn options(
    action: ShortcutAction,
    shortcuts: &Shortcuts,
    view: &ShortcutsView,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let buttons = choosable().into_iter().map(|input| {
        let send = Rc::clone(send);
        let act = Rc::clone(&view.act);
        Button::new(SharedString::from(format!("shortcut-{action:?}-{input:?}")))
            .outline()
            .selected(shortcuts.button(action) == Some(input))
            .disabled(!available(shortcuts, action, input))
            .child(div().text_size(SMALL_TEXT).child(text::input_name(input)))
            .on_click(move |_, window, cx| {
                send(Command::ChangeShortcut(ShortcutChange::Choose(
                    action, input,
                )));
                act(ShortcutsAction::Toggle(action), window, cx);
            })
    });
    div()
        .flex()
        .flex_wrap()
        .gap(GAP)
        .px(INSET)
        .py(GAP)
        .border_b_1()
        .border_color(theme.border)
        .children(buttons)
}

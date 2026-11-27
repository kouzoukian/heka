use std::rc::Rc;

use gpui::{Div, ParentElement, SharedString, Styled, div};
use gpui_component::button::Button;
use gpui_component::{Selectable, Theme};
use gpui_kit_assets::IconName;
use heka_core::desktop::{Chord, Key};
use heka_core::macros::{MacroChange, Step, Target};
use heka_core::profile::{Binding, Input};
use heka_core::stick_use::{Direction, StickUseChange};
use heka_core::tuning::StickSide;

use crate::text;
use crate::ui::metrics::{CONTROL, GAP, SMALL_TEXT, icon};
use crate::ui::model::Command;
use crate::ui::profile::{ProfileAction, ProfileView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureFor {
    Binding(Input),
    StickKey(StickSide, Direction),
    MacroStep(Input, usize, bool),
}

impl CaptureFor {
    pub fn command(self, profile: String, chord: Chord) -> Command {
        match self {
            Self::Binding(input) => Command::BindInput(profile, input, Binding::Keys(chord)),
            Self::StickKey(side, direction) => {
                Command::ChangeStickUse(profile, side, StickUseChange::Key(direction, chord))
            }
            Self::MacroStep(input, index, pressing) => {
                let target = Target::Keys(chord);
                let step = if pressing {
                    Step::Press(target)
                } else {
                    Step::Release(target)
                };
                Command::ChangeMacro(profile, input, MacroChange::Set(index, step))
            }
        }
    }
}

pub fn control_button(id: impl Into<SharedString>, label: SharedString) -> Button {
    Button::new(id.into())
        .outline()
        .h(CONTROL)
        .px(GAP)
        .accessibility_label(label)
}

pub fn capture_control(
    id: String,
    current: Option<Chord>,
    target: CaptureFor,
    view: &ProfileView<'_>,
    theme: &Theme,
) -> Div {
    let capturing = view.capturing == Some(target);
    let words: SharedString = match current {
        _ if capturing => text::CAPTURING_KEY.into(),
        Some(chord) => text::chord_words(chord, view.key_label.as_ref()).into(),
        None => text::CAPTURE_KEY.into(),
    };
    let act = Rc::clone(&view.act);
    let capture = control_button(format!("capture-{id}"), words.clone())
        .selected(capturing)
        .tooltip(text::CAPTURE_TIP)
        .child(
            div()
                .flex()
                .items_center()
                .gap(GAP)
                .text_size(SMALL_TEXT)
                .child(icon(IconName::Keyboard))
                .child(words),
        )
        .on_click(move |_, window, cx| act(ProfileAction::Capture(target), window, cx));
    let escape = capturing.then(|| {
        let act = Rc::clone(&view.act);
        control_button(format!("bind-escape-{id}"), text::BIND_ESCAPE.into())
            .child(div().text_size(SMALL_TEXT).child(text::BIND_ESCAPE))
            .on_click(move |_, window, cx| {
                let escape = Chord::of(Key::ESCAPE);
                act(ProfileAction::Bind(target, escape), window, cx);
            })
    });
    div()
        .flex()
        .gap(GAP)
        .text_color(theme.foreground)
        .children(escape)
        .child(capture)
}

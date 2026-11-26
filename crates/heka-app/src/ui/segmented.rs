//! A segmented control: a few choices side by side in one box, the chosen one
//! filled.

use std::rc::Rc;

use gpui::{
    AnyElement, Div, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Selectable, Theme};

use crate::ui::metrics::{CONTROL, GAP, SMALL_TEXT};
use crate::ui::model::{Command, SendCommand};

/// One choice.
pub struct Segment {
    id: SharedString,
    /// What names it to a screen reader, and in its tooltip if it shows no words.
    label: SharedString,
    content: AnyElement,
    tooltip: bool,
    selected: bool,
    /// What choosing it asks for, `None` when it is already chosen.
    command: Option<Command>,
}

impl Segment {
    /// A choice that shows its words.
    pub fn words(
        id: impl Into<SharedString>,
        words: impl Into<SharedString>,
        selected: bool,
        command: Option<Command>,
    ) -> Self {
        let words = words.into();
        Self {
            id: id.into(),
            label: words.clone(),
            content: div().text_size(SMALL_TEXT).child(words).into_any_element(),
            tooltip: false,
            selected,
            command,
        }
    }

    /// A choice that shows a picture, with `label` in its tooltip.
    pub fn picture(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        picture: impl IntoElement,
        selected: bool,
        command: Option<Command>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            content: picture.into_any_element(),
            tooltip: true,
            selected,
            command,
        }
    }
}

/// The segments in one box, as high as a control.
pub fn segmented(
    segments: impl IntoIterator<Item = Segment>,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let segments = segments.into_iter().map(|segment| {
        let send = Rc::clone(send);
        let command = segment.command;
        Button::new(segment.id)
            .ghost()
            .selected(segment.selected)
            // The switch's border and padding bring it to a control's height.
            .h(CONTROL - px(6.))
            .px(GAP)
            .rounded(px(3.))
            .map(|this| {
                if segment.selected {
                    this.bg(theme.accent).text_color(theme.accent_foreground)
                } else {
                    this.text_color(theme.muted_foreground)
                }
            })
            .accessibility_label(segment.label.clone())
            .when(segment.tooltip, |this| this.tooltip(segment.label))
            .child(segment.content)
            .on_click(move |_, _, _| {
                if let Some(command) = &command {
                    send(command.clone());
                }
            })
    });
    div()
        .flex()
        .gap(px(2.))
        .p(px(2.))
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radius)
        .children(segments)
}

use gpui::{
    Div, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div,
    px, relative,
};
use gpui_component::Theme;
use heka_core::touchpad::{TouchpadChange, TouchpadUse};

use crate::text;
use crate::ui::banners::{clamped, strip};
use crate::ui::detail::row;
use crate::ui::metrics::{GAP, INSET};
use crate::ui::model::{Command, ControllerEntry, ProfileEntry, SendCommand};
use crate::ui::pad::TouchDot;
use crate::ui::segmented::{Segment, on_off, segmented};
use crate::ui::settings::explained;
use crate::ui::sticks::choices;

pub const SENSITIVITIES: [u16; 7] = [25, 50, 75, 100, 150, 200, 300];

const PAD_WIDTH: f32 = 192.;
const PAD_HEIGHT: f32 = 108.;
const DOT: f32 = 12.;

pub fn touchpad_section(
    profile: &ProfileEntry,
    entry: &ControllerEntry,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(readout(entry.raw_touch, theme))
        .child(
            div()
                .id("touchpad-rows")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(rows(profile, send, theme)),
        )
}

fn readout(touch: Option<[Option<TouchDot>; 2]>, theme: &Theme) -> Div {
    let dots = touch.into_iter().flatten().flatten().map(|finger| {
        let (x, y) = finger.share();
        div()
            .absolute()
            .size(px(DOT))
            .rounded_full()
            .bg(theme.info)
            .left(relative(x))
            .top(relative(y))
            .ml(px(-DOT / 2.))
            .mt(px(-DOT / 2.))
    });
    div()
        .flex()
        .flex_none()
        .justify_center()
        .px(INSET)
        .py(GAP)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .relative()
                .w(px(PAD_WIDTH))
                .h(px(PAD_HEIGHT))
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .children(dots),
        )
}

fn rows(profile: &ProfileEntry, send: &SendCommand, theme: &Theme) -> Vec<gpui::AnyElement> {
    let settings = *profile.profile.touchpad();
    let id = profile.id.clone();
    let command = {
        let id = id.clone();
        move |change: TouchpadChange| Command::ChangeTouchpad(id.clone(), change)
    };
    let uses = [
        (TouchpadUse::Controller, text::TOUCHPAD_CONTROLLER),
        (TouchpadUse::Mouse, text::TOUCHPAD_MOUSE),
    ]
    .map(|(used_as, words)| {
        Segment::words(
            format!("touchpad-use-{id}-{used_as:?}"),
            words,
            settings.used_as == used_as,
            Some(command(TouchpadChange::Use(used_as))),
        )
    });
    let mut rows = vec![
        row(theme)
            .child(explained(
                text::ROW_TOUCHPAD_USE,
                text::TOUCHPAD_USE_TIP,
                theme,
            ))
            .child(segmented(uses, send, theme))
            .into_any_element(),
    ];
    if settings.moves_the_mouse() {
        let sensitivities = choices(&SENSITIVITIES, settings.sensitivity)
            .into_iter()
            .map(|value| {
                Segment::words(
                    format!("touchpad-sensitivity-{id}-{value}"),
                    text::percent_of(value),
                    value == settings.sensitivity,
                    Some(command(TouchpadChange::Sensitivity(value))),
                )
            });
        rows.push(
            row(theme)
                .child(explained(
                    text::ROW_SENSITIVITY,
                    text::TOUCHPAD_SENSITIVITY_TIP,
                    theme,
                ))
                .child(segmented(sensitivities, send, theme))
                .into_any_element(),
        );
        let switch = |label: &'static str,
                      tip: &'static str,
                      on: bool,
                      change: fn(bool) -> TouchpadChange| {
            row(theme)
                .child(explained(label, tip, theme))
                .child(on_off(
                    format!("touchpad-{label}-{id}"),
                    on,
                    |value| command(change(value)),
                    send,
                    theme,
                ))
                .into_any_element()
        };
        rows.extend([
            switch(
                text::ROW_INVERT_VERTICAL,
                text::INVERT_VERTICAL_TIP,
                settings.invert_y,
                TouchpadChange::InvertY,
            ),
            switch(
                text::ROW_TAP_CLICK,
                text::TAP_CLICK_TIP,
                settings.tap_click,
                TouchpadChange::TapClick,
            ),
            switch(
                text::ROW_TWO_FINGER_CLICK,
                text::TWO_FINGER_CLICK_TIP,
                settings.two_finger_click,
                TouchpadChange::TwoFingerClick,
            ),
        ]);
        rows.push(
            strip("touchpad-scroll-note", 1, theme)
                .child(clamped(text::TOUCHPAD_SCROLL_NOTE.into()))
                .into_any_element(),
        );
    }
    rows.push(
        strip("touchpad-click-note", 2, theme)
            .child(clamped(text::TOUCHPAD_CLICK_NOTE.into()))
            .into_any_element(),
    );
    rows
}

use std::rc::Rc;

use gpui::{AnyElement, Div, IntoElement, ParentElement, SharedString, Styled, div};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, Selectable};
use gpui_kit_assets::IconName;
use heka_core::desktop::{Chord, Key, MouseButton};
use heka_core::macros::{MAX_STEPS, Macro, MacroChange, Mode, Step, Target, USUAL_RATE};

use crate::text;
use crate::ui::banners::text_button;
use crate::ui::capture::{CaptureFor, capture_control};
use crate::ui::detail::{row, tool_button};
use crate::ui::metrics::{GAP, SMALL_TEXT};
use crate::ui::model::Command;
use crate::ui::profile::{OpenBinding, ProfileAction};
use crate::ui::segmented::{Pick, Segment, picked, segmented};
use crate::ui::settings::{explained, group};
use crate::ui::sticks::choices;

pub const DELAYS: [u16; 6] = [10, 25, 50, 100, 500, 1000];
pub const RATES: [u16; 6] = [2, 5, 10, 15, 20, 30];

const NEW_TARGET: Target = Target::Keys(Chord::of(Key::new(0x10)));

pub(super) fn mode_rows(open: OpenBinding<'_>) -> Vec<Div> {
    let mode = open.profile.profile.mode(open.input);
    let command = |mode: Mode| Command::SetMode(open.profile.id.clone(), open.input, mode);
    let turbo = match mode {
        Mode::Turbo(rate) => rate,
        _ => USUAL_RATE,
    };
    let modes = [Mode::Hold, Mode::Toggle, Mode::Turbo(turbo)].map(|choice| {
        let words = text::mode_name(choice);
        Segment::words(
            open.id(format_args!("mode-{words}")),
            words,
            mode == choice,
            Some(command(choice)),
        )
    });
    let mut rows = vec![
        row(open.theme)
            .child(explained(
                text::ROW_PRESS_MODE,
                text::PRESS_MODE_TIP,
                open.theme,
            ))
            .child(segmented(modes, open.send, open.theme)),
    ];
    if let Mode::Turbo(rate) = mode {
        let rates = choices(&RATES, u16::from(rate)).into_iter().map(|value| {
            Segment::words(
                open.id(format_args!("rate-{value}")),
                value.to_string(),
                value == u16::from(rate),
                Some(command(Mode::Turbo(value as u8))),
            )
        });
        rows.push(
            row(open.theme)
                .child(explained(
                    text::ROW_TURBO_RATE,
                    text::TURBO_RATE_TIP,
                    open.theme,
                ))
                .child(segmented(rates, open.send, open.theme)),
        );
    }
    rows
}

pub(super) fn macro_rows(open: OpenBinding<'_>) -> Vec<AnyElement> {
    let found = open.profile.profile.macro_of(open.input);
    let steps = found.map_or(&[][..], Macro::steps);
    let repeat = found.is_some_and(|found| found.repeat);
    let runs = [(false, text::RUNS_ONCE), (true, text::RUNS_HELD)].map(|(value, words)| {
        Segment::words(
            open.id(format_args!("runs-{words}")),
            words,
            found.is_some() && repeat == value,
            Some(change(open, MacroChange::Repeat(value))),
        )
    });
    let mut rows = vec![
        row(open.theme)
            .child(explained(text::ROW_RUNS, text::RUNS_TIP, open.theme))
            .child(segmented(runs, open.send, open.theme))
            .into_any_element(),
        group(text::GROUP_STEPS, open.theme).into_any_element(),
    ];
    for (index, &step) in steps.iter().enumerate() {
        rows.extend(step_rows(open, index, step, steps.len()));
    }
    let command = change(open, MacroChange::Add);
    let send = Rc::clone(open.send);
    let add = text_button(open.id("add-step"), IconName::Plus, text::ADD_STEP)
        .disabled(steps.len() >= MAX_STEPS)
        .on_click(move |_, _, _| send(command.clone()));
    rows.push(
        row(open.theme)
            .justify_start()
            .child(add)
            .into_any_element(),
    );
    rows
}

fn change(open: OpenBinding<'_>, change: MacroChange) -> Command {
    Command::ChangeMacro(open.profile.id.clone(), open.input, change)
}

fn step_rows(open: OpenBinding<'_>, index: usize, step: Step, count: usize) -> Vec<AnyElement> {
    let target = match step {
        Step::Press(target) | Step::Release(target) => target,
        Step::Wait(_) => NEW_TARGET,
    };
    let wait = match step {
        Step::Wait(millis) => millis,
        _ => 50,
    };
    let kinds = [
        (
            text::STEP_PRESS,
            matches!(step, Step::Press(_)),
            Step::Press(target),
        ),
        (
            text::STEP_RELEASE,
            matches!(step, Step::Release(_)),
            Step::Release(target),
        ),
        (
            text::STEP_WAIT,
            matches!(step, Step::Wait(_)),
            Step::Wait(wait),
        ),
    ]
    .map(|(words, selected, choice)| {
        Segment::words(
            open.id(format_args!("step-{index}-{words}")),
            words,
            selected,
            Some(change(open, MacroChange::Set(index, choice))),
        )
    });
    let tool = |name: IconName, label: &'static str, edit: MacroChange, possible: bool| {
        let command = change(open, edit);
        let send = Rc::clone(open.send);
        tool_button(open.id(format_args!("step-{index}-{label}")), name, label)
            .disabled(!possible)
            .on_click(move |_, _, _| send(command.clone()))
    };
    let head = row(open.theme)
        .child(
            div()
                .flex()
                .items_center()
                .gap(GAP)
                .child(text::step(index + 1))
                .child(segmented(kinds, open.send, open.theme)),
        )
        .child(
            div()
                .flex()
                .child(tool(
                    IconName::ArrowUp,
                    text::MOVE_UP,
                    MacroChange::MoveUp(index),
                    index > 0,
                ))
                .child(tool(
                    IconName::ArrowDown,
                    text::MOVE_DOWN,
                    MacroChange::MoveDown(index),
                    index + 1 < count,
                ))
                .child(tool(
                    IconName::Trash,
                    text::DELETE_STEP,
                    MacroChange::Remove(index),
                    true,
                )),
        )
        .into_any_element();
    let mut rows = vec![head];
    match step {
        Step::Wait(millis) => rows.push(delay_row(open, index, millis).into_any_element()),
        Step::Press(target) => rows.extend(target_rows(open, index, target, true)),
        Step::Release(target) => rows.extend(target_rows(open, index, target, false)),
    }
    rows
}

fn delay_row(open: OpenBinding<'_>, index: usize, millis: u16) -> Div {
    let delays = choices(&DELAYS, millis).into_iter().map(|value| {
        Segment::words(
            open.id(format_args!("step-{index}-delay-{value}")),
            value.to_string(),
            value == millis,
            Some(change(open, MacroChange::Set(index, Step::Wait(value)))),
        )
    });
    row(open.theme)
        .child(explained(text::ROW_DELAY, text::DELAY_TIP, open.theme))
        .child(segmented(delays, open.send, open.theme))
}

fn target_rows(
    open: OpenBinding<'_>,
    index: usize,
    target: Target,
    pressing: bool,
) -> Vec<AnyElement> {
    let shown = open.view.step_open == Some(index);
    let words: SharedString = text::target_words(target, open.view.key_label.as_ref()).into();
    let act = Rc::clone(&open.view.act);
    let current = Button::new(open.id(format_args!("step-{index}-target")))
        .ghost()
        .selected(shown)
        .accessibility_label(words.clone())
        .child(div().text_size(SMALL_TEXT).child(words))
        .on_click(move |_, window, cx| act(ProfileAction::ToggleStep(index), window, cx));
    let head = row(open.theme)
        .child(explained(text::ROW_SENDS, text::SENDS_TIP, open.theme))
        .child(current)
        .into_any_element();
    if !shown {
        return vec![head];
    }
    let chord = match target {
        Target::Keys(chord) => Some(chord),
        Target::Mouse(_) => None,
    };
    let keys = row(open.theme)
        .child(explained(text::ROW_KEYS, text::STEP_KEY_TIP, open.theme))
        .child(capture_control(
            open.id(format_args!("step-{index}")).to_string(),
            chord,
            CaptureFor::MacroStep(open.input, index, pressing),
            open.view,
            open.theme,
        ))
        .into_any_element();
    let buttons = MouseButton::ALL.map(|button| {
        let choice = Target::Mouse(button);
        let step = if pressing {
            Step::Press(choice)
        } else {
            Step::Release(choice)
        };
        let command = change(open, MacroChange::Set(index, step));
        let send = Rc::clone(open.send);
        let act = Rc::clone(&open.view.act);
        let pick: Pick = Rc::new(move |window, cx| {
            send(command.clone());
            act(ProfileAction::ToggleStep(index), window, cx);
        });
        let words = text::mouse_button_short(button);
        (
            open.id(format_args!("step-{index}-mouse-{words}")),
            words,
            target == choice,
            pick,
        )
    });
    let mouse = row(open.theme)
        .child(explained(
            text::ROW_MOUSE_BUTTON,
            text::STEP_MOUSE_TIP,
            open.theme,
        ))
        .child(picked(buttons, open.theme))
        .into_any_element();
    vec![head, keys, mouse]
}

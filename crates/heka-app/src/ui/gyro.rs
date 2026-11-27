use std::rc::Rc;

use gpui::{
    AnyElement, Div, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, div,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, Selectable, Theme};
use gpui_kit_assets::IconName;
use heka_core::gyro::{GyroActivation, GyroAxis, GyroChange, GyroSettings, GyroUse};
use heka_core::profile::Input;
use heka_core::supervisor::DriftStatus;

use crate::text;
use crate::ui::banners::{clamped, strip, text_button};
use crate::ui::detail::row;
use crate::ui::metrics::{GAP, INSET, SMALL_TEXT};
use crate::ui::model::{Command, ControllerEntry, ProfileEntry, SendCommand};
use crate::ui::profile::{ProfileAction, ProfileView};
use crate::ui::segmented::{Segment, on_off, segmented};
use crate::ui::settings::explained;
use crate::ui::sticks::choices;

pub const SENSITIVITIES: [u16; 6] = [25, 50, 100, 150, 200, 300];
pub const DEAD_ZONES: [u16; 5] = [0, 1, 2, 5, 10];

pub fn buttons() -> impl Iterator<Item = Input> {
    Input::ALL
        .into_iter()
        .filter(|input| !matches!(input, Input::TouchpadLeft | Input::TouchpadRight))
}

pub fn can_measure(entry: &ControllerEntry) -> bool {
    entry.active_link.is_some() && entry.drift != DriftStatus::Measuring
}

pub fn gyro_section(
    profile: Option<&ProfileEntry>,
    entry: &ControllerEntry,
    view: &ProfileView<'_>,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let settings = match profile {
        Some(profile) => settings_rows(profile, view, send, theme),
        None => vec![
            strip("gyro-fixed", 2, theme)
                .child(clamped(text::GYRO_DEFAULT.into()))
                .into_any_element(),
        ],
    };
    div()
        .flex()
        .flex_col()
        .flex_none()
        .children(settings)
        .children(drift_rows(entry, send, theme))
}

fn drift_rows(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> [AnyElement; 2] {
    let measuring = entry.drift == DriftStatus::Measuring;
    let words = if measuring {
        text::MEASURING
    } else {
        text::MEASURE
    };
    let command = Command::MeasureDrift(entry.mac);
    let send = Rc::clone(send);
    let button = text_button(
        SharedString::from(format!("measure-drift-{}", entry.mac)),
        IconName::RefreshCw,
        words,
    )
    .disabled(!can_measure(entry))
    .on_click(move |_, _, _| send(command.clone()));
    [
        row(theme)
            .child(explained(text::ROW_DRIFT, text::DRIFT_TIP, theme))
            .child(div().text_color(theme.foreground).child(button))
            .into_any_element(),
        strip("drift-note", 2, theme)
            .child(clamped(text::drift_note(entry.drift).into()))
            .into_any_element(),
    ]
}

fn settings_rows(
    profile: &ProfileEntry,
    view: &ProfileView<'_>,
    send: &SendCommand,
    theme: &Theme,
) -> Vec<AnyElement> {
    let settings = *profile.profile.gyro();
    let id = profile.id.clone();
    let command = {
        let id = id.clone();
        move |change: GyroChange| Command::ChangeGyro(id.clone(), change)
    };
    let choice_row =
        |label: &'static str, tip: &'static str, choices: Vec<(SharedString, bool, GyroChange)>| {
            let segments = choices.into_iter().map(|(words, selected, change)| {
                Segment::words(
                    format!("gyro-{label}-{words}-{id}"),
                    words,
                    selected,
                    Some(command(change)),
                )
            });
            row(theme)
                .child(explained(label, tip, theme))
                .child(segmented(segments, send, theme))
                .into_any_element()
        };
    let uses = [GyroUse::Off, GyroUse::Mouse, GyroUse::RightStick].map(|used_as| {
        (
            text::gyro_use_name(used_as).into(),
            settings.used_as == used_as,
            GyroChange::Use(used_as),
        )
    });
    let mut rows = vec![choice_row(
        text::ROW_GYRO_USE,
        text::GYRO_USE_TIP,
        uses.into(),
    )];
    if settings.used_as == GyroUse::Off {
        return rows;
    }
    let activations = [
        GyroActivation::Always,
        GyroActivation::WhileHeld,
        GyroActivation::Toggle,
    ]
    .map(|activation| {
        (
            text::activation_name(activation).into(),
            settings.activation == activation,
            GyroChange::Activation(activation),
        )
    });
    rows.push(choice_row(
        text::ROW_ACTIVE,
        text::ACTIVE_TIP,
        activations.into(),
    ));
    if settings.activation != GyroActivation::Always {
        rows.extend(button_rows(&id, &settings, view, send, theme));
    }
    let axes = [GyroAxis::Yaw, GyroAxis::Roll].map(|axis| {
        (
            text::axis_name(axis).into(),
            settings.axis == axis,
            GyroChange::Axis(axis),
        )
    });
    rows.push(choice_row(text::ROW_AXIS, text::AXIS_TIP, axes.into()));
    let sensitivity = |current: u16, change: fn(u16) -> GyroChange| {
        choices(&SENSITIVITIES, current)
            .into_iter()
            .map(|value| {
                (
                    text::percent_of(value).into(),
                    value == current,
                    change(value),
                )
            })
            .collect()
    };
    rows.push(choice_row(
        text::ROW_SENSITIVITY_X,
        text::SENSITIVITY_X_TIP,
        sensitivity(settings.sensitivity_x, GyroChange::SensitivityX),
    ));
    rows.push(choice_row(
        text::ROW_SENSITIVITY_Y,
        text::SENSITIVITY_Y_TIP,
        sensitivity(settings.sensitivity_y, GyroChange::SensitivityY),
    ));
    let dead_zones = choices(&DEAD_ZONES, u16::from(settings.dead_zone))
        .into_iter()
        .map(|value| {
            (
                text::degrees_a_second(value).into(),
                value == u16::from(settings.dead_zone),
                GyroChange::DeadZone(value as u8),
            )
        })
        .collect();
    rows.push(choice_row(
        text::ROW_DEAD_ZONE,
        text::GYRO_DEAD_ZONE_TIP,
        dead_zones,
    ));
    let invert = |label: &'static str, on: bool, change: fn(bool) -> GyroChange| {
        row(theme)
            .child(label)
            .child(on_off(
                format!("gyro-{label}-{id}"),
                on,
                |value| command(change(value)),
                send,
                theme,
            ))
            .into_any_element()
    };
    rows.push(invert(
        text::ROW_INVERT_X,
        settings.invert_x,
        GyroChange::InvertX,
    ));
    rows.push(invert(
        text::ROW_INVERT_Y,
        settings.invert_y,
        GyroChange::InvertY,
    ));
    rows
}

fn button_rows(
    id: &str,
    settings: &GyroSettings,
    view: &ProfileView<'_>,
    send: &SendCommand,
    theme: &Theme,
) -> Vec<AnyElement> {
    let open = view.gyro_button_open;
    let words: SharedString = text::input_name(settings.button).into();
    let toggle = Rc::clone(&view.act);
    let current = Button::new(SharedString::from(format!("gyro-button-{id}")))
        .ghost()
        .selected(open)
        .accessibility_label(words.clone())
        .child(div().text_size(SMALL_TEXT).child(words))
        .on_click(move |_, window, cx| toggle(ProfileAction::ToggleGyroButton, window, cx));
    let mut rows = vec![
        row(theme)
            .child(explained(
                text::ROW_GYRO_BUTTON,
                text::GYRO_BUTTON_TIP,
                theme,
            ))
            .child(current)
            .into_any_element(),
    ];
    if open {
        let choices = buttons().map(|input| {
            let command = Command::ChangeGyro(id.to_owned(), GyroChange::Button(input));
            let send = Rc::clone(send);
            let act = Rc::clone(&view.act);
            let words = text::input_name(input);
            Button::new(SharedString::from(format!("gyro-button-{id}-{input:?}")))
                .outline()
                .selected(settings.button == input)
                .accessibility_label(words)
                .child(div().text_size(SMALL_TEXT).child(words))
                .on_click(move |_, window, cx| {
                    send(command.clone());
                    act(ProfileAction::ToggleGyroButton, window, cx);
                })
        });
        rows.push(
            div()
                .id("gyro-buttons")
                .flex()
                .flex_wrap()
                .gap(GAP)
                .px(INSET)
                .py(GAP)
                .border_b_1()
                .border_color(theme.border)
                .children(choices)
                .into_any_element(),
        );
    }
    rows
}

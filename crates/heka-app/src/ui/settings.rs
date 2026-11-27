use std::rc::Rc;

use gpui::{
    App, Div, Entity, InteractiveElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::color_picker::ColorPickerState;
use gpui_component::input::{Input, InputState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, Disableable, Sizable, Theme};
use gpui_kit_assets::IconName;
use heka_core::feedback::{Lightbar, Motor, PlayerLights};
use heka_core::idle::IdleTime;
use heka_core::preferences::{LightbarChoice, PreferenceChange, RumbleStrength};

use crate::text;
use crate::ui::banners::{clamped, strip, text_button};
use crate::ui::detail::row;
use crate::ui::metrics::{GAP, SMALL_TEXT};
use crate::ui::model::{ControllerEntry, SendCommand};
use crate::ui::panel::faint;
use crate::ui::picker::{picker, rainbow_swatch, swatch};
use crate::ui::segmented::{Segment, on_off, segmented};

const NAME_WIDTH: f32 = 220.;

const STRENGTHS: [u8; 5] = [0, 25, 50, 75, 100];

const COLOURS: [(&str, Lightbar); 6] = [
    (text::BLUE, colour(0, 0, 255)),
    (text::RED, colour(255, 0, 0)),
    (text::GREEN, colour(0, 255, 0)),
    (text::PINK, colour(255, 0, 128)),
    (text::ORANGE, colour(255, 96, 0)),
    (text::WHITE, colour(255, 255, 255)),
];

const fn colour(red: u8, green: u8, blue: u8) -> Lightbar {
    Lightbar { red, green, blue }
}

#[derive(Clone, Copy)]
pub struct Fields<'a> {
    pub name: &'a Entity<InputState>,
    pub colour: &'a Entity<ColorPickerState>,
}

pub fn settings(entry: &ControllerEntry, fields: Fields, send: &SendCommand, cx: &App) -> Div {
    let theme = cx.theme();
    let line =
        |label: &'static str, tip: &'static str| row(theme).child(explained(label, tip, theme));
    let player_lights = entry.has_player_lights.then(|| {
        line(text::ROW_PLAYER_LIGHTS, text::PLAYER_LIGHTS_TIP)
            .child(player_lights_switch(entry, send, theme))
    });
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(
            line(text::ROW_NAME, text::NAME_TIP).child(
                div()
                    .w(px(NAME_WIDTH))
                    .text_color(theme.foreground)
                    .child(Input::new(fields.name).small()),
            ),
        )
        .child(line(text::ROW_HIDE, text::HIDE_TIP).child(hide_switch(entry, send, theme)))
        .child(group(text::GROUP_RUMBLE, theme))
        .child(
            line(text::ROW_STRENGTH, text::STRENGTH_TIP).child(strength_switch(entry, send, theme)),
        )
        .child(
            line(text::ROW_SWAP_MOTORS, text::SWAP_MOTORS_TIP)
                .child(swap_switch(entry, send, theme)),
        )
        .child(line(text::ROW_RUMBLE_TEST, text::RUMBLE_TEST_TIP).child(test_buttons(entry, send)))
        .child(group(text::GROUP_LIGHTS, theme))
        .child(
            line(text::ROW_LIGHTBAR, text::LIGHTBAR_TIP).child(lightbar_switch(
                entry,
                fields.colour,
                send,
                cx,
            )),
        )
        .children(player_lights)
        .child(group(text::GROUP_POWER, theme))
        .child(line(text::ROW_IDLE, text::IDLE_TIP).child(idle_switch(entry, send, theme)))
        .child(strip("settings-note", 1, theme).child(clamped(text::SETTINGS_NOTE.into())))
}

pub(super) fn group(words: &'static str, theme: &Theme) -> Div {
    row(theme)
        .justify_start()
        .bg(theme.sidebar)
        .text_size(SMALL_TEXT)
        .text_color(faint(theme))
        .child(words)
}

pub(super) fn explained(label: &'static str, tip: &'static str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(label)
        .flex()
        .items_center()
        .h_full()
        .text_color(theme.muted_foreground)
        .child(label)
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
}

fn id(entry: &ControllerEntry, row: &str, choice: impl std::fmt::Display) -> SharedString {
    format!("{row}-{choice}-{}", entry.mac).into()
}

fn strength_switch(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> Div {
    let segments = STRENGTHS.map(|percent| {
        let strength = RumbleStrength::percent(percent);
        let words: SharedString = if percent == 0 {
            text::OFF.into()
        } else {
            text::percent(percent).into()
        };
        Segment::words(
            id(entry, "rumble", percent),
            words,
            entry.preferences.rumble == strength,
            Some(entry.prefer(PreferenceChange::Rumble(strength))),
        )
    });
    segmented(segments, send, theme)
}

fn swap_switch(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> Div {
    on_off(
        format!("swap-{}", entry.mac),
        entry.preferences.swap_motors,
        |swap| entry.prefer(PreferenceChange::SwapMotors(swap)),
        send,
        theme,
    )
}

fn test_buttons(entry: &ControllerEntry, send: &SendCommand) -> Div {
    let motors = [
        (text::LARGE_MOTOR, Motor::Large),
        (text::SMALL_MOTOR, Motor::Small),
    ];
    let buttons = motors.map(|(words, motor)| {
        let command = entry.test_rumble(motor);
        let send = Rc::clone(send);
        text_button(id(entry, "test", words), IconName::Play, words)
            .disabled(command.is_none())
            .on_click(move |_, _, _| {
                if let Some(command) = &command {
                    send(command.clone());
                }
            })
    });
    div().flex().gap(GAP).children(buttons)
}

fn lightbar_switch(
    entry: &ControllerEntry,
    colour_picker: &Entity<ColorPickerState>,
    send: &SendCommand,
    cx: &App,
) -> Div {
    let chosen = entry.preferences.lightbar;
    let default = Segment::words(
        id(entry, "lightbar", text::DEFAULT),
        text::DEFAULT,
        chosen.is_none(),
        Some(entry.prefer(PreferenceChange::Lightbar(None))),
    );
    let colours = COLOURS.map(|(name, light)| {
        Segment::picture(
            id(entry, "lightbar", name),
            name,
            swatch(light),
            chosen == Some(LightbarChoice::Colour(light)),
            Some(
                entry.prefer(PreferenceChange::Lightbar(Some(LightbarChoice::Colour(
                    light,
                )))),
            ),
        )
    });
    let rainbow = Segment::picture(
        id(entry, "lightbar", text::RAINBOW),
        text::RAINBOW,
        rainbow_swatch(),
        chosen == Some(LightbarChoice::RAINBOW),
        Some(entry.prefer(PreferenceChange::Lightbar(Some(LightbarChoice::RAINBOW)))),
    );
    let custom = match chosen {
        Some(LightbarChoice::Colour(colour)) => {
            Some(colour).filter(|colour| COLOURS.iter().all(|(_, light)| light != colour))
        }
        _ => None,
    };
    let segments = std::iter::once(default).chain(colours).chain([rainbow]);
    segmented(segments, send, cx.theme()).child(picker(colour_picker, custom, cx))
}

fn player_lights_switch(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> Div {
    let choices = [
        (text::DEFAULT, None),
        ("1", Some(PlayerLights::One)),
        ("2", Some(PlayerLights::Two)),
        ("3", Some(PlayerLights::Three)),
        ("4", Some(PlayerLights::Four)),
    ];
    let segments = choices.map(|(words, lights)| {
        Segment::words(
            id(entry, "player", words),
            words,
            entry.preferences.player_lights == lights,
            Some(entry.prefer(PreferenceChange::PlayerLights(lights))),
        )
    });
    segmented(segments, send, theme)
}

fn idle_switch(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> Div {
    let choices = std::iter::once(None).chain(IdleTime::ALL.map(Some));
    let segments = choices.map(|time| {
        let words: SharedString =
            time.map_or(text::OFF.into(), |time| text::idle_time(time).into());
        Segment::words(
            id(entry, "idle", &words),
            words,
            entry.idle_time == time,
            Some(entry.choose_idle_time(time)),
        )
    });
    segmented(segments, send, theme)
}

fn hide_switch(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> Div {
    on_off(
        format!("hide-{}", entry.mac),
        entry.preferences.hide,
        |hide| entry.prefer(PreferenceChange::Hide(hide)),
        send,
        theme,
    )
}

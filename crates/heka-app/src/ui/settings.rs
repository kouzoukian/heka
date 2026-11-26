//! The selected Controller's settings, one row each: its name, rumble, lights and
//! whether it is Hidden. A tooltip on each label explains the row.
//!
//! The rows are the pane's, so that they stay level with the panel's.

use gpui::{
    App, Div, Entity, InteractiveElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::color_picker::ColorPickerState;
use gpui_component::input::{Input, InputState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, Sizable, Theme};
use heka_core::feedback::{Lightbar, PlayerLights};
use heka_core::preferences::RumbleStrength;

use crate::text;
use crate::ui::banners::{clamped, strip};
use crate::ui::detail::row;
use crate::ui::model::{ControllerEntry, SendCommand};
use crate::ui::picker::{picker, swatch};
use crate::ui::segmented::{Segment, segmented};

/// How wide the name's field is.
const NAME_WIDTH: f32 = 220.;

/// The rumble strengths on offer, in percent.
const STRENGTHS: [u8; 5] = [0, 25, 50, 75, 100];

/// The lightbar colours on offer.
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

/// What the settings' fields remember between redraws, which the view keeps.
#[derive(Clone, Copy)]
pub struct Fields<'a> {
    pub name: &'a Entity<InputState>,
    /// The picker for a lightbar colour of the player's own.
    pub colour: &'a Entity<ColorPickerState>,
}

/// The settings of `entry`, typed and picked in `fields`.
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
        .child(line(text::ROW_RUMBLE, text::RUMBLE_TIP).child(rumble_switch(entry, send, theme)))
        .child(
            line(text::ROW_LIGHTBAR, text::LIGHTBAR_TIP).child(lightbar_switch(
                entry,
                fields.colour,
                send,
                cx,
            )),
        )
        .children(player_lights)
        .child(line(text::ROW_HIDE, text::HIDE_TIP).child(hide_switch(entry, send, theme)))
        .child(strip("settings-note", 1, theme).child(clamped(text::SETTINGS_NOTE.into())))
}

/// A row's label, which says what the row is for when pointed at.
fn explained(label: &'static str, tip: &'static str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(label)
        .flex()
        .items_center()
        .h_full()
        .text_color(theme.muted_foreground)
        .child(label)
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
}

/// An id of its own for each Controller's choice, so that each keeps its own hover.
fn id(entry: &ControllerEntry, row: &str, choice: impl std::fmt::Display) -> SharedString {
    format!("{row}-{choice}-{}", entry.mac).into()
}

fn rumble_switch(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> Div {
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
            entry.prefer(|preferences| preferences.rumble = strength),
        )
    });
    segmented(segments, send, theme)
}

/// Default, a few colours, then the picker for any other.
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
        entry.prefer(|preferences| preferences.lightbar = None),
    );
    let colours = COLOURS.map(|(name, light)| {
        Segment::picture(
            id(entry, "lightbar", name),
            name,
            swatch(light),
            chosen == Some(light),
            entry.prefer(|preferences| preferences.lightbar = Some(light)),
        )
    });
    let custom = chosen.filter(|chosen| COLOURS.iter().all(|(_, light)| light != chosen));
    segmented(std::iter::once(default).chain(colours), send, cx.theme()).child(picker(
        colour_picker,
        custom,
        cx,
    ))
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
            entry.prefer(|preferences| preferences.player_lights = lights),
        )
    });
    segmented(segments, send, theme)
}

fn hide_switch(entry: &ControllerEntry, send: &SendCommand, theme: &Theme) -> Div {
    let segments = [(text::ON, true), (text::OFF, false)].map(|(words, hide)| {
        Segment::words(
            id(entry, "hide", words),
            words,
            entry.preferences.hide == hide,
            entry.prefer(|preferences| preferences.hide = hide),
        )
    });
    segmented(segments, send, theme)
}

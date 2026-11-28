use std::rc::Rc;

use gpui::{
    Div, Hsla, InteractiveElement, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_component::Theme;
use heka_core::trigger_effect::{BREAK_LAST_END, EffectChange, EffectKind, EffectSettings};
use heka_core::tuning::{Curve, TriggerChange, TriggerSide, TriggerTuning};

use crate::text;
use crate::ui::banners::{clamped, strip};
use crate::ui::detail::row;
use crate::ui::metrics::{GAP, INSET, SMALL_TEXT};
use crate::ui::model::{Command, ControllerEntry, ProfileEntry, SendCommand};
use crate::ui::panel::faint;
use crate::ui::profile::{ProfileAct, ProfileAction};
use crate::ui::segmented::{Pick, Segment, picked, segmented};
use crate::ui::settings::{explained, group};
use crate::ui::sticks::{DEAD_ZONES, MAX_ZONES, choices};

pub const HAIR_POINTS: [u16; 5] = [10, 20, 30, 50, 70];
pub const PRESS_POINTS: [u16; 7] = [20, 30, 40, 50, 60, 70, 80];
pub const EFFECT_STARTS: [u16; 6] = [0, 10, 30, 50, 70, 90];
pub const BREAK_STARTS: [u16; 6] = [20, 30, 40, 50, 60, 70];
pub const BREAK_ENDS: [u16; 6] = [30, 40, 50, 60, 70, 80];
pub const EFFECT_STRENGTHS: [u16; 5] = [20, 40, 60, 80, 100];

const BAR_HEIGHT: f32 = 96.;
const BAR_WIDTH: f32 = 24.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TravelReadout {
    pub raw: u8,
    pub tuned: u8,
}

pub fn share(travel: u8) -> f32 {
    f32::from(travel) / 255.
}

pub fn point_share(point: u8) -> f32 {
    f32::from(point) / 100.
}

pub fn point_choices(presets: &[u16], current: Option<u8>) -> Vec<u16> {
    match current {
        Some(point) => choices(presets, u16::from(point)),
        None => presets.to_vec(),
    }
}

pub fn effect_starts(effect: &EffectSettings) -> Vec<u16> {
    let presets = match effect.effect {
        EffectKind::Break => &BREAK_STARTS,
        _ => &EFFECT_STARTS,
    };
    choices(presets, u16::from(effect.start))
}

pub fn effect_ends(effect: &EffectSettings) -> Vec<u16> {
    BREAK_ENDS
        .into_iter()
        .filter(|&end| end > u16::from(effect.start) && end <= u16::from(BREAK_LAST_END))
        .collect()
}

pub fn readout(raw: Option<[u8; 2]>, shown: Option<(u8, u8)>, side: TriggerSide) -> TravelReadout {
    let pick = |pair: (u8, u8)| match side {
        TriggerSide::Left => pair.0,
        TriggerSide::Right => pair.1,
    };
    TravelReadout {
        raw: raw.map_or(0, |raw| pick((raw[0], raw[1]))),
        tuned: shown.map_or(0, pick),
    }
}

pub fn triggers_section(
    profile: &ProfileEntry,
    entry: &ControllerEntry,
    side: TriggerSide,
    act: &ProfileAct,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let tuning = *profile.profile.trigger(side);
    let shown = entry.pad.map(|pad| (pad.left_trigger, pad.right_trigger));
    let travel = readout(entry.raw_triggers, shown, side);
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(side_switch(side, act, theme))
        .child(readout_block(travel, &tuning, entry.report_rate, theme))
        .child(
            div()
                .id("trigger-rows")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(rows(profile, side, &tuning, send, theme))
                .child(effect_rows(profile, side, send, theme)),
        )
}

fn effect_rows(
    profile: &ProfileEntry,
    side: TriggerSide,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let id = &profile.id;
    let effect = *profile.profile.trigger_effect(side);
    let command = |change: EffectChange| Command::ChangeTriggerEffect(id.clone(), side, change);
    let kinds = EffectKind::ALL.map(|kind| {
        Segment::words(
            format!("effect-{id}-{side:?}-{kind:?}"),
            text::effect_name(kind),
            effect.effect == kind,
            Some(command(EffectChange::Kind(kind))),
        )
        .explained(text::effect_tip(kind))
    });
    let percent_row = |label: &'static str,
                       tip: &'static str,
                       key: &'static str,
                       values: Vec<u16>,
                       current: u8,
                       change: fn(u8) -> EffectChange| {
        let segments = values.into_iter().map(|value| {
            Segment::words(
                format!("effect-{id}-{side:?}-{key}-{value}"),
                text::percent_of(value),
                value == u16::from(current),
                Some(command(change(value as u8))),
            )
        });
        row(theme)
            .child(explained(label, tip, theme))
            .child(segmented(segments, send, theme))
    };
    let start = (effect.effect != EffectKind::Off).then(|| {
        percent_row(
            text::ROW_EFFECT_START,
            text::EFFECT_START_TIP,
            "start",
            effect_starts(&effect),
            effect.start,
            EffectChange::Start,
        )
    });
    let end = (effect.effect == EffectKind::Break).then(|| {
        percent_row(
            text::ROW_EFFECT_END,
            text::EFFECT_END_TIP,
            "end",
            effect_ends(&effect),
            effect.end,
            EffectChange::End,
        )
    });
    let strength = (effect.effect != EffectKind::Off).then(|| {
        percent_row(
            text::ROW_EFFECT_STRENGTH,
            text::EFFECT_STRENGTH_TIP,
            "strength",
            choices(&EFFECT_STRENGTHS, u16::from(effect.strength)),
            effect.strength,
            EffectChange::Strength,
        )
    });
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(group(text::GROUP_EFFECT, theme))
        .child(
            row(theme)
                .child(explained(text::ROW_EFFECT, text::EFFECT_TIP, theme))
                .child(segmented(kinds, send, theme)),
        )
        .children(start)
        .children(end)
        .children(strength)
        .child(strip("trigger-effect-note", 1, theme).child(clamped(text::EFFECT_NOTE.into())))
}

fn side_switch(side: TriggerSide, act: &ProfileAct, theme: &Theme) -> Div {
    let items = [
        (TriggerSide::Left, text::SIDE_LEFT_TRIGGER),
        (TriggerSide::Right, text::SIDE_RIGHT_TRIGGER),
    ]
    .map(|(this, words)| {
        let act = Rc::clone(act);
        let pick: Pick =
            Rc::new(move |window, cx| act(ProfileAction::TriggerSide(this), window, cx));
        (
            SharedString::from(format!("trigger-side-{this:?}")),
            words,
            side == this,
            pick,
        )
    });
    row(theme).justify_start().child(picked(items, theme))
}

fn readout_block(
    travel: TravelReadout,
    tuning: &TriggerTuning,
    rate: Option<u32>,
    theme: &Theme,
) -> Div {
    let figure = |colour: Hsla, label: &'static str, value: u8| {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(INSET)
            .child(div().text_color(colour).child(label))
            .child(text::travel_percent(value))
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(INSET)
        .px(INSET)
        .py(GAP)
        .border_b_1()
        .border_color(theme.border)
        .child(bar(travel.raw, tuning.hair_trigger, faint(theme), theme))
        .child(bar(travel.tuned, None, theme.info, theme))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(GAP)
                .min_w(px(120.))
                .text_size(SMALL_TEXT)
                .child(figure(faint(theme), text::READOUT_RAW, travel.raw))
                .child(figure(theme.info, text::READOUT_TUNED, travel.tuned))
                .child(
                    div()
                        .text_color(faint(theme))
                        .child(text::report_rate(rate)),
                ),
        )
}

fn bar(travel: u8, hair_trigger: Option<u8>, colour: Hsla, theme: &Theme) -> Div {
    let fill = div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(BAR_HEIGHT * share(travel)))
        .bg(colour);
    let line = hair_trigger.map(|point| {
        div()
            .absolute()
            .left_0()
            .right_0()
            .h(px(1.))
            .bottom(px(BAR_HEIGHT * point_share(point)))
            .bg(theme.warning)
    });
    div()
        .relative()
        .flex_none()
        .w(px(BAR_WIDTH))
        .h(px(BAR_HEIGHT))
        .border_1()
        .border_color(theme.border)
        .child(fill)
        .children(line)
}

fn rows(
    profile: &ProfileEntry,
    side: TriggerSide,
    tuning: &TriggerTuning,
    send: &SendCommand,
    theme: &Theme,
) -> Vec<Div> {
    let id = profile.id.clone();
    let command = {
        let id = id.clone();
        move |change: TriggerChange| Command::ChangeTrigger(id.clone(), side, change)
    };
    let zone_row = |label: &'static str,
                    tip: &'static str,
                    key: &'static str,
                    presets: &[u16],
                    current: u8,
                    change: fn(u8) -> TriggerChange| {
        let segments = choices(presets, u16::from(current))
            .into_iter()
            .map(|value| {
                Segment::words(
                    format!("trigger-{id}-{side:?}-{key}-{value}"),
                    text::percent_of(value),
                    value == u16::from(current),
                    Some(command(change(value as u8))),
                )
            });
        row(theme)
            .child(explained(label, tip, theme))
            .child(segmented(segments, send, theme))
    };
    let point_row = |label: &'static str,
                     tip: &'static str,
                     key: &'static str,
                     none_words: &'static str,
                     presets: &[u16],
                     current: Option<u8>,
                     change: fn(Option<u8>) -> TriggerChange| {
        let none = Segment::words(
            format!("trigger-{id}-{side:?}-{key}-none"),
            none_words,
            current.is_none(),
            Some(command(change(None))),
        );
        let points = point_choices(presets, current).into_iter().map(|value| {
            Segment::words(
                format!("trigger-{id}-{side:?}-{key}-{value}"),
                text::percent_of(value),
                current.map(u16::from) == Some(value),
                Some(command(change(Some(value as u8)))),
            )
        });
        row(theme)
            .child(explained(label, tip, theme))
            .child(segmented(std::iter::once(none).chain(points), send, theme))
    };
    let curve_segments = Curve::ALL.map(|curve| {
        Segment::words(
            format!("trigger-{id}-{side:?}-curve-{curve:?}"),
            text::curve_name(curve),
            tuning.curve == curve,
            Some(command(TriggerChange::Curve(curve))),
        )
    });
    vec![
        zone_row(
            text::ROW_DEAD_ZONE,
            text::DEAD_ZONE_TIP,
            "dead",
            &DEAD_ZONES,
            tuning.dead_zone,
            TriggerChange::DeadZone,
        ),
        zone_row(
            text::ROW_MAX_ZONE,
            text::MAX_ZONE_TIP,
            "max",
            &MAX_ZONES,
            tuning.max_zone,
            TriggerChange::MaxZone,
        ),
        row(theme)
            .child(explained(text::ROW_CURVE, text::CURVE_TIP, theme))
            .child(segmented(curve_segments, send, theme)),
        point_row(
            text::ROW_HAIR_TRIGGER,
            text::HAIR_TRIGGER_TIP,
            "hair",
            text::OFF,
            &HAIR_POINTS,
            tuning.hair_trigger,
            TriggerChange::HairTrigger,
        ),
        point_row(
            text::ROW_PRESS_POINT,
            text::PRESS_POINT_TIP,
            "press",
            text::PRESS_CLICK,
            &PRESS_POINTS,
            tuning.press_point,
            TriggerChange::PressPoint,
        ),
    ]
}

use std::rc::Rc;

use gpui::{
    Div, Hsla, InteractiveElement, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Selectable, Theme};
use heka_core::state::Stick;
use heka_core::stick_use::{Direction, StickUse, StickUseChange};
use heka_core::tuning::{Curve, StickChange, StickSide, StickTuning};

use crate::text;
use crate::ui::banners::{clamped, strip};
use crate::ui::capture::{CaptureFor, capture_control};
use crate::ui::detail::row;
use crate::ui::metrics::{GAP, INSET, SMALL_TEXT};
use crate::ui::model::{Command, ControllerEntry, ProfileEntry, SendCommand};
use crate::ui::pad::stick_offset;
use crate::ui::panel::faint;
use crate::ui::profile::{ProfileAct, ProfileAction, ProfileView};
use crate::ui::segmented::{Pick, Segment, on_off, picked, segmented};
use crate::ui::settings::explained;

pub const DEAD_ZONES: [u16; 6] = [0, 5, 10, 15, 20, 30];
pub const ANTI_DEAD_ZONES: [u16; 4] = [0, 5, 10, 20];
pub const MAX_ZONES: [u16; 5] = [100, 95, 90, 80, 70];
pub const SENSITIVITIES: [u16; 6] = [50, 75, 100, 125, 150, 200];

const PLOT: f32 = 110.;
const RADIUS: f32 = 46.;
const DOT: f32 = 8.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Readout {
    pub raw: Stick,
    pub tuned: Stick,
}

pub fn choices(presets: &[u16], current: u16) -> Vec<u16> {
    let mut all = presets.to_vec();
    if !all.contains(&current) {
        all.push(current);
    }
    all.sort_unstable();
    all
}

pub fn plot_offset(stick: Stick) -> (f32, f32) {
    let (x, y) = stick_offset(stick);
    (x.clamp(-1., 1.), y.clamp(-1., 1.))
}

pub fn readout(
    raw: Option<[Stick; 2]>,
    shown: Option<(Stick, Stick)>,
    side: StickSide,
    swapped: bool,
) -> Readout {
    let pick = |sticks: (Stick, Stick), side: StickSide| match side {
        StickSide::Left => sticks.0,
        StickSide::Right => sticks.1,
    };
    let other = match side {
        StickSide::Left => StickSide::Right,
        StickSide::Right => StickSide::Left,
    };
    let raw = raw.map_or(Stick::CENTER, |raw| pick((raw[0], raw[1]), side));
    let shown_side = if swapped { other } else { side };
    let tuned = shown.map_or(Stick::CENTER, |shown| pick(shown, shown_side));
    Readout { raw, tuned }
}

pub const SPEEDS: [u16; 6] = [50, 75, 100, 150, 200, 300];

pub fn sticks_section(
    profile: &ProfileEntry,
    entry: &ControllerEntry,
    view: &ProfileView<'_>,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let side = view.side;
    let tuning = *profile.profile.stick(side);
    let shown = entry.pad.map(|pad| (pad.left_stick, pad.right_stick));
    let sticks = readout(entry.raw_sticks, shown, side, profile.profile.swap_sticks);
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(side_switch(side, &view.act, theme))
        .child(readout_block(sticks, &tuning, entry.report_rate, theme))
        .child(
            div()
                .id("stick-rows")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(use_block(profile, view, send, theme))
                .children(rows(profile, view, &tuning, send, theme)),
        )
}

fn side_switch(side: StickSide, act: &ProfileAct, theme: &Theme) -> Div {
    let items = [
        (StickSide::Left, text::SIDE_LEFT),
        (StickSide::Right, text::SIDE_RIGHT),
    ]
    .map(|(this, words)| {
        let act = Rc::clone(act);
        let pick: Pick = Rc::new(move |window, cx| act(ProfileAction::Side(this), window, cx));
        (
            SharedString::from(format!("stick-side-{this:?}")),
            words,
            side == this,
            pick,
        )
    });
    row(theme).justify_start().child(picked(items, theme))
}

fn readout_block(sticks: Readout, tuning: &StickTuning, rate: Option<u32>, theme: &Theme) -> Div {
    let legend = |colour: Hsla, words: &'static str| {
        div()
            .flex()
            .items_center()
            .gap(GAP)
            .child(div().size(px(DOT)).rounded_full().bg(colour))
            .child(words)
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
        .child(plot(sticks, tuning, theme))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(GAP)
                .text_size(SMALL_TEXT)
                .child(legend(faint(theme), text::READOUT_RAW))
                .child(legend(theme.info, text::READOUT_TUNED))
                .child(
                    div()
                        .text_color(faint(theme))
                        .child(text::report_rate(rate)),
                ),
        )
}

fn plot(sticks: Readout, tuning: &StickTuning, theme: &Theme) -> Div {
    div()
        .relative()
        .flex_none()
        .size(px(PLOT))
        .rounded_full()
        .border_1()
        .border_color(theme.border)
        .child(ring(f32::from(tuning.dead_zone) / 100., faint(theme)))
        .child(ring(f32::from(tuning.max_zone) / 100., faint(theme)))
        .child(dot(plot_offset(sticks.raw), faint(theme)))
        .child(dot(plot_offset(sticks.tuned), theme.info))
}

fn ring(fraction: f32, colour: Hsla) -> Div {
    let radius = fraction * RADIUS;
    div()
        .absolute()
        .size(px(2. * radius))
        .rounded_full()
        .border_1()
        .border_color(colour.opacity(0.4))
        .left(px(PLOT / 2. - radius))
        .top(px(PLOT / 2. - radius))
}

fn dot(at: (f32, f32), colour: Hsla) -> Div {
    div()
        .absolute()
        .size(px(DOT))
        .rounded_full()
        .bg(colour)
        .left(px(PLOT / 2. + at.0 * RADIUS - DOT / 2.))
        .top(px(PLOT / 2. + at.1 * RADIUS - DOT / 2.))
}

fn use_block(
    profile: &ProfileEntry,
    view: &ProfileView<'_>,
    send: &SendCommand,
    theme: &Theme,
) -> Vec<Div> {
    let side = view.side;
    let used_as = profile.profile.stick_use(side).used_as;
    let open = view.stick_use_open;
    let words = text::stick_use_name(used_as);
    let toggle = Rc::clone(&view.act);
    let current = Button::new(SharedString::from(format!(
        "stick-use-{}-{side:?}",
        profile.id
    )))
    .ghost()
    .selected(open)
    .accessibility_label(words)
    .child(div().text_size(SMALL_TEXT).child(words))
    .on_click(move |_, window, cx| toggle(ProfileAction::ToggleStickUse, window, cx));
    let label = match side {
        StickSide::Left => text::ROW_LEFT_STICK_USE,
        StickSide::Right => text::ROW_RIGHT_STICK_USE,
    };
    let mut block = vec![
        row(theme)
            .child(explained(label, text::STICK_USE_TIP, theme))
            .child(current),
    ];
    if open {
        let choices = StickUse::ALL.map(|choice| {
            let words = text::stick_use_name(choice);
            let command =
                Command::ChangeStickUse(profile.id.clone(), side, StickUseChange::Use(choice));
            let send = Rc::clone(send);
            let act = Rc::clone(&view.act);
            Button::new(SharedString::from(format!(
                "stick-use-{}-{side:?}-{choice:?}",
                profile.id
            )))
            .outline()
            .selected(choice == used_as)
            .accessibility_label(words)
            .child(div().text_size(SMALL_TEXT).child(words))
            .on_click(move |_, window, cx| {
                send(command.clone());
                act(ProfileAction::ToggleStickUse, window, cx);
            })
        });
        block.push(
            div()
                .flex()
                .flex_wrap()
                .gap(GAP)
                .px(INSET)
                .py(GAP)
                .border_b_1()
                .border_color(theme.border)
                .children(choices),
        );
    }
    block
}

fn key_rows(
    profile: &ProfileEntry,
    view: &ProfileView<'_>,
    theme: &Theme,
) -> impl Iterator<Item = Div> {
    let side = view.side;
    let role = profile.profile.stick_use(side);
    Direction::ALL.into_iter().map(move |direction| {
        row(theme)
            .child(explained(
                text::direction_name(direction),
                text::STICK_KEY_TIP,
                theme,
            ))
            .child(capture_control(
                format!("stick-key-{}-{side:?}-{direction:?}", profile.id),
                role.key(direction),
                CaptureFor::StickKey(side, direction),
                view,
                theme,
            ))
    })
}

fn rows(
    profile: &ProfileEntry,
    view: &ProfileView<'_>,
    tuning: &StickTuning,
    send: &SendCommand,
    theme: &Theme,
) -> Vec<Div> {
    let side = view.side;
    let role = *profile.profile.stick_use(side);
    let id = profile.id.clone();
    let command = {
        let id = id.clone();
        move |change: StickChange| Command::ChangeStick(id.clone(), side, change)
    };
    let preset_row = |label: &'static str,
                      tip: &'static str,
                      key: &'static str,
                      presets: &[u16],
                      current: u16,
                      change: &dyn Fn(u16) -> StickChange| {
        let segments = choices(presets, current).into_iter().map(|value| {
            Segment::words(
                format!("stick-{id}-{side:?}-{key}-{value}"),
                text::percent_of(value),
                value == current,
                Some(command(change(value))),
            )
        });
        row(theme)
            .child(explained(label, tip, theme))
            .child(segmented(segments, send, theme))
    };
    let curve_segments = || {
        Curve::ALL.map(|curve| {
            Segment::words(
                format!("stick-{id}-{side:?}-curve-{curve:?}"),
                text::curve_name(curve),
                tuning.curve == curve,
                Some(command(StickChange::Curve(curve))),
            )
        })
    };
    let invert =
        |label: &'static str, key: &'static str, on: bool, change: fn(bool) -> StickChange| {
            row(theme).child(label).child(on_off(
                format!("stick-{id}-{side:?}-{key}"),
                on,
                |value| command(change(value)),
                send,
                theme,
            ))
        };
    let dead_zone = |tip: &'static str| {
        preset_row(
            text::ROW_DEAD_ZONE,
            tip,
            "dead",
            &DEAD_ZONES,
            u16::from(tuning.dead_zone),
            &|value| StickChange::DeadZone(value as u8),
        )
    };
    let curve = || {
        row(theme)
            .child(explained(text::ROW_CURVE, text::CURVE_TIP, theme))
            .child(segmented(curve_segments(), send, theme))
    };
    let rests = || {
        div()
            .flex()
            .flex_col()
            .flex_none()
            .child(strip("stick-rests", 1, theme).child(clamped(text::STICK_RESTS.into())))
    };
    match role.used_as {
        StickUse::Mouse | StickUse::Scroll => {
            let speeds = choices(&SPEEDS, role.speed).into_iter().map(|value| {
                Segment::words(
                    format!("stick-{id}-{side:?}-speed-{value}"),
                    text::percent_of(value),
                    value == role.speed,
                    Some(Command::ChangeStickUse(
                        id.clone(),
                        side,
                        StickUseChange::Speed(value),
                    )),
                )
            });
            vec![
                rests(),
                dead_zone(text::POINTING_DEAD_ZONE_TIP),
                row(theme)
                    .child(explained(text::ROW_SPEED, text::SPEED_TIP, theme))
                    .child(segmented(speeds, send, theme)),
                curve(),
            ]
        }
        StickUse::Wasd | StickUse::Arrows => {
            vec![rests(), dead_zone(text::KEY_DEAD_ZONE_TIP)]
        }
        StickUse::Keys => [rests(), dead_zone(text::KEY_DEAD_ZONE_TIP)]
            .into_iter()
            .chain(key_rows(profile, view, theme))
            .collect(),
        StickUse::Stick => vec![
            dead_zone(text::DEAD_ZONE_TIP),
            preset_row(
                text::ROW_ANTI_DEAD_ZONE,
                text::ANTI_DEAD_ZONE_TIP,
                "anti",
                &ANTI_DEAD_ZONES,
                u16::from(tuning.anti_dead_zone),
                &|value| StickChange::AntiDeadZone(value as u8),
            ),
            preset_row(
                text::ROW_MAX_ZONE,
                text::MAX_ZONE_TIP,
                "max",
                &MAX_ZONES,
                u16::from(tuning.max_zone),
                &|value| StickChange::MaxZone(value as u8),
            ),
            preset_row(
                text::ROW_SENSITIVITY,
                text::SENSITIVITY_TIP,
                "sens",
                &SENSITIVITIES,
                tuning.sensitivity,
                &StickChange::Sensitivity,
            ),
            curve(),
            invert(
                text::ROW_INVERT_X,
                "invx",
                tuning.invert_x,
                StickChange::InvertX,
            ),
            invert(
                text::ROW_INVERT_Y,
                "invy",
                tuning.invert_y,
                StickChange::InvertY,
            ),
            row(theme)
                .child(explained(
                    text::ROW_SWAP_STICKS,
                    text::SWAP_STICKS_TIP,
                    theme,
                ))
                .child(on_off(
                    format!("stick-{id}-swap"),
                    profile.profile.swap_sticks,
                    |swap| Command::SetSwapSticks(id.clone(), swap),
                    send,
                    theme,
                )),
        ],
    }
}

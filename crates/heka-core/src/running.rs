use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::desktop::{Desk, DesktopState, Hands, MouseButton, Nudge};
use crate::gyro::{FULL_STICK_DEGREES_A_SECOND, GyroActivation, GyroUse, POINTER_PIXELS_A_DEGREE};
use crate::macros::{Macro, Mode, Step, Target};
use crate::profile::{Input, Mapped, Mapper};
use crate::sensor::SensorClock;
use crate::state::{Buttons, ControllerState};
use crate::touchpad::{Gesture, Gestures};
use crate::tuning::push_right_stick;

const LONGEST_STEP: Duration = Duration::from_millis(100);
const CLICK_TIME: Duration = Duration::from_millis(30);
const SHORTEST_CYCLE: Duration = Duration::from_millis(10);

#[derive(Debug, Default)]
struct Carry(f32);

impl Carry {
    fn add(&mut self, amount: f32) -> i32 {
        self.0 += amount;
        let whole = self.0.trunc();
        self.0 -= whole;
        whole as i32
    }

    fn at_rate(&mut self, rate: f32, seconds: f32) -> i32 {
        if rate == 0.0 {
            self.0 = 0.0;
        }
        self.add(rate * seconds)
    }
}

#[derive(Debug, Default)]
struct Travel {
    pointer: [Carry; 2],
    scroll: [Carry; 2],
}

impl Travel {
    fn over(&mut self, pointer: (f32, f32), scroll: (f32, f32), seconds: Option<f32>) -> Nudge {
        let take = |carry: &mut Carry, amount: f32| match seconds {
            Some(seconds) => carry.at_rate(amount, seconds),
            None => carry.add(amount),
        };
        let [x, y] = &mut self.pointer;
        let [across, along] = &mut self.scroll;
        Nudge {
            pointer: (take(x, pointer.0), take(y, pointer.1)),
            scroll: (take(across, scroll.0), take(along, scroll.1)),
        }
    }
}

#[derive(Debug, Default)]
struct Aim {
    clock: SensorClock,
    travel: Travel,
    drift: [i32; 3],
    held: bool,
    toggled: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct Aimed {
    nudge: Nudge,
    stick: Option<(f32, f32)>,
}

impl Aim {
    fn follow(&mut self, mapper: &Mapper, state: &ControllerState, now: Instant) -> Aimed {
        let elapsed = self.clock.elapsed(state.motion.timestamp, now);
        let Some(settings) = mapper.aiming_gyro() else {
            return Aimed::default();
        };
        let held = mapper.pressed(settings.button, state);
        let pressed = held && !self.held;
        self.held = held;
        self.toggled ^= pressed;
        let active = match settings.activation {
            GyroActivation::Always => true,
            GyroActivation::WhileHeld => held,
            GyroActivation::Toggle => self.toggled,
        };
        let (x, y) = if active {
            settings.rates(state.motion.gyro, self.drift)
        } else {
            (0.0, 0.0)
        };
        if settings.used_as == GyroUse::RightStick {
            let full = FULL_STICK_DEGREES_A_SECOND;
            return Aimed {
                stick: ((x, y) != (0.0, 0.0)).then(|| (x / full, y / full)),
                ..Aimed::default()
            };
        }
        let pixels = POINTER_PIXELS_A_DEGREE;
        let nudge = self.travel.over(
            (x * pixels, y * pixels),
            (0.0, 0.0),
            Some(elapsed.as_secs_f32()),
        );
        Aimed { nudge, stick: None }
    }

    fn start_over(&mut self, mapper: &Mapper, latest: Option<&ControllerState>) {
        self.held = mapper.aiming_gyro().is_some_and(|settings| {
            latest.is_some_and(|state| mapper.pressed(settings.button, state))
        });
        self.toggled = false;
        self.travel = Travel::default();
    }
}

struct MacroRun {
    steps: Arc<Macro>,
    hands: Hands,
    held: Vec<Target>,
    next: usize,
    due: Instant,
    began: Instant,
}

impl MacroRun {
    fn new(steps: Arc<Macro>, desk: Arc<Desk>, now: Instant) -> Self {
        Self {
            steps,
            hands: Hands::new(desk),
            held: Vec::new(),
            next: 0,
            due: now,
            began: now,
        }
    }

    fn advance(&mut self, now: Instant) {
        let steps = self.steps.steps();
        if steps.is_empty() {
            return;
        }
        loop {
            if self.next == steps.len() {
                if !self.steps.repeat {
                    return;
                }
                self.next = 0;
                self.due = self.due.max(self.began + SHORTEST_CYCLE);
            }
            if self.due > now {
                return;
            }
            if now.saturating_duration_since(self.due) > LONGEST_STEP {
                self.due = now;
            }
            if self.next == 0 {
                self.began = self.due;
            }
            match steps[self.next] {
                Step::Wait(millis) => self.due += Duration::from_millis(millis.into()),
                Step::Press(target) => {
                    if !self.held.contains(&target) {
                        self.held.push(target);
                    }
                    hold(&mut self.hands, &self.held);
                }
                Step::Release(target) => {
                    self.held.retain(|&held| held != target);
                    hold(&mut self.hands, &self.held);
                }
            }
            self.next += 1;
        }
    }
}

fn hold(hands: &mut Hands, targets: &[Target]) {
    let mut desktop = DesktopState::default();
    for &target in targets {
        target.add_to(&mut desktop);
    }
    hands.hold(desktop);
}

#[derive(Default)]
struct Modes {
    sources: u32,
    latched: u32,
    turning: [Option<Instant>; Input::ALL.len()],
    runs: [Option<MacroRun>; Input::ALL.len()],
    cancelled: Vec<MacroRun>,
}

impl Modes {
    fn follow(&mut self, mapper: &Mapper, sources: u32, desk: &Arc<Desk>, now: Instant) -> u32 {
        let rose = sources & !self.sources;
        self.sources = sources;
        let mut effective = sources;
        for input in Input::ALL {
            let bit = input.bit();
            let held = sources & bit != 0;
            match mapper.mode(input) {
                Mode::Hold => {}
                Mode::Toggle => {
                    self.latched ^= rose & bit;
                    effective = effective & !bit | self.latched & bit;
                }
                Mode::Turbo(rate) => {
                    let since = &mut self.turning[input.index()];
                    if held {
                        let since = *since.get_or_insert(now);
                        if !turbo_presses(rate, now.saturating_duration_since(since)) {
                            effective &= !bit;
                        }
                    } else {
                        *since = None;
                    }
                }
            }
            let run = &mut self.runs[input.index()];
            if !held {
                self.cancelled.extend(run.take());
            } else if rose & bit != 0
                && let Some(steps) = mapper.shared_macro(input)
            {
                *run = Some(MacroRun::new(Arc::clone(steps), Arc::clone(desk), now));
            }
        }
        effective
    }

    fn act(&mut self, now: Instant) {
        self.cancelled.clear();
        for run in self.runs.iter_mut().flatten() {
            run.advance(now);
        }
    }
}

fn turbo_presses(rate: u8, held_for: Duration) -> bool {
    let period = 1_000_000 / u128::from(rate);
    held_for.as_micros() % period < period / 2
}

pub struct RunningMapper {
    desk: Arc<Desk>,
    hands: Hands,
    modes: Modes,
    sticks: Travel,
    fingers: Travel,
    gestures: Gestures,
    aim: Aim,
    aimed_stick: Option<(f32, f32)>,
    click: Option<(MouseButton, Instant)>,
    latest: Option<ControllerState>,
    reported: Option<Instant>,
    now: Instant,
    shown: Option<ControllerState>,
    desktop: DesktopState,
    nudge: Nudge,
}

impl RunningMapper {
    pub fn new(desk: Arc<Desk>) -> Self {
        Self {
            hands: Hands::new(Arc::clone(&desk)),
            desk,
            modes: Modes::default(),
            sticks: Travel::default(),
            fingers: Travel::default(),
            gestures: Gestures::default(),
            aim: Aim::default(),
            aimed_stick: None,
            click: None,
            latest: None,
            reported: None,
            now: Instant::now(),
            shown: None,
            desktop: DesktopState::default(),
            nudge: Nudge::default(),
        }
    }

    pub fn set_drift(&mut self, drift: [i32; 3]) {
        self.aim.drift = drift;
    }

    pub fn start_over(&mut self, mapper: &Mapper, latest: Option<&ControllerState>) {
        let sources = latest.map_or(0, |state| mapper.sources(state));
        self.modes = Modes {
            sources,
            ..Modes::default()
        };
        self.aim.start_over(mapper, latest);
        self.aimed_stick = None;
        self.desktop = latest
            .map(|state| {
                let held = sources & !mapper.toggled_inputs();
                mapper.drive(state, held).desktop
            })
            .unwrap_or_default();
        self.hands.start_over(self.desktop);
        self.sticks = Travel::default();
        self.fingers = Travel::default();
        self.gestures = Gestures::default();
        if latest.is_some_and(|state| state.touch.iter().any(Option::is_some)) {
            self.gestures.ignore_until_lifted();
        }
        self.click = None;
        self.nudge = Nudge::default();
    }

    pub fn report(
        &mut self,
        mapper: &Mapper,
        state: &ControllerState,
        now: Instant,
    ) -> ControllerState {
        let elapsed = self.reported.map_or(Duration::ZERO, |before| {
            now.saturating_duration_since(before).min(LONGEST_STEP)
        });
        self.reported = Some(now);
        let gesture = mapper
            .mouse_touchpad()
            .map_or_else(Gesture::default, |settings| {
                let clicked = state.buttons.contains(Buttons::TOUCHPAD);
                self.gestures.follow(settings, state.touch, clicked, now)
            });
        if let Some(button) = gesture.tap {
            self.click = Some((button, now + CLICK_TIME));
        }
        let velocity = mapper.velocity(state);
        let aimed = self.aim.follow(mapper, state, now);
        self.aimed_stick = aimed.stick;
        self.nudge = self.sticks.over(
            velocity.pointer,
            velocity.scroll,
            Some(elapsed.as_secs_f32()),
        ) + self.fingers.over(gesture.pointer, gesture.scroll, None)
            + aimed.nudge;
        self.latest = Some(*state);
        let shown = self.compose(mapper, state, now);
        self.shown = Some(shown);
        shown
    }

    pub fn tick(&mut self, mapper: &Mapper, now: Instant) -> Option<ControllerState> {
        let latest = self.latest?;
        let shown = self.compose(mapper, &latest, now);
        (self.shown.replace(shown) != Some(shown)).then_some(shown)
    }

    fn compose(
        &mut self,
        mapper: &Mapper,
        state: &ControllerState,
        now: Instant,
    ) -> ControllerState {
        self.now = now;
        if self.click.is_some_and(|(_, until)| now >= until) {
            self.click = None;
        }
        let sources = mapper.sources(state);
        let effective = self.modes.follow(mapper, sources, &self.desk, now);
        let Mapped { mut shown, desktop } = mapper.drive(state, effective);
        self.desktop = desktop;
        if let Some(push) = self.aimed_stick {
            push_right_stick(&mut shown, push);
        }
        shown
    }

    pub fn act(&mut self) {
        let mut desktop = self.desktop;
        if let Some((button, _)) = self.click {
            desktop.add_button(button);
        }
        self.hands.hold(desktop);
        self.modes.act(self.now);
        self.hands.nudge(std::mem::take(&mut self.nudge));
    }

    pub fn let_go(&mut self) {
        self.modes = Modes::default();
        self.latest = None;
        self.click = None;
        self.desktop = DesktopState::default();
        self.nudge = Nudge::default();
        self.hands.let_go();
    }
}

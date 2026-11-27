use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::desktop::{Desk, DesktopState, Hands, MouseButton, Nudge};
use crate::profile::{Mapped, Mapper};
use crate::state::{Buttons, ControllerState};
use crate::touchpad::{Gesture, Gestures};

const LONGEST_STEP: Duration = Duration::from_millis(100);
const CLICK_TIME: Duration = Duration::from_millis(30);

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

pub struct RunningMapper {
    hands: Hands,
    sticks: Travel,
    fingers: Travel,
    gestures: Gestures,
    click: Option<(MouseButton, Instant)>,
    reported: Option<Instant>,
    desktop: DesktopState,
    nudge: Nudge,
}

impl RunningMapper {
    pub fn new(desk: Arc<Desk>) -> Self {
        Self {
            hands: Hands::new(desk),
            sticks: Travel::default(),
            fingers: Travel::default(),
            gestures: Gestures::default(),
            click: None,
            reported: None,
            desktop: DesktopState::default(),
            nudge: Nudge::default(),
        }
    }

    pub fn start_over(&mut self, mapper: &Mapper, latest: Option<&ControllerState>) {
        self.desktop = latest
            .map(|state| mapper.mapped(state).desktop)
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
        let Mapped { shown, desktop } = mapper.mapped(state);
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
        self.nudge = self.sticks.over(
            velocity.pointer,
            velocity.scroll,
            Some(elapsed.as_secs_f32()),
        ) + self.fingers.over(gesture.pointer, gesture.scroll, None);
        self.desktop = desktop;
        self.tick(mapper, now);
        shown
    }

    pub fn tick(&mut self, _: &Mapper, now: Instant) -> Option<ControllerState> {
        if self.click.is_some_and(|(_, until)| now >= until) {
            self.click = None;
        }
        None
    }

    pub fn act(&mut self) {
        let mut desktop = self.desktop;
        if let Some((button, _)) = self.click {
            desktop.add_button(button);
        }
        self.hands.hold(desktop);
        self.hands.nudge(std::mem::take(&mut self.nudge));
    }

    pub fn let_go(&mut self) {
        self.click = None;
        self.hands.let_go();
    }
}

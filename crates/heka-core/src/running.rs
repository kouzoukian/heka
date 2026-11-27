use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::desktop::{Desk, DesktopState, Hands, Nudge};
use crate::profile::{Mapped, Mapper};
use crate::state::ControllerState;
use crate::stick_use::Velocity;

const LONGEST_STEP: Duration = Duration::from_millis(100);

#[derive(Debug, Default)]
struct Carry(f32);

impl Carry {
    fn whole(&mut self, rate: f32, seconds: f32) -> i32 {
        if rate == 0.0 {
            self.0 = 0.0;
            return 0;
        }
        self.0 += rate * seconds;
        let whole = self.0.trunc();
        self.0 -= whole;
        whole as i32
    }
}

#[derive(Debug, Default)]
struct Travel {
    pointer: [Carry; 2],
    scroll: [Carry; 2],
}

impl Travel {
    fn over(&mut self, velocity: Velocity, elapsed: Duration) -> Nudge {
        let seconds = elapsed.as_secs_f32();
        let [x, y] = &mut self.pointer;
        let [across, along] = &mut self.scroll;
        Nudge {
            pointer: (
                x.whole(velocity.pointer.0, seconds),
                y.whole(velocity.pointer.1, seconds),
            ),
            scroll: (
                across.whole(velocity.scroll.0, seconds),
                along.whole(velocity.scroll.1, seconds),
            ),
        }
    }
}

pub struct RunningMapper {
    hands: Hands,
    travel: Travel,
    reported: Option<Instant>,
    desktop: DesktopState,
    nudge: Nudge,
}

impl RunningMapper {
    pub fn new(desk: Arc<Desk>) -> Self {
        Self {
            hands: Hands::new(desk),
            travel: Travel::default(),
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
        self.travel = Travel::default();
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
        self.desktop = desktop;
        self.nudge = self.travel.over(mapper.velocity(state), elapsed);
        shown
    }

    pub fn act(&mut self) {
        self.hands.hold(self.desktop);
        self.hands.nudge(std::mem::take(&mut self.nudge));
    }

    pub fn let_go(&mut self) {
        self.hands.let_go();
    }
}

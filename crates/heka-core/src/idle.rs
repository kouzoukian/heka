use std::time::{Duration, Instant};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::state::{ControllerState, Finger, Stick};

const STICK_THRESHOLD: u8 = 8;
const TRIGGER_THRESHOLD: u8 = 8;
const TOUCH_THRESHOLD: u16 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(into = "u64")]
pub enum IdleTime {
    Five,
    Ten,
    Fifteen,
    Thirty,
}

impl IdleTime {
    pub const ALL: [Self; 4] = [Self::Five, Self::Ten, Self::Fifteen, Self::Thirty];

    pub fn minutes(self) -> u64 {
        match self {
            Self::Five => 5,
            Self::Ten => 10,
            Self::Fifteen => 15,
            Self::Thirty => 30,
        }
    }

    pub fn duration(self) -> Duration {
        Duration::from_secs(self.minutes() * 60)
    }

    pub(crate) fn offered<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Self>, D::Error> {
        let minutes = Value::deserialize(deserializer)?.as_u64();
        Ok(Self::ALL
            .into_iter()
            .find(|time| Some(time.minutes()) == minutes))
    }
}

impl From<IdleTime> for u64 {
    fn from(time: IdleTime) -> Self {
        time.minutes()
    }
}

#[derive(Debug, Default)]
pub struct Activity {
    last: Option<ControllerState>,
}

impl Activity {
    pub fn observe(&mut self, state: &ControllerState) -> bool {
        let Some(last) = &self.last else {
            self.last = Some(*state);
            return false;
        };
        let used = differs(last, state);
        if used {
            self.last = Some(*state);
        }
        used
    }
}

fn differs(from: &ControllerState, to: &ControllerState) -> bool {
    let stick = |a: Stick, b: Stick| {
        a.x.abs_diff(b.x) > STICK_THRESHOLD || a.y.abs_diff(b.y) > STICK_THRESHOLD
    };
    let trigger = |a: u8, b: u8| a.abs_diff(b) > TRIGGER_THRESHOLD;
    let finger = |a: Option<Finger>, b: Option<Finger>| match (a, b) {
        (Some(a), Some(b)) => {
            a.id != b.id
                || a.x.abs_diff(b.x) > TOUCH_THRESHOLD
                || a.y.abs_diff(b.y) > TOUCH_THRESHOLD
        }
        (a, b) => a != b,
    };
    from.buttons != to.buttons
        || stick(from.left_stick, to.left_stick)
        || stick(from.right_stick, to.right_stick)
        || trigger(from.left_trigger, to.left_trigger)
        || trigger(from.right_trigger, to.right_trigger)
        || finger(from.touch[0], to.touch[0])
        || finger(from.touch[1], to.touch[1])
}

#[derive(Debug, Default)]
pub struct IdleTimer {
    since: Option<(Instant, u64)>,
}

impl IdleTimer {
    pub fn runs_out(&mut self, limit: Duration, times_used: u64, now: Instant) -> bool {
        let ran_out = match self.since {
            Some((since, seen)) if seen == times_used => {
                if now.saturating_duration_since(since) < limit {
                    return false;
                }
                true
            }
            _ => false,
        };
        self.since = Some((now, times_used));
        ran_out
    }

    pub fn restart(&mut self) {
        self.since = None;
    }
}

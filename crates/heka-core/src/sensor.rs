use std::time::{Duration, Instant};

const WRAP_MASK: u32 = (1 << 20) - 1;
const NANOSECONDS_IN_THREE_TICKS: u64 = 1000;
const LONGEST_GAP: Duration = Duration::from_millis(200);

#[derive(Debug, Default)]
pub struct SensorClock {
    last: Option<(u32, Instant)>,
}

impl SensorClock {
    pub fn elapsed(&mut self, timestamp: u32, now: Instant) -> Duration {
        let last = self.last.replace((timestamp, now));
        let Some((before, arrived)) = last else {
            return Duration::ZERO;
        };
        if now.saturating_duration_since(arrived) > LONGEST_GAP {
            return Duration::ZERO;
        }
        let ticks = u64::from(timestamp.wrapping_sub(before) & WRAP_MASK);
        Duration::from_nanos(ticks * NANOSECONDS_IN_THREE_TICKS / 3)
    }
}

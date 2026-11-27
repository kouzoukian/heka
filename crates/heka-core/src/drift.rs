use std::time::{Duration, Instant};

use crate::sensor::SensorClock;
use crate::state::Motion;

const MEASURING_TIME: Duration = Duration::from_secs(1);
const GIVING_UP_AFTER: Duration = Duration::from_secs(5);
const FEWEST_REPORTS: u32 = 20;
const MOST_SPREAD: i32 = 4 * 1024;
const MOST_DRIFT: i64 = 20 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftError {
    Moved,
    NoInput,
}

#[derive(Debug)]
pub struct DriftMeter {
    started: Instant,
    clock: SensorClock,
    measured: Duration,
    reports: u32,
    sum: [i64; 3],
    low: [i32; 3],
    high: [i32; 3],
}

impl DriftMeter {
    pub fn new(now: Instant) -> Self {
        Self {
            started: now,
            clock: SensorClock::default(),
            measured: Duration::ZERO,
            reports: 0,
            sum: [0; 3],
            low: [i32::MAX; 3],
            high: [i32::MIN; 3],
        }
    }

    pub fn is_overdue(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) > GIVING_UP_AFTER
    }

    pub fn add(&mut self, motion: &Motion, now: Instant) -> Option<Result<[i32; 3], DriftError>> {
        self.measured += self.clock.elapsed(motion.timestamp, now);
        self.reports += 1;
        for (axis, rate) in motion.gyro.into_iter().enumerate() {
            self.sum[axis] += i64::from(rate);
            self.low[axis] = self.low[axis].min(rate);
            self.high[axis] = self.high[axis].max(rate);
        }
        let spread = (0..3).any(|axis| self.high[axis] - self.low[axis] > MOST_SPREAD);
        if spread {
            return Some(Err(DriftError::Moved));
        }
        if self.measured < MEASURING_TIME || self.reports < FEWEST_REPORTS {
            return None;
        }
        let mean = self.sum.map(|sum| sum / i64::from(self.reports));
        if mean.iter().any(|rate| rate.abs() > MOST_DRIFT) {
            return Some(Err(DriftError::Moved));
        }
        Some(Ok(mean.map(|rate| rate as i32)))
    }
}

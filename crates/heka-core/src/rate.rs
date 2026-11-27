use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(1);

#[derive(Debug, Default)]
pub struct RateMeter {
    since: Option<Instant>,
    count: u32,
    rate: Option<f32>,
}

impl RateMeter {
    pub fn tick(&mut self, now: Instant) {
        let Some(since) = self.since else {
            self.since = Some(now);
            return;
        };
        self.count += 1;
        let elapsed = now.saturating_duration_since(since);
        if elapsed >= WINDOW {
            self.rate = Some(self.count as f32 / elapsed.as_secs_f32());
            self.since = Some(now);
            self.count = 0;
        }
    }

    pub fn per_second(&self) -> Option<f32> {
        self.rate
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

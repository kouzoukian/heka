use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use crate::drift::{DriftError, DriftMeter};
use crate::state::Motion;

pub type Measurement = Result<[i32; 3], DriftError>;

#[derive(Debug, Default)]
pub struct LiveGyro {
    version: AtomicU64,
    drift: Mutex<[i32; 3]>,
    wanted: AtomicBool,
    measured: Mutex<Option<Measurement>>,
}

impl LiveGyro {
    pub fn set_drift(&self, drift: [i32; 3]) {
        *lock(&self.drift) = drift;
        self.version.fetch_add(1, Ordering::Release);
    }

    pub fn drift(&self) -> [i32; 3] {
        *lock(&self.drift)
    }

    pub fn measure(&self) {
        *lock(&self.measured) = None;
        self.wanted.store(true, Ordering::Release);
    }

    pub fn take_measurement(&self) -> Option<Measurement> {
        lock(&self.measured).take()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) struct FollowedGyro {
    live: Arc<LiveGyro>,
    seen: u64,
    meter: Option<DriftMeter>,
}

impl FollowedGyro {
    pub(crate) fn new(live: Arc<LiveGyro>) -> Self {
        Self {
            seen: live.version.load(Ordering::Acquire),
            live,
            meter: None,
        }
    }

    pub(crate) fn changed_drift(&mut self) -> Option<[i32; 3]> {
        let version = self.live.version.load(Ordering::Acquire);
        (version != std::mem::replace(&mut self.seen, version)).then(|| self.live.drift())
    }

    pub(crate) fn drift(&self) -> [i32; 3] {
        self.live.drift()
    }

    pub(crate) fn read(&mut self, motion: Option<&Motion>, now: Instant) {
        if self.live.wanted.load(Ordering::Acquire)
            && self.live.wanted.swap(false, Ordering::AcqRel)
        {
            self.meter = Some(DriftMeter::new(now));
        }
        let Some(meter) = &mut self.meter else {
            return;
        };
        let result = motion
            .and_then(|motion| meter.add(motion, now))
            .or_else(|| meter.is_overdue(now).then_some(Err(DriftError::NoInput)));
        if let Some(result) = result {
            self.finish(result);
        }
    }

    fn finish(&mut self, result: Measurement) {
        self.meter = None;
        *lock(&self.live.measured) = Some(result);
    }
}

impl Drop for FollowedGyro {
    fn drop(&mut self) {
        if self.meter.is_some() {
            self.finish(Err(DriftError::NoInput));
        }
    }
}

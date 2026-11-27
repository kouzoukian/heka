use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::profile::Mapper;

#[derive(Debug, Default)]
pub struct LiveMapper {
    version: AtomicU64,
    current: Mutex<Arc<Mapper>>,
}

impl LiveMapper {
    pub fn set(&self, mapper: Mapper) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = Arc::new(mapper);
        self.version.fetch_add(1, Ordering::Release);
    }

    pub fn follow(&self) -> FollowedMapper<'_> {
        let seen = self.version.load(Ordering::Acquire);
        FollowedMapper {
            live: self,
            seen,
            mapper: self.snapshot(),
        }
    }

    fn snapshot(&self) -> Arc<Mapper> {
        Arc::clone(&self.current.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

#[derive(Debug)]
pub struct FollowedMapper<'a> {
    live: &'a LiveMapper,
    seen: u64,
    mapper: Arc<Mapper>,
}

impl FollowedMapper<'_> {
    pub fn refresh(&mut self) -> bool {
        let version = self.live.version.load(Ordering::Acquire);
        if version == self.seen {
            return false;
        }
        self.seen = version;
        let latest = self.live.snapshot();
        let changed = latest != self.mapper;
        self.mapper = latest;
        changed
    }

    pub fn current(&self) -> &Mapper {
        &self.mapper
    }
}

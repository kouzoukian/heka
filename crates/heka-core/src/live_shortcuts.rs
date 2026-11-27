use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use crate::shortcuts::{ShortcutAction, Shortcuts};

#[derive(Debug, Default)]
pub struct LiveShortcuts {
    version: AtomicU64,
    config: Mutex<Shortcuts>,
    events: Mutex<Vec<ShortcutAction>>,
}

impl LiveShortcuts {
    pub fn set(&self, shortcuts: Shortcuts) {
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = shortcuts;
        self.version.fetch_add(1, Ordering::Release);
    }

    pub fn push(&self, action: ShortcutAction) {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(action);
    }

    pub fn take(&self) -> Vec<ShortcutAction> {
        std::mem::take(&mut *self.events.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn follow(&self) -> FollowedShortcuts<'_> {
        let seen = self.version.load(Ordering::Acquire);
        FollowedShortcuts {
            live: self,
            seen,
            shortcuts: self.snapshot(),
        }
    }

    fn snapshot(&self) -> Shortcuts {
        *self.config.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug)]
pub struct FollowedShortcuts<'a> {
    live: &'a LiveShortcuts,
    seen: u64,
    shortcuts: Shortcuts,
}

impl FollowedShortcuts<'_> {
    pub fn get(&mut self) -> &Shortcuts {
        let version = self.live.version.load(Ordering::Acquire);
        if version != self.seen {
            self.seen = version;
            self.shortcuts = self.live.snapshot();
        }
        &self.shortcuts
    }
}

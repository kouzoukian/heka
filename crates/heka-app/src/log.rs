use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

pub fn line(message: &str) {
    eprintln!("heka: {message}");
}

#[derive(Debug, Default)]
pub struct Once(Mutex<BTreeSet<String>>);

impl Once {
    pub const fn new() -> Self {
        Self(Mutex::new(BTreeSet::new()))
    }

    pub fn first(&self, message: &str) -> bool {
        let mut seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        !seen.contains(message) && seen.insert(message.to_owned())
    }

    pub fn line(&self, message: &str) {
        if self.first(message) {
            line(message);
        }
    }
}

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use heka_app::log;
use heka_app::report::Reporter;
use heka_core::release::{ReleaseCheck, Version};
use heka_windows::web::WinHttp;

const WAKE: Duration = Duration::from_secs(60);

type Newer = Arc<Mutex<Option<Version>>>;

pub struct Releases {
    switch: Sender<bool>,
    newer: Newer,
}

impl Releases {
    pub fn start(enabled: bool) -> Self {
        let (switch, switched) = mpsc::channel();
        let newer = Newer::default();
        thread::Builder::new()
            .name("heka-releases".into())
            .spawn({
                let newer = Arc::clone(&newer);
                move || watch(enabled, &switched, &newer)
            })
            .expect("failed to start the release check thread");
        Self { switch, newer }
    }

    pub fn set_enabled(&self, enabled: bool) {
        let _ = self.switch.send(enabled);
    }

    pub fn newer(&self) -> Option<Version> {
        *self.newer.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn watch(mut enabled: bool, switched: &Receiver<bool>, newer: &Newer) {
    let mut check = ReleaseCheck::new(env!("CARGO_PKG_VERSION"));
    let mut reporter = Reporter::default();
    loop {
        if let Some(found) = check.poll(&WinHttp, enabled, Instant::now()) {
            if let Some(message) = reporter.release_check(&found) {
                log::line(&message);
            }
            *newer.lock().unwrap_or_else(PoisonError::into_inner) = check.newer();
        }
        match switched.recv_timeout(WAKE) {
            Ok(on) => enabled = on,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

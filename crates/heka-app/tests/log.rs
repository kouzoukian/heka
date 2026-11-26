use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use heka_app::log::{
    Entry, FILE_NAME, Log, LogFile, PREVIOUS_FILE_NAME, RECENT_LIMIT, REPEAT_WINDOW,
    REPEATS_WRITTEN, Sink, Throttle, copied,
};
use heka_app::text;
use heka_core::clock::Timestamp;

const WAIT: Duration = Duration::from_secs(5);

fn clock() -> Timestamp {
    Timestamp {
        year: 2026,
        month: 3,
        day: 5,
        hour: 21,
        minute: 3,
        second: 7,
        millisecond: 9,
    }
}

struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let id = std::process::id();
        let path = std::env::temp_dir().join(format!("heka-log-test-{id}-{name}"));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }

    fn read(&self, file: &str) -> String {
        fs::read_to_string(self.0.join(file)).unwrap()
    }

    fn log(&self, limit: u64) -> Log {
        let file = LogFile::start(&self.0, limit).unwrap();
        Log::start(Some(Box::new(file)), clock)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn each_entry_is_written_with_its_time_on_one_line() {
    let folder = Folder::new("lines");
    let log = folder.log(1024);

    log.write("heka started");
    log.write("a message\r\non two\nlines");
    assert!(log.flush(WAIT));

    assert_eq!(
        folder.read(FILE_NAME),
        "2026-03-05 21:03:07.009  heka started\n\
         2026-03-05 21:03:07.009  a message on two lines\n"
    );
}

#[test]
fn the_previous_run_is_kept_when_a_new_one_starts() {
    let folder = Folder::new("runs");
    for run in ["first", "second", "third"] {
        let log = folder.log(1024);
        log.write(run);
        assert!(log.flush(WAIT));
    }

    assert!(folder.read(FILE_NAME).ends_with("third\n"));
    assert!(folder.read(PREVIOUS_FILE_NAME).ends_with("second\n"));
    assert_eq!(fs::read_dir(&folder.0).unwrap().count(), 2);
}

#[test]
fn a_run_that_wrote_nothing_does_not_push_out_the_one_before_it() {
    let folder = Folder::new("silent");
    let log = folder.log(1024);
    log.write("worth keeping");
    assert!(log.flush(WAIT));
    LogFile::start(&folder.0, 1024).unwrap();
    LogFile::start(&folder.0, 1024).unwrap();

    assert!(folder.read(PREVIOUS_FILE_NAME).ends_with("worth keeping\n"));
}

#[test]
fn a_full_log_keeps_its_first_line_and_the_newest_ones_within_its_limit() {
    let folder = Folder::new("full");
    let earlier = folder.log(2000);
    earlier.write("the run before");
    assert!(earlier.flush(WAIT));

    let log = folder.log(2000);
    log.write("heka started");
    for number in 0..200 {
        log.write(&format!("entry {number}"));
    }
    assert!(log.flush(WAIT));

    let kept = folder.read(FILE_NAME);
    let lines: Vec<&str> = kept.lines().collect();
    assert!(kept.len() <= 2000, "{} bytes", kept.len());
    assert!(lines[0].ends_with("heka started"));
    assert_eq!(lines[1], text::log::TRIMMED);
    assert!(lines.last().unwrap().ends_with("entry 199"));
    assert_eq!(kept.matches(text::log::TRIMMED).count(), 1);
    assert!(
        folder
            .read(PREVIOUS_FILE_NAME)
            .ends_with("the run before\n")
    );
}

#[test]
fn the_newest_entries_are_listed_and_copied_oldest_first() {
    let log = Log::start(None, clock);
    for number in 0..RECENT_LIMIT + 2 {
        log.write(&format!("entry {number}"));
    }
    assert!(log.flush(WAIT));

    let recent = log.recent();

    assert_eq!(recent.len(), RECENT_LIMIT);
    assert_eq!(recent[0].message, "entry 2");
    assert!(copied(&recent).starts_with("2026-03-05 21:03:07.009  entry 2\r\n"));
    assert!(copied(&recent).ends_with(&format!("entry {}\r\n", RECENT_LIMIT + 1)));
}

#[test]
fn a_message_repeating_without_end_is_written_a_few_times_a_minute() {
    let t0 = Instant::now();
    let mut throttle = Throttle::default();
    let second = Duration::from_secs(1);

    let written: Vec<Option<u32>> = (0..10)
        .map(|n| throttle.pass("again", t0 + second * n))
        .collect();

    let few = REPEATS_WRITTEN as usize;
    assert_eq!(written[..few], vec![Some(0); few]);
    assert_eq!(written[few..], vec![None; 10 - few]);
    assert_eq!(throttle.pass("another", t0 + second), Some(0));
    assert_eq!(
        throttle.pass("again", t0 + REPEAT_WINDOW),
        Some(10 - REPEATS_WRITTEN)
    );
    assert_eq!(throttle.pass("again", t0 + REPEAT_WINDOW * 3), Some(0));
}

#[test]
fn the_log_holds_back_repeats_but_never_an_entry_written_at_once() {
    let log = Log::start(None, clock);
    for _ in 0..10 {
        log.write("again");
    }
    log.write_now("again", WAIT);

    assert_eq!(log.recent().len(), REPEATS_WRITTEN as usize + 1);
}

struct Stuck(Receiver<()>);

impl Sink for Stuck {
    fn append(&mut self, _: &Entry) -> io::Result<()> {
        let _ = self.0.recv();
        Ok(())
    }
}

#[test]
fn writing_never_waits_for_the_disk() {
    let (release, held) = mpsc::channel();
    let log = Log::start(Some(Box::new(Stuck(held))), clock);

    let started = Instant::now();
    for number in 0..100 {
        log.write(&format!("entry {number}"));
    }

    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!log.flush(Duration::from_millis(50)));
    drop(release);
    assert!(log.flush(WAIT));
}

struct Failing;

impl Sink for Failing {
    fn append(&mut self, _: &Entry) -> io::Result<()> {
        Err(io::Error::other("the disk is full"))
    }
}

#[test]
fn a_log_file_that_cannot_be_written_still_leaves_the_entries_to_read() {
    let log = Log::start(Some(Box::new(Failing)), clock);

    log.write("kept");
    assert!(log.flush(WAIT));

    assert_eq!(log.recent()[0].message, "kept");
}

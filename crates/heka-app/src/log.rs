use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use heka_core::clock::Timestamp;

use crate::text;

pub const FILE_NAME: &str = "heka.log";
pub const PREVIOUS_FILE_NAME: &str = "heka.previous.log";
pub const FILE_LIMIT: u64 = 1024 * 1024;
pub const RECENT_LIMIT: usize = 500;
pub const REPEAT_WINDOW: Duration = Duration::from_secs(60);
pub const REPEATS_WRITTEN: u32 = 3;

const REMEMBERED_MESSAGES: usize = 256;
const THREAD_NAME: &str = "heka-log";
const PANIC_FLUSH: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub at: Timestamp,
    pub message: String,
}

impl Entry {
    pub fn line(&self) -> String {
        format!("{}  {}", self.at, self.message)
    }
}

pub type Recent = Arc<VecDeque<Arc<Entry>>>;

pub fn copied(recent: &Recent) -> String {
    recent.iter().map(|entry| entry.line() + "\r\n").collect()
}

pub fn folder() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("heka"))
}

pub trait Sink: Send {
    fn append(&mut self, entry: &Entry) -> io::Result<()>;
}

pub struct LogFile {
    path: PathBuf,
    file: File,
    len: u64,
    limit: u64,
}

impl LogFile {
    pub fn start(folder: &Path, limit: u64) -> io::Result<Self> {
        fs::create_dir_all(folder)?;
        let path = folder.join(FILE_NAME);
        if fs::metadata(&path).is_ok_and(|found| found.len() > 0) {
            let _ = fs::rename(&path, folder.join(PREVIOUS_FILE_NAME));
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let len = file.metadata()?.len();
        Ok(Self {
            path,
            file,
            len,
            limit,
        })
    }

    fn trim(&mut self) -> io::Result<()> {
        let bytes = fs::read(&self.path)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut lines = text.lines();
        let first = lines.next().unwrap_or_default();
        let mut room = self.limit / 2;
        let mut newest: Vec<&str> = lines
            .rev()
            .take_while(|line| {
                let needed = line.len() as u64 + 1;
                let fits = needed <= room;
                room = room.saturating_sub(needed);
                fits
            })
            .collect();
        newest.reverse();
        let mut kept = format!("{first}\n{}\n", text::log::TRIMMED);
        for line in newest {
            kept.push_str(line);
            kept.push('\n');
        }
        self.file = File::create(&self.path)?;
        self.file.write_all(kept.as_bytes())?;
        self.len = kept.len() as u64;
        Ok(())
    }
}

impl Sink for LogFile {
    fn append(&mut self, entry: &Entry) -> io::Result<()> {
        let line = entry.line() + "\n";
        self.file.write_all(line.as_bytes())?;
        self.len += line.len() as u64;
        if self.len > self.limit {
            self.trim()?;
        }
        Ok(())
    }
}

struct Seen {
    since: Instant,
    times: u32,
}

#[derive(Default)]
pub struct Throttle(HashMap<String, Seen>);

impl Throttle {
    pub fn pass(&mut self, message: &str, now: Instant) -> Option<u32> {
        let recent = |seen: &Seen| now.saturating_duration_since(seen.since) < REPEAT_WINDOW;
        if let Some(seen) = self.0.get_mut(message) {
            if recent(seen) {
                seen.times += 1;
                return (seen.times <= REPEATS_WRITTEN).then_some(0);
            }
            let left_out = seen.times.saturating_sub(REPEATS_WRITTEN);
            *seen = Seen {
                since: now,
                times: 1,
            };
            return Some(left_out);
        }
        if self.0.len() >= REMEMBERED_MESSAGES {
            self.0.retain(|_, seen| recent(seen));
        }
        self.0.insert(
            message.to_owned(),
            Seen {
                since: now,
                times: 1,
            },
        );
        Some(0)
    }
}

enum Message {
    Entry { entry: Entry, throttled: bool },
    Flush(Sender<()>),
}

pub struct Log {
    sender: Sender<Message>,
    recent: Arc<Mutex<Recent>>,
    clock: fn() -> Timestamp,
}

impl Log {
    pub fn start(sink: Option<Box<dyn Sink>>, clock: fn() -> Timestamp) -> Self {
        let (sender, inbox) = mpsc::channel();
        let recent = Arc::new(Mutex::new(Recent::default()));
        let spawned = thread::Builder::new().name(THREAD_NAME.into()).spawn({
            let recent = Arc::clone(&recent);
            move || write_entries(&inbox, &recent, sink)
        });
        if let Err(error) = spawned {
            echo(&error.to_string());
        }
        Self {
            sender,
            recent,
            clock,
        }
    }

    pub fn write(&self, message: &str) {
        self.send(message, true);
    }

    pub fn write_now(&self, message: &str, wait: Duration) {
        self.send(message, false);
        self.flush(wait);
    }

    pub fn flush(&self, wait: Duration) -> bool {
        if thread::current().name() == Some(THREAD_NAME) {
            return false;
        }
        let (done, written) = mpsc::channel();
        self.sender.send(Message::Flush(done)).is_ok() && written.recv_timeout(wait).is_ok()
    }

    pub fn recent(&self) -> Recent {
        Arc::clone(&self.recent.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn send(&self, message: &str, throttled: bool) {
        let entry = Entry {
            at: (self.clock)(),
            message: message.replace("\r\n", " ").replace(['\r', '\n'], " "),
        };
        let _ = self.sender.send(Message::Entry { entry, throttled });
    }
}

fn write_entries(
    inbox: &Receiver<Message>,
    recent: &Mutex<Recent>,
    mut sink: Option<Box<dyn Sink>>,
) {
    let mut throttle = Throttle::default();
    for message in inbox {
        match message {
            Message::Entry {
                mut entry,
                throttled,
            } => {
                if throttled {
                    match throttle.pass(&entry.message, Instant::now()) {
                        None => continue,
                        Some(0) => {}
                        Some(left_out) => {
                            entry.message = text::log::repeated(&entry.message, left_out);
                        }
                    }
                }
                echo(&entry.message);
                if let Some(Err(error)) = sink.as_mut().map(|sink| sink.append(&entry)) {
                    echo(&text::log::cannot_write(&error.to_string()));
                }
                let mut recent = recent.lock().unwrap_or_else(PoisonError::into_inner);
                let kept = Arc::make_mut(&mut recent);
                if kept.len() >= RECENT_LIMIT {
                    kept.pop_front();
                }
                kept.push_back(Arc::new(entry));
            }
            Message::Flush(done) => {
                let _ = done.send(());
            }
        }
    }
}

fn echo(message: &str) {
    let _ = writeln!(io::stderr(), "heka: {message}");
}

static LOG: OnceLock<Log> = OnceLock::new();

pub fn install(log: Log) {
    let _ = LOG.set(log);
}

pub fn line(message: &str) {
    match LOG.get() {
        Some(log) => log.write(message),
        None => echo(message),
    }
}

pub fn recent() -> Recent {
    LOG.get().map(Log::recent).unwrap_or_default()
}

pub fn flush(wait: Duration) {
    if let Some(log) = LOG.get() {
        log.flush(wait);
    }
}

pub fn catch_panics() {
    let earlier = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        if let Some(log) = LOG.get() {
            let thread = thread::current();
            let message = text::log::panicked(thread.name(), &panic.to_string());
            log.write_now(&message, PANIC_FLUSH);
        }
        earlier(panic);
    }));
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

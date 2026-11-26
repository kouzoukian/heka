use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

#[derive(Debug, Clone)]
pub struct DeviceInstancePath(String);

impl DeviceInstancePath {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn key(&self) -> String {
        self.0.to_uppercase()
    }
}

impl PartialEq for DeviceInstancePath {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for DeviceInstancePath {}

impl fmt::Display for DeviceInstancePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HidHideConfig {
    pub active: bool,
    pub inverse: bool,
    pub hidden_devices: Vec<String>,
    pub allowed_apps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HidHideError {
    NotInstalled,
    InUse,
    Failed(String),
}

impl fmt::Display for HidHideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => {
                f.write_str("HidHide is not installed or its driver is not running")
            }
            Self::InUse => {
                f.write_str("HidHide is busy: close the HidHide Configuration Client or HidHideCLI")
            }
            Self::Failed(message) => write!(f, "HidHide failed: {message}"),
        }
    }
}

impl std::error::Error for HidHideError {}

pub trait HidHide: Send {
    fn read(&self) -> Result<HidHideConfig, HidHideError>;
    fn set_hidden_devices(&self, devices: &[String]) -> Result<(), HidHideError>;
    fn set_allowed_apps(&self, apps: &[String]) -> Result<(), HidHideError>;
    fn set_active(&self, active: bool) -> Result<(), HidHideError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HidHideStatus {
    Ready,
    NotInstalled,
    InUse,
    Failed(String),
}

impl HidHideStatus {
    pub fn detect(hidhide: &dyn HidHide) -> Self {
        match hidhide.read() {
            Ok(_) => Self::Ready,
            Err(HidHideError::NotInstalled) => Self::NotInstalled,
            Err(HidHideError::InUse) => Self::InUse,
            Err(HidHideError::Failed(message)) => Self::Failed(message),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Journal {
    pub cloak_was_off: bool,
    pub allowed_app: Option<String>,
    pub hidden_devices: Vec<String>,
}

impl Journal {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub trait JournalStore: Send {
    fn load(&self) -> std::io::Result<Journal>;
    fn save(&self, journal: &Journal) -> std::io::Result<()>;
}

pub struct FileJournal {
    path: PathBuf,
}

const JOURNAL_HEADER: &str = "heka hiding journal v1";
const CLOAK_WAS_OFF: &str = "cloak-was-off";
const ALLOWED_APP: &str = "allowed-app ";
const HIDDEN_DEVICE: &str = "hidden-device ";

impl FileJournal {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl JournalStore for FileJournal {
    fn load(&self) -> std::io::Result<Journal> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Journal::default()),
            Err(error) => return Err(error),
        };
        let invalid = |line: &str| {
            std::io::Error::new(
                ErrorKind::InvalidData,
                format!("{}: unexpected line {line:?}", self.path.display()),
            )
        };
        let mut lines = text.lines();
        match lines.next() {
            Some(JOURNAL_HEADER) => {}
            other => return Err(invalid(other.unwrap_or_default())),
        }
        let mut journal = Journal::default();
        for line in lines {
            if line == CLOAK_WAS_OFF {
                journal.cloak_was_off = true;
            } else if let Some(app) = line.strip_prefix(ALLOWED_APP) {
                journal.allowed_app = Some(app.to_owned());
            } else if let Some(device) = line.strip_prefix(HIDDEN_DEVICE) {
                journal.hidden_devices.push(device.to_owned());
            } else if !line.is_empty() {
                return Err(invalid(line));
            }
        }
        Ok(journal)
    }

    fn save(&self, journal: &Journal) -> std::io::Result<()> {
        if journal.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        }
        let mut text = format!("{JOURNAL_HEADER}\n");
        if journal.cloak_was_off {
            text.push_str(&format!("{CLOAK_WAS_OFF}\n"));
        }
        if let Some(app) = &journal.allowed_app {
            text.push_str(&format!("{ALLOWED_APP}{app}\n"));
        }
        for device in &journal.hidden_devices {
            text.push_str(&format!("{HIDDEN_DEVICE}{device}\n"));
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let partial = self.path.with_extension("partial");
        let mut file = fs::File::create(&partial)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&partial, &self.path)
    }
}

#[derive(Debug)]
pub enum HidingError {
    HidHide(HidHideError),
    Journal(std::io::Error),
}

impl fmt::Display for HidingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HidHide(error) => error.fmt(f),
            Self::Journal(error) => write!(f, "cannot save heka's HidHide journal: {error}"),
        }
    }
}

impl std::error::Error for HidingError {}

impl From<HidHideError> for HidingError {
    fn from(error: HidHideError) -> Self {
        Self::HidHide(error)
    }
}

impl From<std::io::Error> for HidingError {
    fn from(error: std::io::Error) -> Self {
        Self::Journal(error)
    }
}

pub struct Hiding {
    inner: Mutex<Inner>,
}

struct Inner {
    hidhide: Box<dyn HidHide>,
    store: Box<dyn JournalStore>,
    journal: Journal,
    app: String,
    wanted: BTreeMap<String, BTreeSet<String>>,
    stay_allowed: bool,
    journal_unread: bool,
}

impl Hiding {
    pub fn new(hidhide: Box<dyn HidHide>, store: Box<dyn JournalStore>, app: String) -> Self {
        Self {
            inner: Mutex::new(Inner {
                hidhide,
                store,
                journal: Journal::default(),
                app,
                wanted: BTreeMap::new(),
                stay_allowed: false,
                journal_unread: false,
            }),
        }
    }

    pub fn allow_heka(&self) -> Result<(), HidingError> {
        let mut inner = self.lock();
        inner.stay_allowed = true;
        inner.read_journal()?;
        let config = inner.hidhide.read()?;
        inner.allow(&config)
    }

    pub fn hide(&self, owner: &str, device: &DeviceInstancePath) -> Result<(), HidingError> {
        let mut inner = self.lock();
        inner
            .wanted
            .entry(owner.to_owned())
            .or_default()
            .insert(device.key());
        inner.hide(device)
    }

    pub fn release(&self, owner: &str) -> Result<(), HidingError> {
        let mut inner = self.lock();
        inner.wanted.remove(owner);
        inner.undo()
    }

    pub fn restore_all(&self) -> Result<(), HidingError> {
        let mut inner = self.lock();
        inner.wanted.clear();
        inner.stay_allowed = false;
        inner.undo()
    }

    pub fn recover(&self) -> Result<(), HidingError> {
        let mut inner = self.lock();
        inner.journal_unread = true;
        inner.undo()
    }

    pub fn restore(&self) -> Result<(), HidingError> {
        self.lock().undo()
    }

    pub fn inverted(&self) -> Result<bool, HidingError> {
        Ok(self.lock().hidhide.read()?.inverse)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Inner {
    fn read_journal(&mut self) -> Result<(), HidingError> {
        if self.journal_unread {
            let saved = self.store.load()?;
            merge(&mut self.journal, saved);
            self.journal_unread = false;
        }
        Ok(())
    }

    fn hide(&mut self, device: &DeviceInstancePath) -> Result<(), HidingError> {
        self.read_journal()?;
        let config = self.hidhide.read()?;

        self.allow(&config)?;

        if !contains(&config.hidden_devices, device.as_str()) {
            if !contains(&self.journal.hidden_devices, device.as_str()) {
                self.journal.hidden_devices.push(device.as_str().to_owned());
                self.save()?;
            }
            let mut devices = config.hidden_devices.clone();
            devices.push(device.as_str().to_owned());
            self.hidhide.set_hidden_devices(&devices)?;
        }

        if !config.active {
            self.journal.cloak_was_off = true;
            self.save()?;
            self.hidhide.set_active(true)?;
        }
        Ok(())
    }

    fn allow(&mut self, config: &HidHideConfig) -> Result<(), HidingError> {
        if config.inverse || contains(&config.allowed_apps, &self.app) {
            return Ok(());
        }
        self.journal.allowed_app = Some(self.app.clone());
        self.save()?;
        let mut apps = config.allowed_apps.clone();
        apps.push(self.app.clone());
        self.hidhide.set_allowed_apps(&apps)?;
        Ok(())
    }

    fn undo(&mut self) -> Result<(), HidingError> {
        self.read_journal()?;
        let still_wanted: BTreeSet<&String> = self.wanted.values().flatten().collect();
        let nothing_wanted = still_wanted.is_empty();
        let release: Vec<String> = self
            .journal
            .hidden_devices
            .iter()
            .filter(|device| !still_wanted.contains(&device.to_uppercase()))
            .cloned()
            .collect();
        let undo_cloak = nothing_wanted && self.journal.cloak_was_off;
        let undo_app = if nothing_wanted && !self.stay_allowed {
            self.journal.allowed_app.clone()
        } else {
            None
        };
        if release.is_empty() && !undo_cloak && undo_app.is_none() {
            return Ok(());
        }

        let config = self.hidhide.read()?;

        if undo_cloak {
            if config.active {
                self.hidhide.set_active(false)?;
            }
            self.journal.cloak_was_off = false;
            self.save()?;
        }

        if !release.is_empty() {
            let devices: Vec<String> = config
                .hidden_devices
                .iter()
                .filter(|device| !contains(&release, device))
                .cloned()
                .collect();
            if devices.len() != config.hidden_devices.len() {
                self.hidhide.set_hidden_devices(&devices)?;
            }
            self.journal
                .hidden_devices
                .retain(|device| !contains(&release, device));
            self.save()?;
        }

        if let Some(app) = undo_app {
            let apps: Vec<String> = config
                .allowed_apps
                .iter()
                .filter(|entry| !entry.eq_ignore_ascii_case(&app))
                .cloned()
                .collect();
            if apps.len() != config.allowed_apps.len() {
                self.hidhide.set_allowed_apps(&apps)?;
            }
            self.journal.allowed_app = None;
            self.save()?;
        }
        Ok(())
    }

    fn save(&self) -> Result<(), HidingError> {
        Ok(self.store.save(&self.journal)?)
    }
}

fn merge(journal: &mut Journal, saved: Journal) {
    journal.cloak_was_off |= saved.cloak_was_off;
    if journal.allowed_app.is_none() {
        journal.allowed_app = saved.allowed_app;
    }
    for device in saved.hidden_devices {
        if !contains(&journal.hidden_devices, &device) {
            journal.hidden_devices.push(device);
        }
    }
}

fn contains(list: &[String], item: &str) -> bool {
    list.iter().any(|entry| entry.eq_ignore_ascii_case(item))
}

//! Hiding Controllers from games with HidHide, and putting everything back.
//!
//! HidHide keeps three things heka touches: a cloak switch, a list of hidden devices
//! (by device instance path) and a list of allowed applications (by full image name).
//! Other tools share that configuration, so heka only ever undoes what it did itself.
//! Every change is written to a [`Journal`] before it is made, so that the next start
//! can undo it after a crash.
//!
//! [`Hiding`] is the seam a Bridge supervisor calls: [`Hiding::hide`] when a Link of a
//! Bridged or Held Controller is seen, [`Hiding::release`] when that Controller's Bridge
//! ends, [`Hiding::restore_all`] on stop or quit, and [`Hiding::recover`] then
//! [`Hiding::allow_heka`] at start, before looking for Controllers: a Controller another
//! tool hid cannot even be found by heka until heka is allowed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

/// A PnP device instance path, such as `HID\VID_054C&PID_0CE6&MI_03\8&15C30336&0&0000`.
///
/// HidHide compares these without regard to case, and so does heka.
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

/// HidHide's whole configuration as read from the driver.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HidHideConfig {
    /// Cloaking is on: hidden devices are hidden from applications that are not allowed.
    pub active: bool,
    /// The application list is inverted: listed applications are the ones denied.
    pub inverse: bool,
    /// Device instance paths of hidden devices.
    pub hidden_devices: Vec<String>,
    /// Full image names (`\Device\HarddiskVolumeN\...\app.exe`) of allowed applications.
    pub allowed_apps: Vec<String>,
}

/// Why HidHide could not be read or changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HidHideError {
    /// HidHide's driver is not installed or not running.
    NotInstalled,
    /// Another program holds HidHide's control device, which takes one client at a time.
    /// Usually HidHide's own configuration app or command line is open.
    InUse,
    /// Any other driver failure, with the system's message.
    Failed(String),
}

impl fmt::Display for HidHideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("HidHide is not installed or its driver is not running"),
            Self::InUse => f.write_str(
                "HidHide is busy: close the HidHide Configuration Client or HidHideCLI so heka can hide the Controller",
            ),
            Self::Failed(message) => write!(f, "HidHide failed: {message}"),
        }
    }
}

impl std::error::Error for HidHideError {}

/// Access to HidHide's configuration. Implemented over the driver in `heka-windows`.
///
/// Each list setter replaces the whole list, as HidHide's own interface does.
pub trait HidHide: Send {
    fn read(&self) -> Result<HidHideConfig, HidHideError>;
    fn set_hidden_devices(&self, devices: &[String]) -> Result<(), HidHideError>;
    fn set_allowed_apps(&self, apps: &[String]) -> Result<(), HidHideError>;
    fn set_active(&self, active: bool) -> Result<(), HidHideError>;
}

/// Whether HidHide can be used, for the setup screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HidHideStatus {
    Ready,
    NotInstalled,
    InUse,
    Failed(String),
}

impl HidHideStatus {
    /// Detects HidHide by reading its configuration. Changes nothing.
    pub fn detect(hidhide: &dyn HidHide) -> Self {
        match hidhide.read() {
            Ok(_) => Self::Ready,
            Err(HidHideError::NotInstalled) => Self::NotInstalled,
            Err(HidHideError::InUse) => Self::InUse,
            Err(HidHideError::Failed(message)) => Self::Failed(message),
        }
    }
}

/// What heka has changed in HidHide and must undo. Empty means nothing to undo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Journal {
    /// heka turned cloaking on; it was off before.
    pub cloak_was_off: bool,
    /// heka added this full image name to the allowed applications.
    pub allowed_app: Option<String>,
    /// Device instance paths heka added to the hidden devices.
    pub hidden_devices: Vec<String>,
}

impl Journal {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Where the journal survives a crash.
pub trait JournalStore: Send {
    /// The saved journal, or an empty one when nothing is saved.
    fn load(&self) -> std::io::Result<Journal>;
    /// Saves the journal durably before returning.
    fn save(&self, journal: &Journal) -> std::io::Result<()>;
}

/// A journal kept in one small text file. An empty journal removes the file.
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
        // Written aside and renamed over, so a kill mid-write never leaves half a journal.
        let partial = self.path.with_extension("partial");
        let mut file = fs::File::create(&partial)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&partial, &self.path)
    }
}

/// Why hiding or restoring failed.
#[derive(Debug)]
pub enum HidingError {
    HidHide(HidHideError),
    /// The journal could not be saved or read, so no change was made.
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

/// Hides Controllers for any number of Bridges. Safe to share between threads.
pub struct Hiding {
    inner: Mutex<Inner>,
}

struct Inner {
    hidhide: Box<dyn HidHide>,
    store: Box<dyn JournalStore>,
    journal: Journal,
    /// heka's own full image name, allowed so that heka still sees hidden Controllers.
    app: String,
    /// The Links each owner wants Hidden, by owner key.
    wanted: BTreeMap<String, BTreeSet<String>>,
    /// heka wants to stay allowed until [`Hiding::restore_all`], whether or not
    /// anything is Hidden, so it can see Controllers that other tools hid.
    stay_allowed: bool,
}

impl Hiding {
    /// `app` is heka's own full image name, as HidHide lists allowed applications.
    pub fn new(hidhide: Box<dyn HidHide>, store: Box<dyn JournalStore>, app: String) -> Self {
        Self {
            inner: Mutex::new(Inner {
                hidhide,
                store,
                journal: Journal::default(),
                app,
                wanted: BTreeMap::new(),
                stay_allowed: false,
            }),
        }
    }

    /// Adds heka to HidHide's allowed applications, so that it can find and open a
    /// Controller another tool left hidden. Call at start, after [`Hiding::recover`]
    /// and before looking for Controllers; on failure, call again later. heka stays
    /// allowed until [`Hiding::restore_all`], which removes it only if heka added it.
    pub fn allow_heka(&self) -> Result<(), HidingError> {
        let mut inner = self.lock();
        inner.stay_allowed = true;
        let config = inner.hidhide.read()?;
        inner.allow(&config)
    }

    /// Hides one Link of a Controller. `owner` is any stable key for the Controller;
    /// a Controller with several Links calls this once per Link. Calling it again for
    /// a Link that is already Hidden changes nothing, so a returning Link can always
    /// be passed again.
    pub fn hide(&self, owner: &str, device: &DeviceInstancePath) -> Result<(), HidingError> {
        let mut inner = self.lock();
        inner
            .wanted
            .entry(owner.to_owned())
            .or_default()
            .insert(device.key());
        inner.hide(device)
    }

    /// Ends hiding for every Link of this owner. A Link another owner still wants stays
    /// Hidden. When no owner wants anything Hidden, everything heka changed is undone.
    pub fn release(&self, owner: &str) -> Result<(), HidingError> {
        let mut inner = self.lock();
        inner.wanted.remove(owner);
        inner.undo()
    }

    /// Ends hiding for every owner and undoes everything heka changed. Call on stop and quit.
    pub fn restore_all(&self) -> Result<(), HidingError> {
        let mut inner = self.lock();
        inner.wanted.clear();
        inner.stay_allowed = false;
        inner.undo()
    }

    /// Undoes what an earlier run journalled but never restored, for example because it
    /// was killed. Call once at start, before the first [`Hiding::hide`].
    pub fn recover(&self) -> Result<(), HidingError> {
        let mut inner = self.lock();
        let saved = inner.store.load()?;
        merge(&mut inner.journal, saved);
        inner.undo()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Inner {
    fn hide(&mut self, device: &DeviceInstancePath) -> Result<(), HidingError> {
        let config = self.hidhide.read()?;

        // heka is allowed first, so it never loses sight of a Controller it Hides.
        self.allow(&config)?;

        // A device that is already hidden belongs to the player or another tool: it is
        // not journalled, so heka never removes it.
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

    /// Adds heka to the allowed applications unless it is already there. With an
    /// inverted list, listing heka would deny it instead, so heka stays off the list.
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

    /// Undoes every journalled change no owner still needs, in the reverse order of
    /// [`Inner::hide`]. Each change is made before it leaves the journal, so a crash in
    /// between leaves an entry whose undo finds nothing left to do.
    fn undo(&mut self) -> Result<(), HidingError> {
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

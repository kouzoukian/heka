//! Start with Windows.
//!
//! The setting is the Run entry itself: heka starts with Windows exactly when its
//! entry is there, so nothing else has to be kept in step with it. The entry runs
//! heka with [`TRAY_ARGUMENT`], so a copy started with Windows opens in the tray.
//! Where the entry lives is behind [`RunEntry`].

use std::io;
use std::path::Path;

/// The argument that makes heka open in the tray instead of showing its window.
pub const TRAY_ARGUMENT: &str = "--tray";

/// heka's entry in the list of programs Windows starts at sign-in.
pub trait RunEntry {
    /// The command line in the entry, `None` when there is no entry.
    fn get(&self) -> io::Result<Option<String>>;
    fn set(&mut self, command: &str) -> io::Result<()>;
    /// Removing a missing entry succeeds.
    fn remove(&mut self) -> io::Result<()>;
}

pub struct Autostart<E> {
    entry: E,
    command: String,
}

impl<E: RunEntry> Autostart<E> {
    /// `exe` is the running copy of heka.
    pub fn new(entry: E, exe: &Path) -> Self {
        Self {
            entry,
            command: format!("\"{}\" {TRAY_ARGUMENT}", exe.display()),
        }
    }

    /// An entry that cannot be read counts as off.
    pub fn is_enabled(&self) -> bool {
        matches!(self.entry.get(), Ok(Some(_)))
    }

    pub fn set_enabled(&mut self, enabled: bool) -> io::Result<()> {
        match (enabled, self.entry.get()?) {
            (true, Some(command)) if command == self.command => Ok(()),
            (true, _) => self.entry.set(&self.command),
            (false, None) => Ok(()),
            (false, Some(_)) => self.entry.remove(),
        }
    }

    /// When the setting is on, points the entry at this copy, which may have moved
    /// since the entry was written. Never turns the setting on.
    pub fn repair(&mut self) -> io::Result<()> {
        match self.entry.get()? {
            Some(command) if command != self.command => self.entry.set(&self.command),
            _ => Ok(()),
        }
    }
}

/// Whether this run should open in the tray. `args` starts with the program name,
/// as [`std::env::args_os`] does.
pub fn opens_in_tray<I>(args: I) -> bool
where
    I: IntoIterator,
    I::Item: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .skip(1)
        .any(|arg| arg.as_ref() == TRAY_ARGUMENT)
}

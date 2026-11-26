use std::io;
use std::path::Path;

pub const TRAY_ARGUMENT: &str = "--tray";

pub trait RunEntry {
    fn get(&self) -> io::Result<Option<String>>;
    fn set(&mut self, command: &str) -> io::Result<()>;
    fn remove(&mut self) -> io::Result<()>;
}

pub struct Autostart<E> {
    entry: E,
    command: String,
}

impl<E: RunEntry> Autostart<E> {
    pub fn new(entry: E, exe: &Path) -> Self {
        Self {
            entry,
            command: format!("\"{}\" {TRAY_ARGUMENT}", exe.display()),
        }
    }

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

    pub fn repair(&mut self) -> io::Result<()> {
        match self.entry.get()? {
            Some(command) if command != self.command => self.entry.set(&self.command),
            _ => Ok(()),
        }
    }
}

pub fn opens_in_tray<I>(args: I) -> bool
where
    I: IntoIterator,
    I::Item: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .skip(1)
        .any(|arg| arg.as_ref() == TRAY_ARGUMENT)
}

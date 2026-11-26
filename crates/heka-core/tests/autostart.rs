use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::rc::Rc;

use heka_core::autostart::{Autostart, RunEntry};

#[derive(Clone, Default)]
struct FakeRunEntry {
    value: Rc<RefCell<Option<String>>>,
    writes: Rc<RefCell<usize>>,
}

impl FakeRunEntry {
    fn holding(command: &str) -> Self {
        let fake = Self::default();
        *fake.value.borrow_mut() = Some(command.into());
        fake
    }

    fn value(&self) -> Option<String> {
        self.value.borrow().clone()
    }

    fn writes(&self) -> usize {
        *self.writes.borrow()
    }
}

impl RunEntry for FakeRunEntry {
    fn get(&self) -> io::Result<Option<String>> {
        Ok(self.value())
    }

    fn set(&mut self, command: &str) -> io::Result<()> {
        *self.writes.borrow_mut() += 1;
        *self.value.borrow_mut() = Some(command.into());
        Ok(())
    }

    fn remove(&mut self) -> io::Result<()> {
        *self.writes.borrow_mut() += 1;
        *self.value.borrow_mut() = None;
        Ok(())
    }
}

const EXE: &str = r"C:\Games\heka 1.0\heka.exe";

fn autostart(entry: &FakeRunEntry) -> Autostart<FakeRunEntry> {
    Autostart::new(entry.clone(), Path::new(EXE))
}

#[test]
fn start_with_windows_is_off_by_default() {
    let entry = FakeRunEntry::default();
    let mut autostart = autostart(&entry);

    assert!(!autostart.is_enabled());
    autostart.repair().unwrap();
    assert_eq!(entry.value(), None);
    assert_eq!(entry.writes(), 0);
}

#[test]
fn turning_it_on_starts_this_copy_in_the_tray() {
    let entry = FakeRunEntry::default();
    let mut autostart = autostart(&entry);

    autostart.set_enabled(true).unwrap();

    assert!(autostart.is_enabled());
    assert_eq!(
        entry.value().as_deref(),
        Some(r#""C:\Games\heka 1.0\heka.exe" --tray"#)
    );
}

#[test]
fn turning_it_off_removes_the_entry() {
    let entry = FakeRunEntry::holding(r#""C:\Games\heka 1.0\heka.exe" --tray"#);
    let mut autostart = autostart(&entry);
    assert!(autostart.is_enabled());

    autostart.set_enabled(false).unwrap();

    assert!(!autostart.is_enabled());
    assert_eq!(entry.value(), None);
}

#[test]
fn a_moved_copy_points_the_entry_at_itself() {
    let entry = FakeRunEntry::holding(r#""D:\old\heka.exe" --tray"#);
    let mut autostart = autostart(&entry);

    autostart.repair().unwrap();

    assert_eq!(
        entry.value().as_deref(),
        Some(r#""C:\Games\heka 1.0\heka.exe" --tray"#)
    );
}

#[test]
fn an_entry_already_right_is_not_written_again() {
    let entry = FakeRunEntry::holding(r#""C:\Games\heka 1.0\heka.exe" --tray"#);
    let mut autostart = autostart(&entry);

    autostart.repair().unwrap();
    autostart.set_enabled(true).unwrap();

    assert_eq!(entry.writes(), 0);
}

#[test]
fn only_the_tray_argument_opens_in_the_tray() {
    use heka_core::autostart::opens_in_tray;

    assert!(opens_in_tray(["heka.exe", "--tray"]));
    assert!(!opens_in_tray(["heka.exe"]));
    assert!(
        !opens_in_tray(["--tray"]),
        "the program name is not an argument"
    );
    assert!(!opens_in_tray(["heka.exe", "--trayx"]));
}

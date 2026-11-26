//! Hiding Controllers through a fake HidHide and an in-memory journal.

use std::sync::{Arc, Mutex};

use heka_core::hiding::{
    DeviceInstancePath, FileJournal, HidHide, HidHideConfig, HidHideError, HidHideStatus, Hiding,
    Journal, JournalStore,
};

const HEKA: &str = r"\Device\HarddiskVolume3\Tools\heka\heka.exe";
const DS4WINDOWS: &str = r"\Device\HarddiskVolume3\Tools\DS4Windows\DS4Windows.exe";
const X360CE: &str = r"\Device\HarddiskVolume3\Tools\x360ce\x360ce.exe";
const USB_LINK: &str = r"HID\VID_054C&PID_0CE6&MI_03\8&15C30336&0&0000";
const BT_LINK: &str =
    r"HID\{00001124-0000-1000-8000-00805F9B34FB}_VID&0002054C_PID&0CE6\9&2D7E4C5&0&0000";
const OTHER_PAD: &str = r"HID\VID_046D&PID_C216\7&1A2B3C4D&0&0000";

fn path(text: &str) -> DeviceInstancePath {
    DeviceInstancePath::new(text)
}

/// One write the fake driver received, with the journal as it was saved at that moment.
#[derive(Debug, Clone)]
struct Write {
    what: String,
    journal_then: Journal,
}

#[derive(Default)]
struct World {
    config: HidHideConfig,
    journal: Journal,
    writes: Vec<Write>,
    fail_with: Option<HidHideError>,
    journal_saves: usize,
}

type Shared = Arc<Mutex<World>>;

struct FakeHidHide(Shared);

impl FakeHidHide {
    fn write(
        &self,
        what: String,
        apply: impl FnOnce(&mut HidHideConfig),
    ) -> Result<(), HidHideError> {
        let mut world = self.0.lock().unwrap();
        if let Some(error) = world.fail_with.clone() {
            return Err(error);
        }
        let journal_then = world.journal.clone();
        world.writes.push(Write { what, journal_then });
        apply(&mut world.config);
        Ok(())
    }
}

impl HidHide for FakeHidHide {
    fn read(&self) -> Result<HidHideConfig, HidHideError> {
        let world = self.0.lock().unwrap();
        match world.fail_with.clone() {
            Some(error) => Err(error),
            None => Ok(world.config.clone()),
        }
    }

    fn set_hidden_devices(&self, devices: &[String]) -> Result<(), HidHideError> {
        self.write(format!("devices {devices:?}"), |c| {
            c.hidden_devices = devices.to_vec()
        })
    }

    fn set_allowed_apps(&self, apps: &[String]) -> Result<(), HidHideError> {
        self.write(format!("apps {apps:?}"), |c| c.allowed_apps = apps.to_vec())
    }

    fn set_active(&self, active: bool) -> Result<(), HidHideError> {
        self.write(format!("active {active}"), |c| c.active = active)
    }
}

struct MemoryJournal(Shared);

impl JournalStore for MemoryJournal {
    fn load(&self) -> std::io::Result<Journal> {
        Ok(self.0.lock().unwrap().journal.clone())
    }

    fn save(&self, journal: &Journal) -> std::io::Result<()> {
        let mut world = self.0.lock().unwrap();
        world.journal = journal.clone();
        world.journal_saves += 1;
        Ok(())
    }
}

/// The development PC: cloaking on, DS4Windows and x360ce allowed, another pad hidden.
fn dev_pc() -> HidHideConfig {
    HidHideConfig {
        active: true,
        inverse: false,
        hidden_devices: vec![OTHER_PAD.into()],
        allowed_apps: vec![DS4WINDOWS.into(), X360CE.into()],
    }
}

fn world(config: HidHideConfig) -> Shared {
    Arc::new(Mutex::new(World {
        config,
        ..World::default()
    }))
}

fn hiding(world: &Shared) -> Hiding {
    Hiding::new(
        Box::new(FakeHidHide(Arc::clone(world))),
        Box::new(MemoryJournal(Arc::clone(world))),
        HEKA.into(),
    )
}

fn config(world: &Shared) -> HidHideConfig {
    world.lock().unwrap().config.clone()
}

#[test]
fn hiding_a_controller_blacklists_it_and_allows_heka_keeping_other_entries() {
    let world = world(dev_pc());
    let hiding = hiding(&world);

    hiding.hide("pad-1", &path(USB_LINK)).unwrap();

    let config = config(&world);
    assert!(config.active);
    assert_eq!(config.hidden_devices, [OTHER_PAD, USB_LINK]);
    assert_eq!(config.allowed_apps, [DS4WINDOWS, X360CE, HEKA]);
}

#[test]
fn every_change_is_journalled_before_it_is_made() {
    let world = world(HidHideConfig::default());
    let hiding = hiding(&world);

    hiding.hide("pad-1", &path(USB_LINK)).unwrap();

    let writes = world.lock().unwrap().writes.clone();
    let whats: Vec<&str> = writes.iter().map(|w| w.what.as_str()).collect();
    assert_eq!(
        whats,
        [
            r#"apps ["\\Device\\HarddiskVolume3\\Tools\\heka\\heka.exe"]"#,
            r#"devices ["HID\\VID_054C&PID_0CE6&MI_03\\8&15C30336&0&0000"]"#,
            "active true",
        ]
    );
    assert_eq!(writes[0].journal_then.allowed_app.as_deref(), Some(HEKA));
    assert_eq!(writes[1].journal_then.hidden_devices, [USB_LINK]);
    assert!(writes[2].journal_then.cloak_was_off);
}

#[test]
fn ending_the_bridge_restores_the_dev_pc_configuration_exactly() {
    let world = world(dev_pc());
    let hiding = hiding(&world);

    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    hiding.release("pad-1").unwrap();

    assert_eq!(config(&world), dev_pc());
    assert!(world.lock().unwrap().journal.is_empty());
}

#[test]
fn ending_the_bridge_turns_cloaking_back_off_when_heka_turned_it_on() {
    let world = world(HidHideConfig::default());
    let hiding = hiding(&world);

    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    hiding.release("pad-1").unwrap();

    assert_eq!(config(&world), HidHideConfig::default());
}

#[test]
fn entries_that_were_there_before_heka_are_never_removed() {
    // The player already hid this Controller and allowed heka, in another letter case.
    let before = HidHideConfig {
        hidden_devices: vec![OTHER_PAD.into(), USB_LINK.to_lowercase()],
        allowed_apps: vec![DS4WINDOWS.into(), HEKA.to_uppercase()],
        ..dev_pc()
    };
    let world = world(before.clone());
    let hiding = hiding(&world);

    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    hiding.release("pad-1").unwrap();

    assert_eq!(config(&world), before);
    assert!(world.lock().unwrap().writes.is_empty());
}

#[test]
fn each_controller_is_restored_when_its_own_bridge_ends() {
    let world = world(HidHideConfig::default());
    let hiding = hiding(&world);

    // pad-1 has a USB and a Bluetooth Link; pad-2 is another Controller.
    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    hiding.hide("pad-1", &path(BT_LINK)).unwrap();
    hiding.hide("pad-2", &path(OTHER_PAD)).unwrap();
    assert_eq!(
        config(&world).hidden_devices,
        [USB_LINK, BT_LINK, OTHER_PAD]
    );

    hiding.release("pad-1").unwrap();
    let config_now = config(&world);
    assert_eq!(config_now.hidden_devices, [OTHER_PAD]);
    assert!(config_now.active, "pad-2 is still Hidden");
    assert_eq!(config_now.allowed_apps, [HEKA]);

    hiding.release("pad-2").unwrap();
    assert_eq!(config(&world), HidHideConfig::default());
}

#[test]
fn a_held_controller_stays_hidden_and_its_returning_link_changes_nothing() {
    let world = world(dev_pc());
    let hiding = hiding(&world);
    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    let writes_while_bridged = world.lock().unwrap().writes.len();

    // Held: the supervisor does not release. The Link returns and is passed again.
    hiding.hide("pad-1", &path(USB_LINK)).unwrap();

    assert_eq!(world.lock().unwrap().writes.len(), writes_while_bridged);
    assert_eq!(config(&world).hidden_devices, [OTHER_PAD, USB_LINK]);
}

#[test]
fn the_next_start_restores_what_a_killed_heka_left_hidden() {
    let world = world(HidHideConfig::default());
    let killed = hiding(&world);
    killed.hide("pad-1", &path(USB_LINK)).unwrap();
    killed.hide("pad-2", &path(OTHER_PAD)).unwrap();
    std::mem::forget(killed);

    hiding(&world).recover().unwrap();

    assert_eq!(config(&world), HidHideConfig::default());
    assert!(world.lock().unwrap().journal.is_empty());
}

#[test]
fn a_kill_between_journalling_and_changing_is_recovered_without_harm() {
    // The journal says heka added things, but the kill came before HidHide was changed.
    let world = world(dev_pc());
    world.lock().unwrap().journal = Journal {
        cloak_was_off: false,
        allowed_app: Some(HEKA.into()),
        hidden_devices: vec![USB_LINK.into()],
    };

    hiding(&world).recover().unwrap();

    assert_eq!(config(&world), dev_pc());
    assert!(world.lock().unwrap().writes.is_empty());
    assert!(world.lock().unwrap().journal.is_empty());
}

#[test]
fn quitting_restores_every_controller() {
    let world = world(dev_pc());
    let hiding = hiding(&world);
    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    hiding.hide("pad-2", &path(BT_LINK)).unwrap();

    hiding.restore_all().unwrap();

    assert_eq!(config(&world), dev_pc());
    assert!(world.lock().unwrap().journal.is_empty());
}

#[test]
fn with_an_inverted_application_list_heka_is_not_listed() {
    // Inverted, the list names applications that are denied: listing heka would blind it.
    let before = HidHideConfig {
        inverse: true,
        ..dev_pc()
    };
    let world = world(before.clone());
    let hiding = hiding(&world);

    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    assert_eq!(config(&world).allowed_apps, before.allowed_apps);

    hiding.release("pad-1").unwrap();
    assert_eq!(config(&world), before);
}

#[test]
fn a_busy_hidhide_is_reported_and_hiding_works_once_it_is_free() {
    let world = world(dev_pc());
    let hiding = hiding(&world);
    world.lock().unwrap().fail_with = Some(HidHideError::InUse);

    let error = hiding.hide("pad-1", &path(USB_LINK)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("close the HidHide Configuration Client"),
        "{error}"
    );
    assert_eq!(config(&world), dev_pc());

    // The supervisor tries again later, for example on its next poll.
    world.lock().unwrap().fail_with = None;
    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    hiding.release("pad-1").unwrap();
    assert_eq!(config(&world), dev_pc());
}

#[test]
fn a_restore_that_failed_is_finished_by_the_next_one() {
    let world = world(dev_pc());
    let hiding = hiding(&world);
    hiding.hide("pad-1", &path(USB_LINK)).unwrap();

    world.lock().unwrap().fail_with = Some(HidHideError::InUse);
    assert!(hiding.release("pad-1").is_err());
    world.lock().unwrap().fail_with = None;
    hiding.restore_all().unwrap();

    assert_eq!(config(&world), dev_pc());
    assert!(world.lock().unwrap().journal.is_empty());
}

#[test]
fn the_file_journal_survives_a_restart() {
    let dir = std::env::temp_dir().join(format!("heka-journal-test-{}", std::process::id()));
    let file = dir.join("hiding.journal");
    let journal = Journal {
        cloak_was_off: true,
        allowed_app: Some(HEKA.into()),
        hidden_devices: vec![USB_LINK.into(), BT_LINK.into()],
    };

    assert_eq!(
        FileJournal::new(file.clone()).load().unwrap(),
        Journal::default()
    );
    FileJournal::new(file.clone()).save(&journal).unwrap();
    assert_eq!(FileJournal::new(file.clone()).load().unwrap(), journal);
    FileJournal::new(file.clone())
        .save(&Journal::default())
        .unwrap();
    assert_eq!(
        FileJournal::new(file.clone()).load().unwrap(),
        Journal::default()
    );

    std::fs::write(&file, "not a journal").unwrap();
    assert!(FileJournal::new(file.clone()).load().is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn detection_reads_hidhide_and_changes_nothing() {
    let world = world(dev_pc());
    assert_eq!(
        HidHideStatus::detect(&FakeHidHide(Arc::clone(&world))),
        HidHideStatus::Ready
    );
    world.lock().unwrap().fail_with = Some(HidHideError::NotInstalled);
    assert_eq!(
        HidHideStatus::detect(&FakeHidHide(Arc::clone(&world))),
        HidHideStatus::NotInstalled
    );
    world.lock().unwrap().fail_with = Some(HidHideError::InUse);
    assert_eq!(
        HidHideStatus::detect(&FakeHidHide(Arc::clone(&world))),
        HidHideStatus::InUse
    );
    assert!(world.lock().unwrap().writes.is_empty());
}

/// The maintainer's PC: DS4Windows left the DualSense hidden and is the only allowed app.
fn hidden_by_ds4windows() -> HidHideConfig {
    HidHideConfig {
        active: true,
        inverse: false,
        hidden_devices: vec![USB_LINK.into()],
        allowed_apps: vec![DS4WINDOWS.into()],
    }
}

#[test]
fn heka_allows_itself_before_any_controller_is_bridged_journalling_it_first() {
    let world = world(hidden_by_ds4windows());
    let hiding = hiding(&world);

    hiding.allow_heka().unwrap();

    let writes = world.lock().unwrap().writes.clone();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].journal_then.allowed_app.as_deref(), Some(HEKA));
    let config_now = config(&world);
    assert_eq!(config_now.allowed_apps, [DS4WINDOWS, HEKA]);
    assert_eq!(config_now.hidden_devices, [USB_LINK]);
    assert!(config_now.active);
}

#[test]
fn heka_stays_allowed_after_the_last_bridge_ends_and_is_removed_on_quit() {
    let world = world(hidden_by_ds4windows());
    let hiding = hiding(&world);
    hiding.allow_heka().unwrap();

    // The Controller DS4Windows hid can now be opened and Bridged.
    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    hiding.release("pad-1").unwrap();
    assert_eq!(config(&world).allowed_apps, [DS4WINDOWS, HEKA]);
    assert_eq!(
        config(&world).hidden_devices,
        [USB_LINK],
        "heka never unhides what it did not hide"
    );

    hiding.restore_all().unwrap();
    assert_eq!(config(&world), hidden_by_ds4windows());
    assert!(world.lock().unwrap().journal.is_empty());
}

#[test]
fn ending_the_last_bridge_undoes_hiding_and_cloak_but_keeps_heka_allowed() {
    let world = world(HidHideConfig::default());
    let hiding = hiding(&world);
    hiding.allow_heka().unwrap();
    hiding.hide("pad-1", &path(USB_LINK)).unwrap();

    hiding.release("pad-1").unwrap();

    assert_eq!(
        config(&world),
        HidHideConfig {
            allowed_apps: vec![HEKA.into()],
            ..HidHideConfig::default()
        }
    );
    hiding.restore_all().unwrap();
    assert_eq!(config(&world), HidHideConfig::default());
    assert!(world.lock().unwrap().journal.is_empty());
}

#[test]
fn heka_allowed_by_someone_else_is_left_allowed_on_quit() {
    let before = HidHideConfig {
        allowed_apps: vec![DS4WINDOWS.into(), HEKA.to_lowercase()],
        ..hidden_by_ds4windows()
    };
    let world = world(before.clone());
    let hiding = hiding(&world);

    hiding.allow_heka().unwrap();
    hiding.restore_all().unwrap();

    assert_eq!(config(&world), before);
    assert!(world.lock().unwrap().writes.is_empty());
}

#[test]
fn with_an_inverted_application_list_heka_does_not_list_itself_at_start() {
    let before = HidHideConfig {
        inverse: true,
        ..hidden_by_ds4windows()
    };
    let world = world(before.clone());
    let hiding = hiding(&world);

    hiding.allow_heka().unwrap();
    hiding.restore_all().unwrap();

    assert_eq!(config(&world), before);
    assert!(world.lock().unwrap().writes.is_empty());
}

#[test]
fn the_next_start_removes_heka_from_the_allowed_apps_after_a_kill() {
    let world = world(hidden_by_ds4windows());
    let killed = hiding(&world);
    killed.allow_heka().unwrap();
    std::mem::forget(killed);

    let next = hiding(&world);
    next.recover().unwrap();
    assert_eq!(config(&world), hidden_by_ds4windows());

    // The next run allows itself again and cleans up on quit as usual.
    next.allow_heka().unwrap();
    assert_eq!(config(&world).allowed_apps, [DS4WINDOWS, HEKA]);
    next.restore_all().unwrap();
    assert_eq!(config(&world), hidden_by_ds4windows());
    assert!(world.lock().unwrap().journal.is_empty());
}

#[test]
fn a_busy_hidhide_at_start_is_reported_and_allowing_succeeds_on_retry() {
    let world = world(hidden_by_ds4windows());
    let hiding = hiding(&world);
    world.lock().unwrap().fail_with = Some(HidHideError::InUse);

    assert!(hiding.allow_heka().is_err());
    assert_eq!(config(&world), hidden_by_ds4windows());

    world.lock().unwrap().fail_with = None;
    hiding.allow_heka().unwrap();
    assert_eq!(config(&world).allowed_apps, [DS4WINDOWS, HEKA]);
    hiding.restore_all().unwrap();
    assert_eq!(config(&world), hidden_by_ds4windows());
}

#[test]
fn heka_allowed_while_hidhide_fails_mid_change_is_still_removed_on_quit() {
    // The journal is saved, then the write fails: the entry stays journalled and the
    // quit finds nothing to remove.
    let world = world(hidden_by_ds4windows());
    let hiding = hiding(&world);
    hiding.allow_heka().unwrap();
    world.lock().unwrap().fail_with = Some(HidHideError::InUse);
    assert!(hiding.restore_all().is_err());
    world.lock().unwrap().fail_with = None;

    hiding.restore_all().unwrap();

    assert_eq!(config(&world), hidden_by_ds4windows());
    assert!(world.lock().unwrap().journal.is_empty());
}

#[test]
fn a_link_removed_from_hidhide_by_someone_else_is_hidden_again_on_return() {
    let world = world(dev_pc());
    let hiding = hiding(&world);
    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    world.lock().unwrap().config = dev_pc();

    hiding.hide("pad-1", &path(USB_LINK)).unwrap();
    hiding.release("pad-1").unwrap();

    assert_eq!(config(&world), dev_pc());
    assert!(world.lock().unwrap().journal.is_empty());
}

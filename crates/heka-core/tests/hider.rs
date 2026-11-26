use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use heka_core::hider::{HideError, Hider, HidingWarning, INVERSE_CHECK_INTERVAL};
use heka_core::hiding::{
    DeviceInstancePath, HidHide, HidHideConfig, HidHideError, Hiding, HidingError,
    HidingUnavailable, Journal, JournalError, JournalStore,
};
use heka_core::lifecycle::{Change, Removal};
use heka_core::mac::MacAddress;
use heka_core::virtual_controller::OutputType;

const HEKA: &str = r"\Device\HarddiskVolume3\Tools\heka\heka.exe";
const FIRST: &str = r"HID\VID_054C&PID_0CE6&MI_03\8&15C30336&0&0000";
const SECOND: &str = r"HID\VID_054C&PID_0CE6&MI_03\8&22222222&0&0000";
const POLL: Duration = Duration::from_millis(250);

#[derive(Default)]
struct World {
    config: HidHideConfig,
    journal: Journal,
    busy: bool,
    refuse_unhiding: bool,
    unreadable_journal: bool,
}

type Shared = Arc<Mutex<World>>;

struct FakeHidHide(Shared);

impl FakeHidHide {
    fn write(&self, apply: impl FnOnce(&mut HidHideConfig)) -> Result<(), HidHideError> {
        let mut world = self.0.lock().unwrap();
        if world.busy {
            return Err(HidHideError::InUse);
        }
        apply(&mut world.config);
        Ok(())
    }
}

impl HidHide for FakeHidHide {
    fn read(&self) -> Result<HidHideConfig, HidHideError> {
        let world = self.0.lock().unwrap();
        if world.busy {
            return Err(HidHideError::InUse);
        }
        Ok(world.config.clone())
    }

    fn set_hidden_devices(&self, devices: &[String]) -> Result<(), HidHideError> {
        let unhiding = {
            let world = self.0.lock().unwrap();
            world.refuse_unhiding && devices.len() < world.config.hidden_devices.len()
        };
        if unhiding {
            return Err(HidHideError::Failed("it will not let go".into()));
        }
        self.write(|config| config.hidden_devices = devices.to_vec())
    }

    fn set_allowed_apps(&self, apps: &[String]) -> Result<(), HidHideError> {
        self.write(|config| config.allowed_apps = apps.to_vec())
    }

    fn set_active(&self, active: bool) -> Result<(), HidHideError> {
        self.write(|config| config.active = active)
    }
}

struct MemoryJournal(Shared);

impl JournalStore for MemoryJournal {
    fn load(&self) -> Result<Journal, JournalError> {
        let world = self.0.lock().unwrap();
        if world.unreadable_journal {
            return Err(JournalError::Io("the disk is busy".into()));
        }
        Ok(world.journal.clone())
    }

    fn save(&self, journal: &Journal) -> Result<(), JournalError> {
        self.0.lock().unwrap().journal = journal.clone();
        Ok(())
    }
}

fn hider(world: &Shared) -> Hider<&'static str> {
    let hiding = Hiding::new(
        Box::new(FakeHidHide(Arc::clone(world))),
        Box::new(MemoryJournal(Arc::clone(world))),
        HEKA.into(),
    );
    Hider::new(Ok(hiding), [])
}

fn mac(n: u8) -> MacAddress {
    MacAddress([0xA0, 0, 0, 0, 0, n])
}

fn seen(hider: &mut Hider<&'static str>, n: u8, device: &'static str) {
    hider.link_seen(mac(n), device, Ok(DeviceInstancePath::new(device)));
}

fn bridged(n: u8) -> Change {
    Change::Bridged {
        mac: mac(n),
        output_type: OutputType::Xbox,
    }
}

fn removed(n: u8) -> Change {
    Change::Removed {
        mac: mac(n),
        why: Removal::Stopped,
    }
}

fn hidden(world: &Shared) -> Vec<String> {
    world.lock().unwrap().config.hidden_devices.clone()
}

fn busy() -> HidingError {
    HidingError::HidHide(HidHideError::InUse)
}

fn hidden_first(world: &Shared) -> Hider<&'static str> {
    let mut hider = hider(world);
    seen(&mut hider, 1, FIRST);
    hider.follow(&[bridged(1)]);
    assert_eq!(hidden(world), [FIRST]);
    hider
}

fn killed_with_first_hidden() -> Shared {
    let world = Shared::default();
    std::mem::forget(hidden_first(&world));
    world
}

#[test]
fn a_failed_restore_is_retried_every_poll_until_it_succeeds() {
    let t0 = Instant::now();
    let world = Shared::default();
    let mut hider = hidden_first(&world);
    world.lock().unwrap().busy = true;

    hider.follow(&[removed(1)]);
    for poll in 0..5 {
        hider.poll(t0 + POLL * poll);
        assert_eq!(hider.warnings(), [HidingWarning::NotRestored(busy())]);
    }

    world.lock().unwrap().busy = false;
    hider.poll(t0 + POLL * 5);
    assert_eq!(hider.warnings(), []);
    assert_eq!(hidden(&world), Vec::<String>::new());
    assert!(!world.lock().unwrap().config.active);
}

#[test]
fn a_restore_warning_outlasts_the_success_of_something_else() {
    let t0 = Instant::now();
    let world = Shared::default();
    let mut hider = hidden_first(&world);
    world.lock().unwrap().refuse_unhiding = true;
    hider.follow(&[removed(1)]);

    seen(&mut hider, 2, SECOND);
    hider.follow(&[bridged(2)]);
    hider.poll(t0);

    assert_eq!(hidden(&world), [FIRST, SECOND]);
    assert!(matches!(
        hider.warnings()[..],
        [HidingWarning::NotRestored(_)]
    ));
    world.lock().unwrap().refuse_unhiding = false;
    hider.poll(t0 + POLL);
    assert_eq!(hidden(&world), [SECOND]);
    assert_eq!(hider.warnings(), []);
}

#[test]
fn a_controller_bridged_again_while_its_restore_is_pending_stays_hidden() {
    let t0 = Instant::now();
    let world = Shared::default();
    let mut hider = hidden_first(&world);
    world.lock().unwrap().busy = true;
    hider.follow(&[removed(1)]);

    hider.follow(&[bridged(1)]);
    world.lock().unwrap().busy = false;
    hider.poll(t0);

    assert_eq!(hidden(&world), [FIRST]);
    assert!(world.lock().unwrap().config.active);
    assert_eq!(hider.warnings(), []);
}

#[test]
fn a_failed_recovery_at_start_is_retried_until_it_succeeds() {
    let t0 = Instant::now();
    let world = killed_with_first_hidden();
    world.lock().unwrap().busy = true;

    let mut hider = hider(&world);
    hider.poll(t0);
    assert_eq!(
        hider.warnings(),
        [
            HidingWarning::EarlierRunNotRestored(busy()),
            HidingWarning::NotAllowed(busy()),
        ]
    );

    world.lock().unwrap().busy = false;
    hider.poll(t0 + POLL);
    assert_eq!(hider.warnings(), []);
    assert_eq!(hidden(&world), Vec::<String>::new());
    assert!(!world.lock().unwrap().config.active);
}

#[test]
fn nothing_is_changed_while_an_earlier_runs_journal_cannot_be_read() {
    let t0 = Instant::now();
    let world = killed_with_first_hidden();
    let journal = world.lock().unwrap().journal.clone();
    world.lock().unwrap().unreadable_journal = true;

    let mut hider = hider(&world);
    seen(&mut hider, 2, SECOND);
    hider.follow(&[bridged(2)]);
    hider.poll(t0);

    assert_eq!(world.lock().unwrap().journal, journal);
    assert_eq!(hidden(&world), [FIRST]);
    assert!(
        hider
            .warnings()
            .iter()
            .any(|warning| matches!(warning, HidingWarning::EarlierRunNotRestored(_)))
    );

    world.lock().unwrap().unreadable_journal = false;
    hider.poll(t0 + POLL);
    assert_eq!(hidden(&world), [SECOND]);
    assert_eq!(hider.warnings(), []);
}

#[test]
fn a_controller_the_player_lets_games_see_is_restored_after_a_failure_too() {
    let t0 = Instant::now();
    let world = Shared::default();
    let mut hider = hidden_first(&world);
    world.lock().unwrap().busy = true;

    hider.set_hide(mac(1), false);
    world.lock().unwrap().busy = false;
    hider.poll(t0);

    assert_eq!(hidden(&world), Vec::<String>::new());
    assert_eq!(hider.warnings(), []);
}

#[test]
fn a_failed_hide_is_retried_and_its_warning_goes_with_the_controller() {
    let t0 = Instant::now();
    let world = Shared::default();
    let mut hider = hider(&world);
    seen(&mut hider, 1, FIRST);
    world.lock().unwrap().busy = true;
    hider.follow(&[bridged(1)]);
    assert_eq!(
        hider.warnings(),
        [HidingWarning::NotHidden(HideError::Failed(busy()))]
    );

    world.lock().unwrap().busy = false;
    hider.poll(t0);
    assert_eq!(hider.warnings(), []);
    assert_eq!(hidden(&world), [FIRST]);

    hider.link_gone(mac(1), &FIRST);
    hider.follow(&[bridged(1)]);
    assert_eq!(
        hider.warnings(),
        [HidingWarning::NotHidden(HideError::NoLink)]
    );
    hider.follow(&[removed(1)]);
    assert_eq!(hider.warnings(), []);
}

#[test]
fn the_inverse_application_cloak_is_warned_about_until_it_is_off() {
    let t0 = Instant::now();
    let world = Shared::default();
    world.lock().unwrap().config.inverse = true;
    let mut hider = hidden_first(&world);

    hider.poll(t0);
    assert_eq!(hider.warnings(), [HidingWarning::Inverted]);
    assert!(world.lock().unwrap().config.inverse);

    world.lock().unwrap().busy = true;
    hider.poll(t0 + INVERSE_CHECK_INTERVAL);
    assert_eq!(hider.warnings(), [HidingWarning::Inverted]);

    world.lock().unwrap().busy = false;
    world.lock().unwrap().config.inverse = false;
    hider.poll(t0 + INVERSE_CHECK_INTERVAL * 2);
    assert_eq!(hider.warnings(), []);
}

#[test]
fn without_hidhide_the_reason_is_the_only_warning() {
    let t0 = Instant::now();
    let mut hider: Hider<&str> = Hider::new(Err(HidingUnavailable::NoLocalAppData), []);

    seen(&mut hider, 1, FIRST);
    hider.follow(&[bridged(1), removed(1)]);
    hider.poll(t0);

    assert_eq!(
        hider.warnings(),
        [HidingWarning::Unavailable(
            HidingUnavailable::NoLocalAppData
        )]
    );
    assert!(hider.restore_all().is_ok());
}

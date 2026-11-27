use std::collections::BTreeMap;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use heka_app::report::Reporter;
use heka_app::{log, text};
use heka_core::app_settings::AppSettings;
use heka_core::discovery::{Discovery, Finder, Found, FoundLink, OpenFailure, Unopened};
use heka_core::feedback::Motor;
use heka_core::hider::{Hider, HidingWarning};
use heka_core::idle::IdleTime;
use heka_core::lifecycle::Change;
use heka_core::link::controller_model;
use heka_core::mac::MacAddress;
use heka_core::preferences::PreferenceChange;
use heka_core::profile::{Binding, Input, Profile};
use heka_core::profiles::Profiles;
use heka_core::settings::Settings;
use heka_core::setup::{self, Driver, Setup};
use heka_core::supervisor::{ControllerView, Supervisor};
use heka_core::tuning::{StickChange, StickSide, TriggerChange, TriggerSide};
use heka_core::virtual_controller::{OutputType, VirtualFailure};
use heka_windows::bluetooth::BluetoothRadios;
use heka_windows::drivers::InstalledDrivers;
use heka_windows::foreground;
use heka_windows::hid::{ControllerLink, OpenError, controller_paths};
use heka_windows::hidhide;
use hidapi::HidApi;

use crate::{profiles_file, settings_file};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const LOG_FLUSH: Duration = Duration::from_secs(1);
const FOREGROUND_POLL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Starting,
    HidError(String),
    Running,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub status: Status,
    pub setup: Option<Setup>,
    pub controllers: Vec<ControllerView>,
    pub profiles: Profiles,
    pub profile_notice: Option<String>,
    pub profile_users: BTreeMap<String, usize>,
    pub foreground_unreadable: bool,
    pub hiding: Vec<HidingWarning>,
    pub unopened: Vec<Unopened>,
    pub not_unplugged: Vec<String>,
}

enum Command {
    Stop(MacAddress),
    Start(MacAddress),
    SetOutputType(MacAddress, OutputType),
    SetNickname(MacAddress, String),
    ChangePreference(MacAddress, PreferenceChange),
    TestRumble(MacAddress, Motor),
    EndRumbleTest(MacAddress),
    TurnOff(MacAddress),
    SetIdleTime(MacAddress, Option<IdleTime>),
    CheckDrivers,
    SetAppSettings(AppSettings),
    ChooseProfile(MacAddress, Option<String>),
    CreateProfile(String),
    BindInput(String, Input, Binding),
    SetProfileOutputType(String, Option<OutputType>),
    DuplicateProfile(Option<String>),
    RenameProfile(String, String),
    DeleteProfile(String),
    ImportProfile(PathBuf),
    ExportProfile(String, PathBuf),
    ChangeStick(String, StickSide, StickChange),
    SetSwapSticks(String, bool),
    ChangeTrigger(String, TriggerSide, TriggerChange),
    Quit,
}

pub struct Bridging {
    commands: Sender<Command>,
    thread: Mutex<Option<JoinHandle<()>>>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl Bridging {
    pub fn start(settings: Settings, profiles: Profiles) -> Self {
        let (commands, inbox) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(Snapshot {
            status: Status::Starting,
            setup: None,
            controllers: Vec::new(),
            profiles: Profiles::default(),
            profile_notice: None,
            profile_users: BTreeMap::new(),
            foreground_unreadable: false,
            hiding: Vec::new(),
            unopened: Vec::new(),
            not_unplugged: Vec::new(),
        }));
        let thread = thread::Builder::new()
            .name("heka-supervisor".into())
            .spawn({
                let snapshot = Arc::clone(&snapshot);
                move || supervise(settings, profiles, &inbox, &snapshot)
            })
            .expect("failed to start the supervisor thread");
        Self {
            commands,
            thread: Mutex::new(Some(thread)),
            snapshot,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn stop_controller(&self, mac: MacAddress) {
        let _ = self.commands.send(Command::Stop(mac));
    }

    pub fn start_controller(&self, mac: MacAddress) {
        let _ = self.commands.send(Command::Start(mac));
    }

    pub fn set_output_type(&self, mac: MacAddress, output_type: OutputType) {
        let _ = self.commands.send(Command::SetOutputType(mac, output_type));
    }

    pub fn set_nickname(&self, mac: MacAddress, name: String) {
        let _ = self.commands.send(Command::SetNickname(mac, name));
    }

    pub fn change_preference(&self, mac: MacAddress, change: PreferenceChange) {
        let _ = self.commands.send(Command::ChangePreference(mac, change));
    }

    pub fn test_rumble(&self, mac: MacAddress, motor: Motor) {
        let _ = self.commands.send(Command::TestRumble(mac, motor));
    }

    pub fn end_rumble_test(&self, mac: MacAddress) {
        let _ = self.commands.send(Command::EndRumbleTest(mac));
    }

    pub fn turn_off(&self, mac: MacAddress) {
        let _ = self.commands.send(Command::TurnOff(mac));
    }

    pub fn set_idle_time(&self, mac: MacAddress, idle_time: Option<IdleTime>) {
        let _ = self.commands.send(Command::SetIdleTime(mac, idle_time));
    }

    pub fn check_drivers(&self) {
        let _ = self.commands.send(Command::CheckDrivers);
    }

    pub fn set_app_settings(&self, app: AppSettings) {
        let _ = self.commands.send(Command::SetAppSettings(app));
    }

    pub fn choose_profile(&self, mac: MacAddress, profile: Option<String>) {
        let _ = self.commands.send(Command::ChooseProfile(mac, profile));
    }

    pub fn create_profile(&self, name: String) {
        let _ = self.commands.send(Command::CreateProfile(name));
    }

    pub fn bind_input(&self, profile: String, input: Input, binding: Binding) {
        let _ = self
            .commands
            .send(Command::BindInput(profile, input, binding));
    }

    pub fn set_profile_output_type(&self, profile: String, output_type: Option<OutputType>) {
        let _ = self
            .commands
            .send(Command::SetProfileOutputType(profile, output_type));
    }

    pub fn duplicate_profile(&self, profile: Option<String>) {
        let _ = self.commands.send(Command::DuplicateProfile(profile));
    }

    pub fn rename_profile(&self, profile: String, name: String) {
        let _ = self.commands.send(Command::RenameProfile(profile, name));
    }

    pub fn delete_profile(&self, profile: String) {
        let _ = self.commands.send(Command::DeleteProfile(profile));
    }

    pub fn import_profile(&self, path: PathBuf) {
        let _ = self.commands.send(Command::ImportProfile(path));
    }

    pub fn export_profile(&self, profile: String, path: PathBuf) {
        let _ = self.commands.send(Command::ExportProfile(profile, path));
    }

    pub fn change_stick(&self, profile: String, side: StickSide, change: StickChange) {
        let _ = self
            .commands
            .send(Command::ChangeStick(profile, side, change));
    }

    pub fn set_swap_sticks(&self, profile: String, swap: bool) {
        let _ = self.commands.send(Command::SetSwapSticks(profile, swap));
    }

    pub fn change_trigger(&self, profile: String, side: TriggerSide, change: TriggerChange) {
        let _ = self
            .commands
            .send(Command::ChangeTrigger(profile, side, change));
    }

    pub fn quit(&self) {
        let _ = self.commands.send(Command::Quit);
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(thread) = thread {
            let _ = thread.join();
        }
        log::flush(LOG_FLUSH);
    }
}

impl Drop for Bridging {
    fn drop(&mut self) {
        self.quit();
    }
}

fn supervise(
    settings: Settings,
    profiles: Profiles,
    inbox: &mpsc::Receiver<Command>,
    snapshot: &Mutex<Snapshot>,
) {
    let mut hider = Hider::new(hidhide::hiding(), settings.never_hidden());
    let mut supervisor =
        Supervisor::without_bus(settings).turning_off_with(Box::new(BluetoothRadios));
    supervisor.set_profiles(profiles, Instant::now());
    let (mut detected, changes) = setup::check(&InstalledDrivers, &mut supervisor, Instant::now());
    log_all(text::log::drivers(&detected));
    follow(&mut hider, &changes);
    let mut reporter = Reporter::default();
    let mut discovery = Discovery::default();
    let mut api = None;
    let mut status = Status::Starting;
    let mut not_unplugged = Vec::new();
    let mut looked: Option<Instant> = None;
    let mut unsaved = false;
    let mut profile_notice: Option<String> = None;
    let mut next_foreground = Instant::now();
    let mut watching_foreground = false;
    let mut foreground_unreadable = false;
    loop {
        let command = inbox.recv_timeout(POLL_INTERVAL);
        let now = Instant::now();
        let burst = matches!(
            command,
            Ok(Command::SetNickname(..)
                | Command::ChangePreference(..)
                | Command::TestRumble(..)
                | Command::EndRumbleTest(..)
                | Command::SetAppSettings(..))
        );
        let changes = match command {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Command::Stop(mac)) => {
                unsaved = true;
                supervisor.stop(mac, now)
            }
            Ok(Command::Start(mac)) => {
                unsaved = true;
                supervisor.start(mac, now)
            }
            Ok(Command::SetOutputType(mac, output_type)) => {
                unsaved = true;
                supervisor.set_output_type(mac, output_type, now)
            }
            Ok(Command::SetNickname(mac, name)) => {
                unsaved = true;
                supervisor.set_nickname(mac, &name);
                Vec::new()
            }
            Ok(Command::ChangePreference(mac, change)) => {
                unsaved = true;
                let preferences = supervisor.change_preference(mac, change);
                hider.set_hide(mac, preferences.hide);
                Vec::new()
            }
            Ok(Command::TestRumble(mac, motor)) => {
                supervisor.test_rumble(mac, motor);
                Vec::new()
            }
            Ok(Command::EndRumbleTest(mac)) => {
                supervisor.end_rumble_test(mac);
                Vec::new()
            }
            Ok(Command::TurnOff(mac)) => supervisor.turn_off(mac, now),
            Ok(Command::SetIdleTime(mac, idle_time)) => {
                unsaved = true;
                supervisor.set_idle_time(mac, idle_time);
                Vec::new()
            }
            Ok(Command::SetAppSettings(app)) => {
                unsaved = true;
                supervisor.set_app_settings(app);
                Vec::new()
            }
            Ok(Command::ChooseProfile(mac, profile)) => {
                unsaved = true;
                supervisor.choose_profile(mac, profile, now)
            }
            Ok(Command::CreateProfile(name)) => {
                profile_notice = None;
                let mut profiles = supervisor.profiles().clone();
                let id = profiles.create(&name);
                save_profile(&profiles, &id);
                supervisor.set_profiles(profiles, now)
            }
            Ok(Command::BindInput(id, input, binding)) => {
                edit_profile(&mut supervisor, &id, now, |profile| {
                    profile.bind(input, binding);
                })
            }
            Ok(Command::SetProfileOutputType(id, output_type)) => {
                edit_profile(&mut supervisor, &id, now, |profile| {
                    profile.output_type = output_type;
                })
            }
            Ok(Command::DuplicateProfile(source)) => {
                profile_notice = None;
                let mut profiles = supervisor.profiles().clone();
                if let Some(id) = profiles.duplicate(source.as_deref()) {
                    save_profile(&profiles, &id);
                }
                supervisor.set_profiles(profiles, now)
            }
            Ok(Command::RenameProfile(id, name)) => {
                profile_notice = None;
                let mut profiles = supervisor.profiles().clone();
                if profiles.rename(&id, &name) {
                    save_profile(&profiles, &id);
                }
                supervisor.set_profiles(profiles, now)
            }
            Ok(Command::DeleteProfile(id)) => {
                profile_notice = None;
                unsaved = true;
                let changes = supervisor.delete_profile(&id, now);
                profiles_file::remove(&id);
                changes
            }
            Ok(Command::ImportProfile(path)) => {
                profile_notice = None;
                import_profile(&mut supervisor, &path, now, &mut profile_notice)
            }
            Ok(Command::ExportProfile(id, path)) => {
                profile_notice = None;
                if let Some((_, content)) = supervisor.profiles().export(&id)
                    && let Err(message) = profiles_file::write_export(&path, &content)
                {
                    log::line(&message);
                    profile_notice = Some(message);
                }
                Vec::new()
            }
            Ok(Command::ChangeStick(id, side, change)) => {
                edit_profile(&mut supervisor, &id, now, |profile| {
                    profile.change_stick(side, change);
                })
            }
            Ok(Command::SetSwapSticks(id, swap)) => {
                edit_profile(&mut supervisor, &id, now, |profile| {
                    profile.swap_sticks = swap;
                })
            }
            Ok(Command::ChangeTrigger(id, side, change)) => {
                edit_profile(&mut supervisor, &id, now, |profile| {
                    profile.change_trigger(side, change);
                })
            }
            Ok(Command::CheckDrivers) => {
                let (found, changes) = setup::check(&InstalledDrivers, &mut supervisor, now);
                detected = found;
                log_all(text::log::drivers(&detected));
                changes
            }
            Err(RecvTimeoutError::Timeout) => Vec::new(),
        };
        follow(&mut hider, &changes);

        if now >= next_foreground {
            next_foreground = now + FOREGROUND_POLL;
            let has_rules = !supervisor.settings().app().game_rules.is_empty();
            if has_rules || watching_foreground {
                let seen = if has_rules {
                    foreground::foreground()
                } else {
                    foreground::Foreground::Nothing
                };
                foreground_unreadable = seen.is_unreadable();
                let changes =
                    supervisor.set_foreground(seen.as_front(), &foreground::is_running, now);
                follow(&mut hider, &changes);
                watching_foreground = has_rules;
            }
        }

        let due = looked.is_none_or(|at| now.duration_since(at) >= POLL_INTERVAL);
        if !burst || due {
            if unsaved {
                settings_file::save(supervisor.settings());
                unsaved = false;
            }
            status = match refreshed(&mut api) {
                Ok(api) => {
                    let found = discovery.scan(&mut HidFinder(api), &mut supervisor, now);
                    act_on(found, &mut hider);
                    Status::Running
                }
                Err(error) => Status::HidError(error),
            };
            follow(&mut hider, &supervisor.poll(now));
            hider.poll(now);
            looked = Some(now);
        }
        report_failures(&mut supervisor, &mut not_unplugged);
        log_hiding_changes(&mut hider);
        let hiding = if detected.is_missing(Driver::HidHide) {
            Vec::new()
        } else {
            hider.warnings()
        };
        let controllers = supervisor.controllers();
        log_all(reporter.controllers(&controllers));
        log_all(reporter.hiding(&hiding));
        match &status {
            Status::HidError(error) => log_all(reporter.hid_error(Some(error))),
            Status::Running => log_all(reporter.hid_error(None)),
            Status::Starting => {}
        }
        publish(
            snapshot,
            Snapshot {
                status: status.clone(),
                setup: Some(detected.clone()),
                controllers,
                profiles: supervisor.profiles().clone(),
                profile_notice: profile_notice.clone(),
                profile_users: supervisor.settings().profile_users(),
                foreground_unreadable,
                hiding,
                unopened: discovery.unopened().cloned().collect(),
                not_unplugged: not_unplugged.clone(),
            },
        );
    }
    log::line(text::log::QUITTING);
    if unsaved {
        settings_file::save(supervisor.settings());
    }
    follow(&mut hider, &supervisor.quit(Instant::now()));
    report_failures(&mut supervisor, &mut not_unplugged);
    if let Err(error) = hider.restore_all() {
        log::line(&text::log::cannot_restore_on_quit(&error));
    }
    log_hiding_changes(&mut hider);
}

fn save_profile(profiles: &Profiles, id: &str) {
    if let Some(profile) = profiles.get(id) {
        profiles_file::save(id, profile);
    }
}

fn import_profile(
    supervisor: &mut Supervisor,
    path: &Path,
    now: Instant,
    notice: &mut Option<String>,
) -> Vec<Change> {
    let (content, name) = match profiles_file::read_import(path) {
        Ok(read) => read,
        Err(message) => {
            log::line(&message);
            *notice = Some(message);
            return Vec::new();
        }
    };
    let mut profiles = supervisor.profiles().clone();
    match profiles.import(&content, &name) {
        Ok(id) => {
            save_profile(&profiles, &id);
            supervisor.set_profiles(profiles, now)
        }
        Err(error) => {
            let message = text::cannot_import(path, &error);
            log::line(&message);
            *notice = Some(message);
            Vec::new()
        }
    }
}

fn edit_profile(
    supervisor: &mut Supervisor,
    id: &str,
    now: Instant,
    change: impl FnOnce(&mut Profile),
) -> Vec<Change> {
    let mut profiles = supervisor.profiles().clone();
    let Some(profile) = profiles.get_mut(id) else {
        return Vec::new();
    };
    change(profile);
    save_profile(&profiles, id);
    supervisor.set_profiles(profiles, now)
}

fn publish(snapshot: &Mutex<Snapshot>, latest: Snapshot) {
    *snapshot.lock().unwrap_or_else(PoisonError::into_inner) = latest;
}

fn log_all(messages: impl IntoIterator<Item = String>) {
    for message in messages {
        log::line(&message);
    }
}

fn log_hiding_changes(hider: &mut Hider<CString>) {
    log_all(hider.take_changes().iter().map(text::log::hiding_change));
}

fn follow(hider: &mut Hider<CString>, changes: &[Change]) {
    log_all(changes.iter().map(text::log::change));
    hider.follow(changes);
}

fn act_on(found: Vec<Found<CString>>, hider: &mut Hider<CString>) {
    for found in found {
        match found {
            Found::Seen {
                mac,
                path,
                model,
                link,
                problems,
            } => {
                log::line(&text::log::link_found(mac, model, link));
                log_all(
                    problems
                        .iter()
                        .map(|problem| text::log::link_problem(mac, problem)),
                );
                let device = hidhide::device_instance_path(&path);
                hider.link_seen(mac, path, device);
            }
            Found::Gone { mac, path } => {
                log::line(&text::log::link_gone(mac));
                hider.link_gone(mac, &path);
            }
            Found::Changes(changes) => follow(hider, &changes),
            Found::CannotOpen(unopened) => {
                log::line(&text::cannot_open(&unopened));
            }
        }
    }
}

fn report_failures(supervisor: &mut Supervisor, not_unplugged: &mut Vec<String>) {
    for failure in supervisor.take_virtual_failures() {
        match failure {
            VirtualFailure::NoFeedback(error) => {
                log::line(&text::log::no_virtual_feedback(&error));
            }
            VirtualFailure::NotReleased(error) => log::line(&text::not_released(&error)),
            VirtualFailure::NotUnplugged(error) => {
                log::line(&text::not_unplugged(&error));
                if !not_unplugged.contains(&error) {
                    not_unplugged.push(error);
                }
            }
        }
    }
}

fn refreshed(api: &mut Option<HidApi>) -> Result<&HidApi, String> {
    let api = match api {
        Some(api) => {
            api.refresh_devices().map_err(|e| e.to_string())?;
            api
        }
        None => api.insert(HidApi::new().map_err(|e| e.to_string())?),
    };
    Ok(api)
}

struct HidFinder<'a>(&'a HidApi);

impl Finder for HidFinder<'_> {
    type Path = CString;

    fn present(&mut self) -> Vec<CString> {
        controller_paths(self.0)
    }

    fn open(&mut self, path: &CString) -> Result<FoundLink, OpenFailure> {
        match ControllerLink::open_path(self.0, path) {
            Ok((link, mac, problems)) => Ok(FoundLink {
                mac,
                reader: Box::new(link),
                problems,
            }),
            Err(OpenError::VirtualController) => Err(OpenFailure::VirtualController),
            Err(OpenError::Refused(error)) => {
                let model = self
                    .0
                    .device_list()
                    .find(|info| info.path() == path.as_c_str())
                    .and_then(|info| controller_model(info.vendor_id(), info.product_id()))
                    .unwrap_or_default();
                Err(OpenFailure::Refused(Unopened { model, error }))
            }
        }
    }
}

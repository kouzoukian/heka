use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use crate::app_settings::AppSettings;
use crate::bridge::{Bridge, BridgeEnd, LiveInput, OpenLink, Plugged};
use crate::feedback::Motor;
use crate::idle::{IdleTime, IdleTimer};
use crate::lifecycle::{Change, Lifecycle, Phase, Removal};
use crate::link::{ControllerModel, LinkKind, LinkReader, active_link};
use crate::mac::MacAddress;
use crate::preferences::{LivePreferences, PreferenceChange, Preferences};
use crate::radio::{Radio, TurnOffError};
use crate::settings::Settings;
use crate::threads::{SystemThreads, Threads};
use crate::virtual_controller::{
    OutputType, VirtualBus, VirtualError, VirtualFailure, VirtualFailures,
};
use crate::watch::Watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkId(u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    NoBus,
    NoThread(String),
    VirtualController(String),
}

impl BridgeError {
    fn no_thread(error: &std::io::Error) -> Self {
        Self::NoThread(error.to_string())
    }

    fn refused(error: &VirtualError) -> Self {
        Self::VirtualController(error.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct ControllerView {
    pub mac: MacAddress,
    pub model: ControllerModel,
    pub phase: Phase,
    pub output_type: OutputType,
    pub nickname: Option<String>,
    pub preferences: Preferences,
    pub active_link: Option<LinkKind>,
    pub has_bluetooth: bool,
    pub idle_time: Option<IdleTime>,
    pub live: Arc<LiveInput>,
    pub error: Option<BridgeError>,
    pub turn_off_error: Option<TurnOffError>,
}

#[derive(Clone, Copy)]
enum Departure {
    Lost,
    TurnedOff,
}

struct LinkSlot {
    id: LinkId,
    kind: LinkKind,
    open: Option<OpenLink>,
}

#[derive(Default)]
struct Slot {
    model: ControllerModel,
    live: Arc<LiveInput>,
    preferences: Arc<LivePreferences>,
    links: Vec<LinkSlot>,
    reading: Option<LinkId>,
    bridge: Option<Bridge>,
    watch: Option<Watch>,
    held: Option<Plugged>,
    error: Option<BridgeError>,
    watch_error: Option<BridgeError>,
    idle: IdleTimer,
    turn_off_error: Option<TurnOffError>,
}

impl Slot {
    fn active(&self) -> Option<&LinkSlot> {
        let kinds: Vec<LinkKind> = self.links.iter().map(|link| link.kind).collect();
        active_link(&kinds).map(|index| &self.links[index])
    }

    fn active_id(&self) -> Option<LinkId> {
        self.active().map(|link| link.id)
    }

    fn bluetooth_links(&self) -> impl Iterator<Item = LinkId> + '_ {
        self.links
            .iter()
            .filter(|link| link.kind == LinkKind::Bluetooth)
            .map(|link| link.id)
    }

    fn reads_bluetooth(&self) -> bool {
        self.reading
            .is_some_and(|id| self.bluetooth_links().any(|link| link == id))
    }

    fn reads_inactive_link(&self) -> bool {
        (self.bridge.is_some() || self.watch.is_some()) && self.reading != self.active_id()
    }

    fn take_active(&mut self) -> Option<OpenLink> {
        let id = self.active_id()?;
        let link = self
            .links
            .iter_mut()
            .find(|link| link.id == id)?
            .open
            .take()?;
        self.reading = Some(id);
        Some(link)
    }

    fn put_back(&mut self, link: OpenLink) {
        let id = self.reading.take();
        if let Some(slot) = self.links.iter_mut().find(|slot| Some(slot.id) == id) {
            slot.open = Some(link);
        }
    }

    fn forget_read(&mut self) -> Option<LinkId> {
        let id = self.reading.take()?;
        self.drop_link(id);
        Some(id)
    }

    fn drop_link(&mut self, id: LinkId) {
        if self.bluetooth_links().any(|link| link == id) {
            self.turn_off_error = None;
        }
        self.links.retain(|link| link.id != id);
    }
}

pub struct Supervisor {
    bus: Option<Box<dyn VirtualBus>>,
    lifecycle: Lifecycle,
    slots: HashMap<MacAddress, Slot>,
    next_link_id: u64,
    threads: Arc<dyn Threads>,
    failures: Arc<VirtualFailures>,
    dropped: Vec<LinkId>,
    radio: Option<Box<dyn Radio>>,
    turned_off: Vec<LinkId>,
}

impl Supervisor {
    pub fn new(bus: Box<dyn VirtualBus>, settings: Settings) -> Self {
        Self::with(Some(bus), Lifecycle::new(settings))
    }

    pub fn without_bus(settings: Settings) -> Self {
        Self::with(None, Lifecycle::paused(settings))
    }

    fn with(bus: Option<Box<dyn VirtualBus>>, lifecycle: Lifecycle) -> Self {
        Self {
            bus,
            lifecycle,
            slots: HashMap::new(),
            next_link_id: 0,
            threads: Arc::new(SystemThreads),
            failures: Arc::default(),
            dropped: Vec::new(),
            radio: None,
            turned_off: Vec::new(),
        }
    }

    pub fn spawning_with(mut self, threads: Arc<dyn Threads>) -> Self {
        self.threads = threads;
        self
    }

    pub fn turning_off_with(mut self, radio: Box<dyn Radio>) -> Self {
        self.radio = Some(radio);
        self
    }

    pub fn has_bus(&self) -> bool {
        self.bus.is_some()
    }

    pub fn connect_bus(&mut self, bus: Box<dyn VirtualBus>, now: Instant) -> Vec<Change> {
        if self.has_bus() {
            return Vec::new();
        }
        self.bus = Some(bus);
        let watched: Vec<MacAddress> = self.slots.keys().copied().collect();
        let mut changes: Vec<Change> = watched
            .into_iter()
            .flat_map(|mac| self.stop_watch(mac, now))
            .collect();
        changes.extend(self.lifecycle.resume_bridging(now));
        self.apply(changes, now)
    }

    pub fn settings(&self) -> &Settings {
        self.lifecycle.settings()
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.lifecycle.next_deadline()
    }

    pub fn take_dropped_links(&mut self) -> Vec<LinkId> {
        std::mem::take(&mut self.dropped)
    }

    pub fn take_turned_off_links(&mut self) -> Vec<LinkId> {
        std::mem::take(&mut self.turned_off)
    }

    pub fn take_virtual_failures(&mut self) -> Vec<VirtualFailure> {
        self.failures.take()
    }

    pub fn controllers(&self) -> Vec<ControllerView> {
        self.lifecycle
            .controllers()
            .map(|(mac, phase)| {
                let slot = self.slots.get(&mac);
                let settings = self.settings().controller(mac);
                ControllerView {
                    mac,
                    model: slot.map(|slot| slot.model).unwrap_or_default(),
                    phase,
                    output_type: settings.output_type,
                    nickname: settings.nickname,
                    preferences: settings.preferences,
                    active_link: slot.and_then(Slot::active).map(|link| link.kind),
                    has_bluetooth: slot.is_some_and(|slot| slot.bluetooth_links().next().is_some()),
                    idle_time: settings.idle_time,
                    live: slot.map(|slot| Arc::clone(&slot.live)).unwrap_or_default(),
                    error: slot.and_then(|slot| slot.error.clone().or(slot.watch_error.clone())),
                    turn_off_error: slot.and_then(|slot| slot.turn_off_error.clone()),
                }
            })
            .collect()
    }

    pub fn link_arrived(
        &mut self,
        mac: MacAddress,
        link: Box<dyn LinkReader>,
        now: Instant,
    ) -> (LinkId, Vec<Change>) {
        let id = LinkId(self.next_link_id);
        self.next_link_id += 1;
        if mac.is_virtual_controller() {
            return (id, Vec::new());
        }
        let link = OpenLink::new(link);
        let format = link.format();
        let slot = self.slot(mac);
        slot.model = format.model();
        slot.links.push(LinkSlot {
            id,
            kind: format.kind(),
            open: Some(link),
        });
        let changes = self.lifecycle.link_arrived(mac, now);
        let mut done = self.apply(changes, now);
        done.extend(self.follow_active_link(mac, now));
        (id, done)
    }

    pub fn link_gone(&mut self, id: LinkId, now: Instant) -> Vec<Change> {
        self.remove_link(id, Departure::Lost, now)
    }

    pub fn turn_off(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let Some(slot) = self.slots.get_mut(&mac) else {
            return Vec::new();
        };
        let links: Vec<LinkId> = slot.bluetooth_links().collect();
        if links.is_empty() {
            return Vec::new();
        }
        let ended = self
            .radio
            .as_ref()
            .ok_or(TurnOffError::NoRadio)
            .and_then(|radio| radio.end_link(mac));
        slot.turn_off_error = ended.clone().err();
        if ended.is_err() {
            return Vec::new();
        }
        let mut done = Vec::new();
        for id in links {
            self.turned_off.push(id);
            done.extend(self.remove_link(id, Departure::TurnedOff, now));
        }
        done
    }

    fn remove_link(&mut self, id: LinkId, departure: Departure, now: Instant) -> Vec<Change> {
        let Some((&mac, _)) = self
            .slots
            .iter()
            .find(|(_, slot)| slot.links.iter().any(|link| link.id == id))
        else {
            return Vec::new();
        };
        let slot = self.slots.get_mut(&mac).expect("found above");
        let mut moving = None;
        let mut failed = false;
        if slot.reading == Some(id) {
            slot.watch = None;
            if let Some(bridge) = slot.bridge.take() {
                match bridge.stop() {
                    BridgeEnd::Stopped { controller, .. } | BridgeEnd::LinkLost { controller } => {
                        moving = Some(controller)
                    }
                    BridgeEnd::VirtualControllerFailed { error, .. } => {
                        slot.error = Some(BridgeError::refused(&error));
                        failed = true;
                    }
                }
            }
            slot.reading = None;
        }
        slot.drop_link(id);
        let mut changes = Vec::new();
        if failed {
            changes.extend(self.lifecycle.bridge_failed(mac, now));
        }
        match (departure, moving) {
            (Departure::Lost, Some(controller)) => {
                changes.extend(self.lost_read_link(mac, controller, now));
            }
            (Departure::Lost, None) => changes.extend(self.lifecycle.link_lost(mac, now)),
            (Departure::TurnedOff, moving) => {
                changes.extend(self.turned_off_link(mac, moving, now));
            }
        }
        self.apply(changes, now)
    }

    fn turned_off_link(
        &mut self,
        mac: MacAddress,
        moving: Option<Plugged>,
        now: Instant,
    ) -> Vec<Change> {
        let mut changes = self.lifecycle.link_turned_off(mac, now);
        if let Some(controller) = moving {
            if self.slot(mac).links.is_empty() {
                let _ = controller.unplug();
            } else {
                changes.extend(self.move_bridge(mac, controller, now));
            }
        }
        changes
    }

    pub fn poll(&mut self, now: Instant) -> Vec<Change> {
        let ended: Vec<MacAddress> = self
            .slots
            .iter()
            .filter(|(_, slot)| slot.bridge.as_ref().is_some_and(Bridge::is_finished))
            .map(|(&mac, _)| mac)
            .collect();
        let mut done = Vec::new();
        for mac in ended {
            let Some(slot) = self.slots.get_mut(&mac) else {
                continue;
            };
            let Some(bridge) = slot.bridge.take() else {
                continue;
            };
            let changes = match bridge.stop() {
                BridgeEnd::LinkLost { controller } => {
                    self.dropped.extend(slot.forget_read());
                    self.lost_read_link(mac, controller, now)
                }
                BridgeEnd::VirtualControllerFailed { link, error } => {
                    slot.put_back(link);
                    slot.error = Some(BridgeError::refused(&error));
                    self.lifecycle.bridge_failed(mac, now)
                }
                BridgeEnd::Stopped { link, controller } => {
                    slot.put_back(link);
                    let _ = controller.unplug();
                    self.lifecycle.bridge_failed(mac, now)
                }
            };
            done.extend(self.apply(changes, now));
        }
        let unread: Vec<MacAddress> = self
            .slots
            .iter()
            .filter(|(_, slot)| slot.watch.as_ref().is_some_and(Watch::is_finished))
            .map(|(&mac, _)| mac)
            .collect();
        for mac in unread {
            let Some(slot) = self.slots.get_mut(&mac) else {
                continue;
            };
            if slot.watch.take().is_none() {
                continue;
            }
            self.dropped.extend(slot.forget_read());
            let changes = self.lifecycle.link_lost(mac, now);
            done.extend(self.apply(changes, now));
        }
        let changes = self.lifecycle.tick(now);
        done.extend(self.apply(changes, now));
        done.extend(self.turn_off_idle(now));
        done
    }

    fn turn_off_idle(&mut self, now: Instant) -> Vec<Change> {
        let settings = self.lifecycle.settings();
        let mut idle = Vec::new();
        for (&mac, slot) in &mut self.slots {
            let reads_bluetooth = slot.reads_bluetooth();
            match settings.controller(mac).idle_time {
                Some(time) if reads_bluetooth => {
                    let used = slot.live.times_used();
                    if slot.idle.runs_out(time.duration(), used, now) {
                        idle.push(mac);
                    }
                }
                _ => slot.idle.restart(),
            }
        }
        idle.into_iter()
            .flat_map(|mac| self.turn_off(mac, now))
            .collect()
    }

    pub fn stop(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let changes = self.lifecycle.stop(mac, now);
        self.apply(changes, now)
    }

    pub fn start(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let changes = self.lifecycle.start(mac, now);
        self.apply(changes, now)
    }

    pub fn set_output_type(
        &mut self,
        mac: MacAddress,
        output_type: OutputType,
        now: Instant,
    ) -> Vec<Change> {
        let changes = self.lifecycle.set_output_type(mac, output_type, now);
        self.apply(changes, now)
    }

    pub fn set_app_settings(&mut self, app: AppSettings) {
        self.lifecycle.settings_mut().set_app(app);
    }

    pub fn set_nickname(&mut self, mac: MacAddress, name: &str) {
        self.lifecycle.settings_mut().set_nickname(mac, name);
    }

    pub fn set_idle_time(&mut self, mac: MacAddress, idle_time: Option<IdleTime>) {
        self.lifecycle.settings_mut().set_idle_time(mac, idle_time);
        if let Some(slot) = self.slots.get_mut(&mac) {
            slot.idle.restart();
        }
    }

    pub fn change_preference(&mut self, mac: MacAddress, change: PreferenceChange) -> Preferences {
        let mut preferences = self.settings().controller(mac).preferences;
        preferences.change(change);
        self.lifecycle
            .settings_mut()
            .set_preferences(mac, preferences);
        if let Some(slot) = self.slots.get(&mac) {
            slot.preferences.set(preferences);
        }
        preferences
    }

    pub fn test_rumble(&self, mac: MacAddress, motor: Motor) {
        if let Some(bridge) = self.bridge_of(mac) {
            bridge.test_rumble(motor);
        }
    }

    pub fn end_rumble_test(&self, mac: MacAddress) {
        if let Some(bridge) = self.bridge_of(mac) {
            bridge.end_rumble_test();
        }
    }

    pub fn quit(&mut self, now: Instant) -> Vec<Change> {
        let changes = self.lifecycle.quit();
        let done = self.apply(changes, now);
        self.slots.clear();
        done
    }

    fn lost_read_link(
        &mut self,
        mac: MacAddress,
        mut controller: Plugged,
        now: Instant,
    ) -> Vec<Change> {
        let mut changes = self.lifecycle.link_lost(mac, now);
        if self.slot(mac).links.is_empty() {
            if controller.release().is_ok() {
                self.slot(mac).held = Some(controller);
            } else {
                let _ = controller.unplug();
                changes.extend(self.lifecycle.release_failed(mac, now));
            }
        } else {
            changes.extend(self.move_bridge(mac, controller, now));
        }
        changes
    }

    fn move_bridge(&mut self, mac: MacAddress, controller: Plugged, now: Instant) -> Vec<Change> {
        let Err(error) = self.bridge_to(mac, controller) else {
            return Vec::new();
        };
        self.slot(mac).error = Some(error);
        self.lifecycle.bridge_failed(mac, now)
    }

    fn follow_active_link(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let Some(slot) = self.slots.get(&mac) else {
            return Vec::new();
        };
        if !slot.reads_inactive_link() {
            return Vec::new();
        }
        let mut changes = self.stop_watch(mac, now);
        let slot = self.slots.entry(mac).or_default();
        if let Some(bridge) = slot.bridge.take() {
            match bridge.stop() {
                BridgeEnd::Stopped { link, controller } => {
                    slot.put_back(link);
                    changes.extend(self.move_bridge(mac, controller, now));
                }
                BridgeEnd::LinkLost { controller } => {
                    self.dropped.extend(slot.forget_read());
                    changes.extend(self.lost_read_link(mac, controller, now));
                }
                BridgeEnd::VirtualControllerFailed { link, error } => {
                    slot.put_back(link);
                    slot.error = Some(BridgeError::refused(&error));
                    changes.extend(self.lifecycle.bridge_failed(mac, now));
                }
            }
        }
        self.apply(changes, now)
    }

    fn stop_watch(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let Some(slot) = self.slots.get_mut(&mac) else {
            return Vec::new();
        };
        let Some(watch) = slot.watch.take() else {
            return Vec::new();
        };
        if let Some(link) = watch.stop() {
            slot.put_back(link);
            return Vec::new();
        }
        self.dropped.extend(slot.forget_read());
        let mut changes = Vec::new();
        if slot.links.is_empty() {
            changes.extend(self.lifecycle.bridge_failed(mac, now));
        }
        changes.extend(self.lifecycle.link_lost(mac, now));
        changes
    }

    fn apply(&mut self, changes: Vec<Change>, now: Instant) -> Vec<Change> {
        let mut queue = VecDeque::from(changes);
        let mut done = Vec::new();
        while let Some(change) = queue.pop_front() {
            done.push(change);
            match change {
                Change::Bridged { mac, output_type } => {
                    queue.extend(self.carry_out_bridge(mac, output_type, false, now));
                }
                Change::Held { .. } => {}
                Change::Returned { mac } => {
                    let output_type = self.settings().controller(mac).output_type;
                    queue.extend(self.carry_out_bridge(mac, output_type, true, now));
                }
                Change::Removed { mac, why } => queue.extend(self.remove(mac, why, now)),
            }
        }
        let listed: Vec<MacAddress> = self.lifecycle.controllers().map(|(mac, _)| mac).collect();
        self.slots.retain(|mac, _| listed.contains(mac));
        self.watch_idle_links();
        done
    }

    fn watch_idle_links(&mut self) {
        for slot in self.slots.values_mut() {
            if slot.watch.is_some() || slot.bridge.is_some() {
                continue;
            }
            let Some(link) = slot.take_active() else {
                continue;
            };
            match Watch::start_on(self.threads.as_ref(), link, Arc::clone(&slot.live)) {
                Ok(watch) => {
                    slot.watch = Some(watch);
                    slot.watch_error = None;
                }
                Err((error, link)) => {
                    slot.put_back(link);
                    slot.watch_error = Some(BridgeError::no_thread(&error));
                }
            }
        }
    }

    fn carry_out_bridge(
        &mut self,
        mac: MacAddress,
        output_type: OutputType,
        resume: bool,
        now: Instant,
    ) -> Vec<Change> {
        let mut changes = self.stop_watch(mac, now);
        if self.slot(mac).links.is_empty() {
            return changes;
        }
        if let Err(error) = self.bridge(mac, output_type, resume) {
            self.slot(mac).error = Some(error);
            changes.extend(self.lifecycle.bridge_failed(mac, now));
        }
        changes
    }

    fn bridge(
        &mut self,
        mac: MacAddress,
        output_type: OutputType,
        resume: bool,
    ) -> Result<(), BridgeError> {
        let held = if resume {
            self.slot(mac).held.take()
        } else {
            None
        };
        let controller = match held {
            Some(controller) => controller,
            None => {
                let bus = self.bus.as_ref().ok_or(BridgeError::NoBus)?;
                let plugged = bus
                    .plug(output_type)
                    .map_err(|error| BridgeError::refused(&error))?;
                let mut controller = Plugged::new(plugged);
                controller.report_to(Arc::clone(&self.failures));
                controller
            }
        };
        self.bridge_to(mac, controller)
    }

    fn bridge_to(&mut self, mac: MacAddress, mut controller: Plugged) -> Result<(), BridgeError> {
        let preferences = self.settings().controller(mac).preferences;
        let threads = Arc::clone(&self.threads);
        let slot = self.slot(mac);
        let link = slot
            .take_active()
            .expect("an unbridged Controller keeps its Active Link open");
        slot.preferences.set(preferences);
        controller.follow(Arc::clone(&slot.preferences));
        match Bridge::resume_on(threads.as_ref(), link, controller, Arc::clone(&slot.live)) {
            Ok(bridge) => {
                slot.bridge = Some(bridge);
                slot.error = None;
                slot.watch_error = None;
                Ok(())
            }
            Err(failed) => {
                slot.put_back(failed.link);
                slot.held = Some(failed.controller);
                Err(BridgeError::no_thread(&failed.error))
            }
        }
    }

    fn remove(&mut self, mac: MacAddress, why: Removal, now: Instant) -> Vec<Change> {
        let slot = self.slots.entry(mac).or_default();
        let mut link_lost = false;
        if let Some(bridge) = slot.bridge.take() {
            match bridge.stop() {
                BridgeEnd::Stopped { link, controller } => {
                    slot.put_back(link);
                    let _ = controller.unplug();
                }
                BridgeEnd::LinkLost { controller } => {
                    self.dropped.extend(slot.forget_read());
                    link_lost = true;
                    let _ = controller.unplug();
                }
                BridgeEnd::VirtualControllerFailed { link, .. } => slot.put_back(link),
            }
        }
        if let Some(controller) = slot.held.take() {
            let _ = controller.unplug();
        }
        if why == Removal::Quit {
            slot.watch = None;
            slot.links.clear();
            slot.reading = None;
        }
        if link_lost {
            self.lifecycle.link_lost(mac, now)
        } else {
            Vec::new()
        }
    }

    fn bridge_of(&self, mac: MacAddress) -> Option<&Bridge> {
        self.slots.get(&mac)?.bridge.as_ref()
    }

    fn slot(&mut self, mac: MacAddress) -> &mut Slot {
        self.slots.entry(mac).or_default()
    }
}

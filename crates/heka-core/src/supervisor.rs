use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use crate::bridge::{Bridge, BridgeEnd, LiveInput, OpenLink, Plugged};
use crate::lifecycle::{Change, Lifecycle, Phase, Removal};
use crate::link::{ControllerModel, LinkKind, LinkReader, active_link};
use crate::mac::MacAddress;
use crate::preferences::{LivePreferences, Preferences};
use crate::settings::Settings;
use crate::virtual_controller::{OutputType, VirtualBus};
use crate::watch::Watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkId(u64);

#[derive(Debug, Clone)]
pub struct ControllerView {
    pub mac: MacAddress,
    pub model: ControllerModel,
    pub phase: Phase,
    pub output_type: OutputType,
    pub nickname: Option<String>,
    pub preferences: Preferences,
    pub active_link: Option<LinkKind>,
    pub live: Arc<LiveInput>,
    pub error: Option<String>,
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
    error: Option<String>,
}

impl Slot {
    fn active(&self) -> Option<&LinkSlot> {
        let kinds: Vec<LinkKind> = self.links.iter().map(|link| link.kind).collect();
        active_link(&kinds).map(|index| &self.links[index])
    }

    fn active_id(&self) -> Option<LinkId> {
        self.active().map(|link| link.id)
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

    fn forget_read(&mut self) {
        if let Some(id) = self.reading.take() {
            self.links.retain(|link| link.id != id);
        }
    }
}

pub struct Supervisor {
    bus: Option<Box<dyn VirtualBus>>,
    lifecycle: Lifecycle,
    slots: HashMap<MacAddress, Slot>,
    next_link_id: u64,
}

impl Supervisor {
    pub fn new(bus: Box<dyn VirtualBus>, settings: Settings) -> Self {
        Self {
            bus: Some(bus),
            lifecycle: Lifecycle::new(settings),
            slots: HashMap::new(),
            next_link_id: 0,
        }
    }

    pub fn without_bus(settings: Settings) -> Self {
        Self {
            bus: None,
            lifecycle: Lifecycle::paused(settings),
            slots: HashMap::new(),
            next_link_id: 0,
        }
    }

    pub fn has_bus(&self) -> bool {
        self.bus.is_some()
    }

    pub fn connect_bus(&mut self, bus: Box<dyn VirtualBus>, now: Instant) -> Vec<Change> {
        if self.has_bus() {
            return Vec::new();
        }
        self.bus = Some(bus);
        let mut lost = Vec::new();
        for (&mac, slot) in &mut self.slots {
            if let Some(watch) = slot.watch.take() {
                match watch.stop() {
                    Some(link) => slot.put_back(link),
                    None => {
                        slot.forget_read();
                        lost.push(mac);
                    }
                }
            }
        }
        let mut changes = Vec::new();
        for mac in lost {
            changes.extend(self.lifecycle.link_lost(mac, now));
        }
        changes.extend(self.lifecycle.resume_bridging(now));
        self.apply(changes, now)
    }

    pub fn settings(&self) -> &Settings {
        self.lifecycle.settings()
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.lifecycle.next_deadline()
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
                    live: slot.map(|slot| Arc::clone(&slot.live)).unwrap_or_default(),
                    error: slot.and_then(|slot| slot.error.clone()),
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
        let Some((&mac, _)) = self
            .slots
            .iter()
            .find(|(_, slot)| slot.links.iter().any(|link| link.id == id))
        else {
            return Vec::new();
        };
        let slot = self.slots.get_mut(&mac).expect("found above");
        let mut moving = None;
        if slot.reading == Some(id) {
            slot.watch = None;
            if let Some(bridge) = slot.bridge.take() {
                match bridge.stop() {
                    BridgeEnd::Stopped { controller, .. } | BridgeEnd::LinkLost { controller } => {
                        moving = Some(controller)
                    }
                    BridgeEnd::VirtualControllerFailed { .. } => {}
                }
            }
            slot.reading = None;
        }
        slot.links.retain(|link| link.id != id);
        let changes = match moving {
            Some(controller) => self.lost_read_link(mac, controller, now),
            None => self.lifecycle.link_lost(mac, now),
        };
        self.apply(changes, now)
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
            let slot = self.slots.get_mut(&mac).expect("listed above");
            let bridge = slot.bridge.take().expect("finished above");
            let changes = match bridge.stop() {
                BridgeEnd::LinkLost { controller } => {
                    slot.forget_read();
                    self.lost_read_link(mac, controller, now)
                }
                BridgeEnd::VirtualControllerFailed { link, error } => {
                    slot.put_back(link);
                    slot.error = Some(error.to_string());
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
            let slot = self.slots.get_mut(&mac).expect("listed above");
            slot.watch = None;
            slot.forget_read();
            let changes = self.lifecycle.link_lost(mac, now);
            done.extend(self.apply(changes, now));
        }
        let changes = self.lifecycle.tick(now);
        done.extend(self.apply(changes, now));
        done
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

    pub fn set_nickname(&mut self, mac: MacAddress, name: &str) {
        self.lifecycle.settings_mut().set_nickname(mac, name);
    }

    pub fn set_preferences(&mut self, mac: MacAddress, preferences: Preferences) {
        self.lifecycle
            .settings_mut()
            .set_preferences(mac, preferences);
        if let Some(slot) = self.slots.get(&mac) {
            slot.preferences.set(preferences);
        }
    }

    pub fn quit(&mut self) -> Vec<Change> {
        let changes = self.lifecycle.quit();
        let done = self.apply(changes, Instant::now());
        self.slots.clear();
        done
    }

    fn lost_read_link(
        &mut self,
        mac: MacAddress,
        controller: Plugged,
        now: Instant,
    ) -> Vec<Change> {
        let mut changes = self.lifecycle.link_lost(mac, now);
        if self.slot(mac).links.is_empty() {
            self.slot(mac).held = Some(release(controller));
        } else if let Err(error) = self.bridge_to(mac, controller) {
            self.slot(mac).error = Some(error);
            changes.extend(self.lifecycle.bridge_failed(mac, now));
        }
        changes
    }

    fn follow_active_link(&mut self, mac: MacAddress, now: Instant) -> Vec<Change> {
        let Some(slot) = self.slots.get_mut(&mac) else {
            return Vec::new();
        };
        if !slot.reads_inactive_link() {
            return Vec::new();
        }
        let mut changes = Vec::new();
        if let Some(watch) = slot.watch.take() {
            match watch.stop() {
                Some(link) => slot.put_back(link),
                None => {
                    slot.forget_read();
                    changes.extend(self.lifecycle.link_lost(mac, now));
                }
            }
        }
        let slot = self.slot(mac);
        if let Some(bridge) = slot.bridge.take() {
            match bridge.stop() {
                BridgeEnd::Stopped { link, controller } => {
                    slot.put_back(link);
                    if let Err(error) = self.bridge_to(mac, controller) {
                        self.slot(mac).error = Some(error);
                        changes.extend(self.lifecycle.bridge_failed(mac, now));
                    }
                }
                BridgeEnd::LinkLost { controller } => {
                    slot.forget_read();
                    changes.extend(self.lost_read_link(mac, controller, now));
                }
                BridgeEnd::VirtualControllerFailed { link, error } => {
                    slot.put_back(link);
                    slot.error = Some(error.to_string());
                    changes.extend(self.lifecycle.bridge_failed(mac, now));
                }
            }
        }
        self.apply(changes, now)
    }

    fn apply(&mut self, changes: Vec<Change>, now: Instant) -> Vec<Change> {
        let mut queue = VecDeque::from(changes);
        let mut done = Vec::new();
        while let Some(change) = queue.pop_front() {
            done.push(change);
            match change {
                Change::Bridged { mac, output_type } => {
                    if let Err(error) = self.bridge(mac, output_type, false) {
                        self.slot(mac).error = Some(error);
                        queue.extend(self.lifecycle.bridge_failed(mac, now));
                    }
                }
                Change::Held { .. } => {}
                Change::Returned { mac } => {
                    let output_type = self.settings().controller(mac).output_type;
                    if let Err(error) = self.bridge(mac, output_type, true) {
                        self.slot(mac).error = Some(error);
                        queue.extend(self.lifecycle.bridge_failed(mac, now));
                    }
                }
                Change::Removed { mac, why } => queue.extend(self.remove(mac, why, now)),
            }
        }
        let listed: Vec<MacAddress> = self.lifecycle.controllers().map(|(mac, _)| mac).collect();
        self.slots.retain(|mac, _| listed.contains(mac));
        if !self.has_bus() {
            self.watch_idle_links();
        }
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
            match Watch::start(link, Arc::clone(&slot.live)) {
                Ok(watch) => slot.watch = Some(watch),
                Err(error) => slot.error = Some(format!("cannot start the input thread: {error}")),
            }
        }
    }

    fn bridge(
        &mut self,
        mac: MacAddress,
        output_type: OutputType,
        resume: bool,
    ) -> Result<(), String> {
        let ready = self
            .slot(mac)
            .active()
            .is_some_and(|link| link.open.is_some());
        if !ready {
            return Err("the Link was closed; reconnect the Controller".into());
        }
        let held = if resume {
            self.slot(mac).held.take()
        } else {
            None
        };
        let controller = match held {
            Some(controller) => controller,
            None => {
                let bus = self.bus.as_ref().ok_or("there is no bus")?;
                Plugged::new(bus.plug(output_type).map_err(|e| e.to_string())?)
            }
        };
        self.bridge_to(mac, controller)
    }

    fn bridge_to(&mut self, mac: MacAddress, mut controller: Plugged) -> Result<(), String> {
        let preferences = self.settings().controller(mac).preferences;
        let slot = self.slot(mac);
        let link = slot
            .take_active()
            .ok_or("the Link was closed; reconnect the Controller")?;
        slot.preferences.set(preferences);
        controller.follow(Arc::clone(&slot.preferences));
        let bridge = Bridge::resume(link, controller, Arc::clone(&slot.live))
            .map_err(|e| format!("cannot start the input thread: {e}"))?;
        slot.bridge = Some(bridge);
        slot.error = None;
        Ok(())
    }

    fn remove(&mut self, mac: MacAddress, why: Removal, now: Instant) -> Vec<Change> {
        let slot = self.slot(mac);
        let mut link_lost = false;
        if let Some(bridge) = slot.bridge.take() {
            match bridge.stop() {
                BridgeEnd::Stopped { link, controller } => {
                    slot.put_back(link);
                    let _ = controller.unplug();
                }
                BridgeEnd::LinkLost { controller } => {
                    slot.forget_read();
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
            slot.links.clear();
            slot.reading = None;
        }
        if link_lost {
            self.lifecycle.link_lost(mac, now)
        } else {
            Vec::new()
        }
    }

    fn slot(&mut self, mac: MacAddress) -> &mut Slot {
        self.slots.entry(mac).or_default()
    }
}

fn release(mut controller: Plugged) -> Plugged {
    let _ = controller.release();
    controller
}

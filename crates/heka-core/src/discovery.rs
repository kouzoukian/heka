use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::lifecycle::Change;
use crate::link::{ControllerModel, LinkKind, LinkProblem, LinkReader, ReportError};
use crate::mac::MacAddress;
use crate::supervisor::{LinkId, Supervisor};

pub const FIRST_RETRY_DELAY: Duration = Duration::from_millis(250);

pub const LONGEST_RETRY_DELAY: Duration = Duration::from_secs(5);

pub const STEADY_LINK_TIME: Duration = Duration::from_secs(10);

pub trait Finder {
    type Path: Clone + Ord;

    fn present(&mut self) -> Vec<Self::Path>;

    fn open(&mut self, path: &Self::Path) -> Result<FoundLink, OpenFailure>;
}

pub struct FoundLink {
    pub mac: MacAddress,
    pub reader: Box<dyn LinkReader>,
    pub problems: Vec<LinkProblem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    Hid(String),
    Address(ReportError),
    SerialNumber,
    NotAController,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unopened {
    pub model: ControllerModel,
    pub error: OpenError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenFailure {
    VirtualController,
    Refused(Unopened),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found<P> {
    Seen {
        mac: MacAddress,
        path: P,
        model: ControllerModel,
        link: LinkKind,
        problems: Vec<LinkProblem>,
    },
    Gone {
        mac: MacAddress,
        path: P,
    },
    Changes(Vec<Change>),
    CannotOpen(Unopened),
}

struct Known {
    id: LinkId,
    mac: MacAddress,
    since: Instant,
}

#[derive(Default)]
struct Trouble {
    failures: u32,
    retry_at: Option<Instant>,
    unopened: Option<Unopened>,
}

impl Trouble {
    fn waits(&self, now: Instant) -> bool {
        self.retry_at.is_some_and(|at| now < at)
    }
}

pub struct Discovery<P> {
    known: BTreeMap<P, Known>,
    virtual_controllers: BTreeSet<P>,
    troubled: BTreeMap<P, Trouble>,
}

impl<P> Default for Discovery<P> {
    fn default() -> Self {
        Self {
            known: BTreeMap::new(),
            virtual_controllers: BTreeSet::new(),
            troubled: BTreeMap::new(),
        }
    }
}

impl<P: Clone + Ord> Discovery<P> {
    pub fn unopened(&self) -> impl Iterator<Item = &Unopened> {
        self.troubled
            .values()
            .filter_map(|trouble| trouble.unopened.as_ref())
    }

    pub fn scan(
        &mut self,
        finder: &mut impl Finder<Path = P>,
        supervisor: &mut Supervisor,
        now: Instant,
    ) -> Vec<Found<P>> {
        let present = finder.present();
        self.virtual_controllers
            .retain(|path| present.contains(path));
        self.troubled.retain(|path, _| present.contains(path));
        let mut found = Vec::new();
        for id in supervisor.take_dropped_links() {
            self.reopen_later(id, &present, now, &mut found);
        }
        let gone: Vec<P> = self
            .known
            .keys()
            .filter(|path| !present.contains(path))
            .cloned()
            .collect();
        for path in gone {
            if let Some(Known { id, mac, .. }) = self.known.remove(&path) {
                found.push(Found::Gone { mac, path });
                found.push(Found::Changes(supervisor.link_gone(id, now)));
            }
        }
        for path in present {
            let skipped = self.known.contains_key(&path)
                || self.virtual_controllers.contains(&path)
                || self
                    .troubled
                    .get(&path)
                    .is_some_and(|trouble| trouble.waits(now));
            if !skipped {
                self.open(path, finder, supervisor, now, &mut found);
            }
        }
        found
    }

    fn reopen_later(
        &mut self,
        dropped: LinkId,
        present: &[P],
        now: Instant,
        found: &mut Vec<Found<P>>,
    ) {
        let Some(path) = self
            .known
            .iter()
            .find(|(_, known)| known.id == dropped)
            .map(|(path, _)| path.clone())
        else {
            return;
        };
        let Some(Known { mac, since, .. }) = self.known.remove(&path) else {
            return;
        };
        found.push(Found::Gone {
            mac,
            path: path.clone(),
        });
        if !present.contains(&path) {
            return;
        }
        let trouble = self.troubled.entry(path).or_default();
        if now.saturating_duration_since(since) >= STEADY_LINK_TIME {
            trouble.failures = 0;
        }
        trouble.retry_at = Some(now + delay_after(trouble.failures));
        trouble.failures += 1;
    }

    fn open(
        &mut self,
        path: P,
        finder: &mut impl Finder<Path = P>,
        supervisor: &mut Supervisor,
        now: Instant,
        found: &mut Vec<Found<P>>,
    ) {
        match finder.open(&path) {
            Ok(FoundLink {
                mac,
                reader,
                problems,
            }) => {
                if let Some(trouble) = self.troubled.get_mut(&path) {
                    trouble.unopened = None;
                }
                let format = reader.format();
                found.push(Found::Seen {
                    mac,
                    path: path.clone(),
                    model: format.model(),
                    link: format.kind(),
                    problems,
                });
                let (id, changes) = supervisor.link_arrived(mac, reader, now);
                let since = now;
                self.known.insert(path, Known { id, mac, since });
                found.push(Found::Changes(changes));
            }
            Err(OpenFailure::VirtualController) => {
                self.troubled.remove(&path);
                self.virtual_controllers.insert(path);
            }
            Err(OpenFailure::Refused(unopened)) => {
                let trouble = self.troubled.entry(path).or_default();
                trouble.failures += 1;
                trouble.retry_at = Some(now + delay_after(trouble.failures));
                if trouble.unopened.as_ref() != Some(&unopened) {
                    found.push(Found::CannotOpen(unopened.clone()));
                    trouble.unopened = Some(unopened);
                }
            }
        }
    }
}

fn delay_after(failures: u32) -> Duration {
    match failures.checked_sub(1) {
        None => Duration::ZERO,
        Some(doublings) => FIRST_RETRY_DELAY
            .saturating_mul(2u32.saturating_pow(doublings))
            .min(LONGEST_RETRY_DELAY),
    }
}

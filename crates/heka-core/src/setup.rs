use std::time::Instant;

use crate::hiding::{HidHideError, HidHideStatus};
use crate::lifecycle::Change;
use crate::supervisor::Supervisor;
use crate::virtual_controller::VirtualBus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusUnavailable {
    NotInstalled,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    ViGEmBus,
    HidHide,
}

impl Driver {
    pub fn installer_url(self) -> &'static str {
        match self {
            Self::ViGEmBus => "https://github.com/nefarius/ViGEmBus/releases/latest",
            Self::HidHide => "https://github.com/nefarius/HidHide/releases/latest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverError {
    Bus(String),
    HidHide(HidHideError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    pub driver: Driver,
    pub error: Option<DriverError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    vigem_bus: Option<BusUnavailable>,
    hidhide: HidHideStatus,
}

impl Setup {
    pub fn missing(&self) -> Vec<Missing> {
        let vigem_bus = self.vigem_bus.as_ref().map(|why| Missing {
            driver: Driver::ViGEmBus,
            error: match why {
                BusUnavailable::NotInstalled => None,
                BusUnavailable::Failed(message) => Some(DriverError::Bus(message.clone())),
            },
        });
        let hidhide = match &self.hidhide {
            HidHideStatus::Ready | HidHideStatus::InUse => None,
            HidHideStatus::NotInstalled => Some(None),
            HidHideStatus::Failed(error) => Some(Some(DriverError::HidHide(error.clone()))),
        }
        .map(|error| Missing {
            driver: Driver::HidHide,
            error,
        });
        vigem_bus.into_iter().chain(hidhide).collect()
    }

    pub fn is_missing(&self, driver: Driver) -> bool {
        self.missing()
            .iter()
            .any(|missing| missing.driver == driver)
    }
}

pub trait Drivers {
    fn open_vigem_bus(&self) -> Result<Box<dyn VirtualBus>, BusUnavailable>;

    fn hidhide(&self) -> HidHideStatus;
}

pub fn check(
    drivers: &dyn Drivers,
    supervisor: &mut Supervisor,
    now: Instant,
) -> (Setup, Vec<Change>) {
    let mut changes = Vec::new();
    let mut vigem_bus = None;
    if !supervisor.has_bus() {
        match drivers.open_vigem_bus() {
            Ok(bus) => changes = supervisor.connect_bus(bus, now),
            Err(why) => vigem_bus = Some(why),
        }
    }
    let setup = Setup {
        vigem_bus,
        hidhide: drivers.hidhide(),
    };
    (setup, changes)
}

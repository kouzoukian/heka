//! Finding the drivers heka needs.
//!
//! Detection goes through [`Drivers`], so that any combination of results can be
//! injected. Nothing here downloads, installs or runs anything.

use std::time::Instant;

use crate::hiding::HidHideStatus;
use crate::lifecycle::Change;
use crate::supervisor::Supervisor;
use crate::virtual_controller::VirtualBus;

/// Why ViGEmBus cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusUnavailable {
    /// The driver is not installed or not running.
    NotInstalled,
    /// Any other failure, with the system's message.
    Failed(String),
}

/// A driver heka needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    /// Makes Virtual Controllers. Without it nothing is Bridged.
    ViGEmBus,
    /// Hides Controllers. Without it Bridges work, but games may see a Controller twice.
    HidHide,
}

impl Driver {
    /// Where its publisher offers the official installer. heka only links there.
    pub fn installer_url(self) -> &'static str {
        match self {
            Self::ViGEmBus => "https://github.com/nefarius/ViGEmBus/releases/latest",
            Self::HidHide => "https://github.com/nefarius/HidHide/releases/latest",
        }
    }
}

/// One driver the setup screen has to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    pub driver: Driver,
    /// The system's message when the driver may be there but cannot be used.
    pub error: Option<String>,
}

/// What the last [`check`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    vigem_bus: Option<BusUnavailable>,
    hidhide: HidHideStatus,
}

impl Setup {
    /// The drivers that cannot be used, ViGEmBus first. A HidHide that another
    /// program is using is installed, so it is not listed.
    pub fn missing(&self) -> Vec<Missing> {
        let vigem_bus = self.vigem_bus.as_ref().map(|why| Missing {
            driver: Driver::ViGEmBus,
            error: match why {
                BusUnavailable::NotInstalled => None,
                BusUnavailable::Failed(message) => Some(message.clone()),
            },
        });
        let hidhide = match &self.hidhide {
            HidHideStatus::Ready | HidHideStatus::InUse => None,
            HidHideStatus::NotInstalled => Some(None),
            HidHideStatus::Failed(message) => Some(Some(message.clone())),
        }
        .map(|error| Missing {
            driver: Driver::HidHide,
            error,
        });
        vigem_bus.into_iter().chain(hidhide).collect()
    }

    /// Whether this driver is listed in [`Self::missing`].
    pub fn is_missing(&self, driver: Driver) -> bool {
        self.missing()
            .iter()
            .any(|missing| missing.driver == driver)
    }
}

/// How the drivers heka needs are found. Implemented in `heka-windows`.
pub trait Drivers {
    /// Opens ViGEmBus. Must see a driver installed since the last call, if the
    /// driver itself allows that without a restart.
    fn open_vigem_bus(&self) -> Result<Box<dyn VirtualBus>, BusUnavailable>;

    /// Detects HidHide without changing it.
    fn hidhide(&self) -> HidHideStatus;
}

/// Looks for the drivers: at start, and again whenever the player asks. A
/// ViGEmBus found for the first time is handed to the Supervisor, which Bridges
/// the Controllers it lists; the [`Change`]s that carried out come back too.
pub fn check(
    drivers: &dyn Drivers,
    supervisor: &mut Supervisor,
    now: Instant,
) -> (Setup, Vec<Change>) {
    let mut changes = Vec::new();
    let mut vigem_bus = None;
    // A bus already in use is not opened again.
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

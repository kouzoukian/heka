//! Finding the installed drivers, for [`heka_core::setup`]. It opens them and
//! reads, and never installs, downloads or configures anything.

use std::error::Error;

use heka_core::hiding::HidHideStatus;
use heka_core::setup::{BusUnavailable, Drivers};
use heka_core::virtual_controller::{VirtualBus, VirtualError};
use vigem_rust::{BusError, ClientError};

use crate::hidhide::HidHideDriver;
use crate::vigem::VigemBus;

/// The drivers on this PC.
#[derive(Debug, Default, Clone, Copy)]
pub struct InstalledDrivers;

impl Drivers for InstalledDrivers {
    /// Each call looks for ViGEmBus afresh, so one installed while heka runs is found.
    fn open_vigem_bus(&self) -> Result<Box<dyn VirtualBus>, BusUnavailable> {
        match VigemBus::connect() {
            Ok(bus) => Ok(Box::new(bus)),
            Err(error) => Err(why_unavailable(&error)),
        }
    }

    fn hidhide(&self) -> HidHideStatus {
        HidHideStatus::detect(&HidHideDriver)
    }
}

/// `vigem-rust` reports a driver that is missing or not running as
/// `BusError::BusNotFound` (`Client::connect` in its `client.rs`).
fn why_unavailable(error: &VirtualError) -> BusUnavailable {
    match error.source().and_then(|e| e.downcast_ref::<ClientError>()) {
        Some(ClientError::Bus(BusError::BusNotFound { .. })) => BusUnavailable::NotInstalled,
        _ => BusUnavailable::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn why(error: ClientError) -> BusUnavailable {
        why_unavailable(&VirtualError::new(error))
    }

    #[test]
    fn a_bus_that_is_not_found_is_not_installed() {
        assert_eq!(
            why(BusError::BusNotFound { source: None }.into()),
            BusUnavailable::NotInstalled
        );
    }

    #[test]
    fn any_other_failure_keeps_its_message() {
        let error: ClientError = BusError::VersionMismatch { requested: 1 }.into();
        let message = error.to_string();
        assert_eq!(why(error), BusUnavailable::Failed(message));
    }
}

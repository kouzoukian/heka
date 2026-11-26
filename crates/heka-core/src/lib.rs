//! Controller report parsing, translation, the Bridge state machine and settings.
//!
//! This crate makes no Windows calls and takes no Windows-only dependencies.

pub mod autostart;
pub mod bridge;
pub mod calibration;
pub mod crc;
pub mod dualsense;
pub mod dualshock4;
pub mod feedback;
pub mod hiding;
pub mod lifecycle;
pub mod link;
pub mod mac;
pub mod preferences;
pub mod ps4;
pub mod settings;
pub mod setup;
pub mod state;
pub mod supervisor;
pub mod virtual_controller;
pub mod watch;
pub mod xbox;

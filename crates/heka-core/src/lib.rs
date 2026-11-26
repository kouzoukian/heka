//! Controller report parsing, translation, the Bridge state machine and settings.
//!
//! This crate makes no Windows calls and takes no Windows-only dependencies.

pub mod bridge;
pub mod calibration;
pub mod dualsense;
pub mod feedback;
pub mod hiding;
pub mod lifecycle;
pub mod link;
pub mod mac;
pub mod ps4;
pub mod settings;
pub mod state;
pub mod supervisor;
pub mod virtual_controller;
pub mod xbox;

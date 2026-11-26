//! Controller report parsing, translation, the Bridge state machine and settings.
//!
//! This crate makes no Windows calls and takes no Windows-only dependencies.

pub mod autostart;
pub mod bridge;
pub mod dualsense;
pub mod feedback;
pub mod hiding;
pub mod lifecycle;
pub mod link;
pub mod mac;
pub mod settings;
pub mod state;
pub mod supervisor;
pub mod virtual_controller;
pub mod xbox;

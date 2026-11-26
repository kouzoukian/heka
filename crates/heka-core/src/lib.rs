//! Controller report parsing, translation, the Bridge state machine and settings.
//!
//! This crate makes no Windows calls and takes no Windows-only dependencies.

pub mod bridge;
pub mod dualsense;
pub mod hiding;
pub mod link;
pub mod state;
pub mod virtual_controller;
pub mod xbox;

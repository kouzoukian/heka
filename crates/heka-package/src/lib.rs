//! Builds heka's portable release zip (`cargo run -p heka-package`).

pub mod binary;
pub mod crates;
pub mod notices;
pub mod release;

/// Why the release cannot be packaged, as a message for whoever runs it.
pub type Error = Box<dyn std::error::Error>;

pub mod binary;
pub mod crates;
pub mod notices;
pub mod release;

pub type Error = Box<dyn std::error::Error>;

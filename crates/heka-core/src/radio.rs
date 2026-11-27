use crate::mac::MacAddress;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnOffError {
    NoRadio,
    Failed(String),
}

pub trait Radio: Send {
    fn end_link(&self, mac: MacAddress) -> Result<(), TurnOffError>;
}

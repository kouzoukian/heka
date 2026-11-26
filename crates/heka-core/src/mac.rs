//! MAC addresses: how heka tells Controllers apart, whatever their Link.

use std::fmt;
use std::str::FromStr;

/// A Controller's Bluetooth MAC address, in the order printed on its label.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MacAddress(pub [u8; 6]);

/// The vendor prefix ViGEmBus gives every virtual DualShock 4 it creates
/// (`GenerateRandomMacAddress` in ViGEmBus `sys/Ds4Pdo.cpp`: "C0:13:37").
const VIGEM_DS4_PREFIX: [u8; 3] = [0xC0, 0x13, 0x37];

impl MacAddress {
    /// True for a virtual DualShock 4 made by ViGEmBus, such as heka's own PS4
    /// Virtual Controllers. It carries Sony's IDs, so only its MAC address tells
    /// it apart from a real DualShock 4.
    pub fn is_virtual_controller(self) -> bool {
        self.0[..3] == VIGEM_DS4_PREFIX
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e, g] = self.0;
        write!(f, "{a:02X}:{b:02X}:{c:02X}:{d:02X}:{e:02X}:{g:02X}")
    }
}

impl fmt::Debug for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Text that is not six `:`-separated hex bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidMacAddress;

impl fmt::Display for InvalidMacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a MAC address")
    }
}

impl std::error::Error for InvalidMacAddress {}

impl FromStr for MacAddress {
    type Err = InvalidMacAddress;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; 6];
        let mut parts = text.split(':');
        for byte in &mut bytes {
            let part = parts.next().ok_or(InvalidMacAddress)?;
            if part.len() != 2 {
                return Err(InvalidMacAddress);
            }
            *byte = u8::from_str_radix(part, 16).map_err(|_| InvalidMacAddress)?;
        }
        if parts.next().is_some() {
            return Err(InvalidMacAddress);
        }
        Ok(Self(bytes))
    }
}

impl serde::Serialize for MacAddress {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for MacAddress {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

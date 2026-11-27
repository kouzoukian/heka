use std::io;
use std::mem::size_of;
use std::ptr;

use heka_core::mac::MacAddress;
use heka_core::radio::{Radio, TurnOffError};
use windows_sys::Win32::Devices::Bluetooth::{
    BLUETOOTH_FIND_RADIO_PARAMS, BluetoothFindFirstRadio, BluetoothFindNextRadio,
    BluetoothFindRadioClose,
};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::IO::DeviceIoControl;

const IOCTL_BTH_DISCONNECT_DEVICE: u32 = 0x0041_000C;

#[derive(Debug, Default, Clone, Copy)]
pub struct BluetoothRadios;

impl Radio for BluetoothRadios {
    fn end_link(&self, mac: MacAddress) -> Result<(), TurnOffError> {
        let params = BLUETOOTH_FIND_RADIO_PARAMS {
            dwSize: size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32,
        };
        let mut radio: HANDLE = ptr::null_mut();
        let search = unsafe { BluetoothFindFirstRadio(&params, &mut radio) };
        if search.is_null() {
            return Err(TurnOffError::NoRadio);
        }
        let address = bluetooth_address(mac);
        let result = loop {
            let ended = end_link_through(radio, address);
            unsafe { CloseHandle(radio) };
            if ended.is_ok() || unsafe { BluetoothFindNextRadio(search, &mut radio) } == 0 {
                break ended;
            }
        };
        unsafe { BluetoothFindRadioClose(search) };
        result.map_err(|error| TurnOffError::Failed(error.to_string()))
    }
}

fn end_link_through(radio: HANDLE, address: u64) -> io::Result<()> {
    let mut returned = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            radio,
            IOCTL_BTH_DISCONNECT_DEVICE,
            ptr::from_ref(&address).cast(),
            size_of::<u64>() as u32,
            ptr::null_mut(),
            0,
            &mut returned,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn bluetooth_address(mac: MacAddress) -> u64 {
    mac.0
        .iter()
        .fold(0, |address, &byte| (address << 8) | u64::from(byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bluetooth_address_reads_the_mac_address_as_printed() {
        let mac = MacAddress([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

        assert_eq!(bluetooth_address(mac), 0x0000_1122_3344_5566);
        assert_eq!(
            bluetooth_address(mac).to_le_bytes(),
            [0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0, 0]
        );
    }

    #[test]
    fn the_disconnect_code_matches_ctl_code_by_hand() {
        assert_eq!(IOCTL_BTH_DISCONNECT_DEVICE, (0x41 << 16) | (3 << 2));
    }
}

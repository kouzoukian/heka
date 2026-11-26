# DualShock 4 support is experimental, written from public sources

No DualShock 4 was available, so support for both hardware revisions is written from public sources, never tried on one, and labelled experimental in the window until #22 is done. Sources: Linux `drivers/hid/hid-playstation.c` at commit `4dd10fbbeb9c` (the `struct dualshock4_*` layouts, the `DS4_*` definitions, `dualshock4_parse_report`, `dualshock4_get_calibration_data`, `dualshock4_get_mac_address`), and SDL `SDL_hidapi_ps4.c` at commit `3ebae2a704f9` where named. Offsets are into the report as read from HID, report ID included, unless they are into `struct dualshock4_input_report_common` or `dualshock4_output_report_common`.

## Identity

- Product IDs `0x05C4` (first revision, also what ViGEmBus's virtual DualShock 4 uses) and `0x09CC` (second). The Sony wireless USB adaptor is out of scope.
- Over USB the MAC address is in pairing-info feature report `0x12`, 16 bytes. Over Bluetooth it is the HID serial number, 12 hex digits in printed order, as the kernel and SDL (`HIDAPI_DriverPS4_InitDevice`) take it. All zeros is no address. In the USB report the address follows the report ID least significant byte first (SDL `ReadWiredSerial`).

## Input

- USB report `0x01`, 64 bytes: the common block from byte 1, the touch report count at 33, then up to 3 touch reports of 9 bytes (a timestamp byte and two 4-byte points). Bluetooth report `0x11`, 78 bytes: the common block from byte 3, the count at 35, up to 4 touch reports, and a CRC. Anything after the report, such as the padding Windows adds, is ignored.
- Buttons are packed as on a DualSense. The bit a DualSense uses for mute is part of a DualShock 4's report counter, so mute is cleared.
- The fingers come from the newest touch report, the last one counted, as the kernel applies them in order. With none counted the first is read, as SDL does whatever the count.
- One DualShock 4 sensor tick is 16/3 µs and heka's state tick is 1/3 µs, so the 16-bit timestamp is shifted left by 4.
- `status[0]`: capacity in bits 0..=3, cable connected in bit 4. Without a cable, capacity 0 means 0..=9%, so the middle of each step is shown, and anything past 9 reads as full. With one, 11 is full (`DS4_BATTERY_STATUS_FULL`), 14 a voltage or temperature error, 15 a charge error, and the rest are undefined.

## Feature reports

Reading them sends nothing to the Controller but the request.

- USB calibration `0x02`, 37 bytes, gyro plus and minus values in pairs per axis.
- Bluetooth calibration `0x05`, 41 bytes ending in a CRC that `ps_get_report` checks: the three gyro plus values come before the three minus values. Reading it also switches the DualShock 4 from its short input report `0x01` to the full one (SDL: into "advanced report mode"). If it cannot be asked for, opening the Link fails and is tried again, since heka does not read the short report. Calibration that cannot be used reads at full scale.
- Feature reports are read into a 64-byte buffer, as long as the one SDL passes and longer than any of them.

## Output

- USB report `0x05`, 32 bytes, the common block from byte 1. Bluetooth report `0x11`, 78 bytes: `hw_control` at 1 and `audio_control` at 2, the common block from byte 3, and a CRC.
- `hw_control` says the report carries HID data and ends in a CRC, and its low bits give the milliseconds between input reports. Every report states 4 (`DS4_BT_DEFAULT_POLL_INTERVAL_MS`), as SDL's do, because 0 there means 1 ms.
- Offsets: over USB `valid_flag0` 1, `motor_right` 4, `motor_left` 5, then red, green, blue; over Bluetooth 3, 6, 7, then the colour.
- Only rumble, and the lightbar when the Feedback has a colour, are set. The large (Xbox left) motor drives the left motor (`dualshock4_play_effect`).

## Source names

| heka (`dualshock4.rs`) | Source |
| --- | --- |
| `PRODUCT_ID_DUALSHOCK4`, `_V2` | `USB_DEVICE_ID_SONY_PS4_CONTROLLER`, `_2` in `drivers/hid/hid-ids.h` (the DualSense IDs are its `USB_DEVICE_ID_SONY_PS5_*`) |
| `USB_INPUT_REPORT_ID`, `_LEN` | `DS4_INPUT_REPORT_USB`, `DS4_INPUT_REPORT_USB_SIZE` |
| `BT_INPUT_REPORT_ID`, `_LEN` | `DS4_INPUT_REPORT_BT`, `DS4_INPUT_REPORT_BT_SIZE`; the short report is `DS4_INPUT_REPORT_BT_MINIMAL` |
| `USB_OUTPUT_REPORT_ID`, `_LEN` | `DS4_OUTPUT_REPORT_USB`, `DS4_OUTPUT_REPORT_USB_SIZE` |
| `BT_OUTPUT_REPORT_ID`, `_LEN` | `DS4_OUTPUT_REPORT_BT`, `DS4_OUTPUT_REPORT_BT_SIZE` |
| `HW_CONTROL_HID_AND_CRC` | `DS4_OUTPUT_HWCTL_HID`, `DS4_OUTPUT_HWCTL_CRC32` |
| `PAIRING_INFO_REPORT_ID`, `_LEN` | `DS4_FEATURE_REPORT_PAIRING_INFO`, `DS4_FEATURE_REPORT_PAIRING_INFO_SIZE` |
| `USB_CALIBRATION_REPORT_ID`, `_LEN` | `DS4_FEATURE_REPORT_CALIBRATION`, `DS4_FEATURE_REPORT_CALIBRATION_SIZE` |
| `BT_CALIBRATION_REPORT_ID`, `_LEN` | `DS4_FEATURE_REPORT_CALIBRATION_BT`, `DS4_FEATURE_REPORT_CALIBRATION_BT_SIZE` |
| `DUALSHOCK4_FEATURE_BUFFER_LEN` (`heka-windows`) | SDL's `USB_PACKET_LENGTH` |

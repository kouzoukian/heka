# DualSense reports follow Linux `hid-playstation.c`

Sony publishes no layout. DualSense and DualSense Edge reports are read and built as Linux `drivers/hid/hid-playstation.c` does (`struct dualsense_input_report`, `struct dualsense_output_report_common`, `_usb` and `_bt`, and the `DS_*` definitions), and [Kernel names](#kernel-names) maps heka's constants to the kernel's so they can be checked against it. USB product IDs are `0x0CE6` (DualSense) and `0x0DF2` (Edge), vendor `0x054C`.

## Input

- Offsets in `dualsense.rs` are into the USB report as read from HID, one more than the struct offsets because byte 0 is the report ID (the kernel reads the struct from `&data[1]`). The kernel's fields map as: `x`, `y`, `rx`, `ry` to the two sticks, `z` and `rz` to the left and right triggers; byte 7 is `seq_number`; then `buttons[0..3]`, `gyro[3]` and `accel[3]` (le16), `sensor_timestamp` (le32), two touch points of 4 bytes and `status[0]` at byte 53.
- `buttons[0]` holds the d-pad as a hat in its low nibble (`DS_BUTTONS0_HAT_SWITCH`), clockwise from 0 = up; any value past 7 means released (`ps_gamepad_hat_mapping`). Button bits are the `DS_BUTTONS0_*` to `DS_BUTTONS2_*` definitions, shared with the DualShock 4.
- A touch point is contact, x low, x high nibble and y low nibble, y high (`struct dualsense_touch_point`). Bit 7 of the contact byte means no finger (`DS_TOUCH_POINT_INACTIVE`).
- `status[0]`: capacity in the low nibble, each unit 10%, read as `min(capacity * 10 + 5, 100)`; charging state in the high nibble, where 0 is discharging, 1 charging, 2 full, `0xA` and `0xB` mean voltage or temperature out of range or a temperature error, and `0xF` a charging error.
- Over Bluetooth the full report is `0x31`, 78 bytes with ID and CRC, and the struct starts at byte 2. It is read by copying it into a USB-shaped report, so whatever the USB parser reads a Bluetooth Link reads too. Until full reports are enabled the DualSense sends a minimal report `0x01` over Bluetooth.

## Feature reports

Reading them sends nothing to the Controller but the request.

- Pairing info, `0x09`, 20 bytes over USB (over Bluetooth it also carries a CRC): the MAC address is 6 bytes from byte 1, least significant first, the reverse of the order printed on the label (`dualsense_get_mac_address`). A DualShock 4's USB pairing report puts it in the same place.
- Calibration, `0x05`, 41 bytes; see [ADR 0008](0008-controller-state-and-calibration.md).
- Firmware info, `0x20`, 64 bytes: the little-endian update version at bytes 44..46.

## Output

- USB report `0x02`, 63 bytes, the common block from byte 1. Bluetooth report `0x31`, 78 bytes: `seq_tag` at byte 1 with the sequence number in bits 4..8, the tag `0x10` (`DS_OUTPUT_TAG`) at byte 2, the 47-byte common block from byte 3, and a CRC. The sequence number runs 0..=15 and must increase with every report.
- The Bluetooth common block is the USB one, so whatever a USB Link sets a Bluetooth Link sets too.
- Only rumble, and the lightbar and player lights when the Feedback has them, are set. Nothing else on the Controller changes. The large (Xbox left) motor drives the left motor, as the kernel maps a force-feedback effect's strong magnitude.
- Offsets into the USB report: `valid_flag0` 1, `valid_flag1` 2, `motor_right` 3, `motor_left` 4, `valid_flag2` 39, `player_leds` 44, lightbar red, green, blue 45 to 47. Over Bluetooth the common block starts two bytes later; the setup report sets the setup bit of `valid_flag2` at byte 41 and `lightbar_setup` at byte 44.
- `Compatible` rumble sets `valid_flag0` to compatible vibration and haptics select; `CompatibleV2` sets haptics select there and compatible vibration 2 in `valid_flag2`.
- Player lights are five bits from bit 0 on the left (`player_ids` in `dualsense_set_player_leds`).
- The default lightbar colour is the blue `dualsense_create` gives a DualSense when nothing chooses one.
- Over USB the lightbar takes a colour at once. Over Bluetooth the DualSense keeps control of its lightbar until a setup report fades its own colour out (`dualsense_reset_leds`, `DS_OUTPUT_LIGHTBAR_SETUP_LIGHT_OUT`), so that report precedes the first colour and changes nothing else. Resending it after a failed write is owed: #45.

## Rumble mode

A DualSense imitates two-motor rumble with haptic actuators in one of two ways. `Compatible` is accepted by every firmware. `CompatibleV2` feels like classic rumble, is what the kernel notes newer firmware made the default, and exists from update version 2.21 (`0x0215`) and on every Edge. The mode is chosen as `dualsense_create` does; a DualSense whose firmware info cannot be read gets `Compatible`.

## Bluetooth CRC

DualSense and DualShock 4 Bluetooth reports end in a CRC-32 (IEEE, reflected, polynomial `0xEDB88320`), little-endian, over a one-byte seed followed by the report with its ID and without the 4 CRC bytes. The seed is the HIDP header of the transaction: `0xA1` input, `0xA2` output, `0xA3` feature. Sources: `ps_check_crc32`, `dualsense_send_output_report`, and SDL `SDL_hidapi_ps5.c` (`VerifyCRC`).

## Kernel names

| heka (`dualsense.rs`) | `hid-playstation.c` |
| --- | --- |
| `USB_INPUT_REPORT_ID`, `_LEN` | `DS_INPUT_REPORT_USB`, `DS_INPUT_REPORT_USB_SIZE` |
| `BT_INPUT_REPORT_ID`, `_LEN` | `DS_INPUT_REPORT_BT`, `DS_INPUT_REPORT_BT_SIZE` |
| `USB_OUTPUT_REPORT_ID`, `_LEN` | `DS_OUTPUT_REPORT_USB`, `DS_OUTPUT_REPORT_USB_SIZE` |
| `BT_OUTPUT_REPORT_ID`, `_LEN` | `DS_OUTPUT_REPORT_BT`, `DS_OUTPUT_REPORT_BT_SIZE` |
| `PAIRING_INFO_REPORT_ID`, `_LEN` | `DS_FEATURE_REPORT_PAIRING_INFO`, `DS_FEATURE_REPORT_PAIRING_INFO_SIZE` |
| `CALIBRATION_REPORT_ID`, `_LEN` | `DS_FEATURE_REPORT_CALIBRATION`, `DS_FEATURE_REPORT_CALIBRATION_SIZE` |
| `FIRMWARE_INFO_REPORT_ID`, `_LEN` | `DS_FEATURE_REPORT_FIRMWARE_INFO`, `DS_FEATURE_REPORT_FIRMWARE_INFO_SIZE` |
| `UPDATE_VERSION_V2_RUMBLE` | `DS_FEATURE_VERSION(2, 21)` |
| `OUT_BT_SEQ_TAG` | `DS_OUTPUT_SEQ_NO` |
| `FLAG*` | `DS_OUTPUT_VALID_FLAG*`, such as `DS_OUTPUT_VALID_FLAG1_LIGHTBAR_CONTROL_ENABLE` and `DS_OUTPUT_VALID_FLAG2_LIGHTBAR_SETUP_CONTROL_ENABLE` |
| `touch_point` | `DS_TOUCH_POINT_X`, `DS_TOUCH_POINT_Y` |

The output report follows `dualsense_init_output_report` and `dualsense_output_worker`, and input parsing `dualsense_parse_report`.

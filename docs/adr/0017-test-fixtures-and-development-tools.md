# Test fixtures, expected values and development tools

## Captured and derived reports

- **Captured** fixtures are raw bytes read from a real Controller, one report per line in hex, under `crates/heka-core/tests/fixtures`. **Derived** reports are a captured report, or a layout from Linux `hid-playstation.c`, with bytes changed at documented offsets. Test helpers say which is which by name (`captured_*`, `derived`, `ds4_*`, `bluetooth_from_usb`).
- No real Controller's MAC address is stored in the repository.
- Only a resting DualSense over USB is captured, so button reports and every Bluetooth report are derived (#57). No DualShock 4 was available, so all its reports are derived (#22).

## Fixture provenance

| File | What it holds |
| --- | --- |
| `dualsense_usb_idle.hex` | 40 consecutive USB input reports of a resting DualSense (`054c:0ce6`, interface 3), captured 2026-11-26 with `capture_dualsense`: every button and trigger released, sticks resting. The first starts `01 7e 7d 7f 7c`, and byte 53 is `0x29`, charging state full. |
| `dualsense_usb_calibration.hex` | Calibration feature report `0x05` of the same DualSense. |
| `dualsense_usb_firmware_info.hex` | Firmware info feature report `0x20` of a real DualSense over USB, read with `capture_dualsense`; bytes 44..46 hold update version `0x0630`. |

## Derived DualShock 4 reports

- USB: report `0x01`, 64 bytes; sticks at 1..=4, buttons at 5..=7 (hat 8 = released), triggers at 8 and 9, touch report count at 33, 9-byte touch reports from 34. The resting report has one touch report with both fingers lifted. Fingers of the three touch reports start at 35, 44 and 53.
- Bluetooth: report `0x11`, 78 bytes; the common block from byte 3, count at 35, four touch reports from 36 (the fourth, whose first finger is at 64, lifted), CRC seeded `0xA1`.
- In the USB report, motion fields: timestamp at 10, gyro from 13, accelerometer from 19; `status[0]` at 30.

## Where expected values come from

Expected values are worked out independently of the code under test.

- Output report bytes are written out by hand from the kernel's structs and `DS_OUTPUT_*` / `DS4_OUTPUT_*` bits.
- CRCs come from Python's `zlib.crc32` (for example `zlib.crc32(bytes([0xA2])) == 0xEADA2D49`, and Bluetooth output as `zlib.crc32(bytes([0xA2]) + report[:74])`), the CRC-32/ISO-HDLC check value of `123456789`, and a bit-at-a-time CRC in the test helpers that does not share code with `heka_core::crc`.
- DualSense USB input offsets: byte 8 is `buttons[0]` (hat; Square, Cross, Circle, Triangle in bits 4..7), byte 9 L1, R1, L2, R2, Create, Options, L3, R3 in bits 0..7, byte 10 PS, touchpad and mute in bits 0..2. `status[0]` is struct offset 52, byte 53. The Xbox mapping tests press one bit at a time from these.
- Calibration reports are written as the kernel reads them: biases, plus/minus values, speeds, then accelerometer pairs (DualSense y has a bias of 100). Expected motion is computed from the formula, for example DualSense pitch 1000 × 1024 × 1600 / 16000 = 102400, and accelerometer 2 × 8192 × (raw − bias) / (plus − minus); DualShock 4 gyro raw × 1000 × 1024 / (plus − minus), accelerometer (raw − middle) × 16384 / (plus − minus).
- The virtual DualShock 4's calibration is ViGEmBus's (`sys/Ds4Pdo.cpp`, `HID_REPORT_ID_1`). Accelerometer x: plus 7807, minus −8402, range 16209, bias −297, so one g is raw 7808. One raw step is about 63 gyro units and 1 accelerometer unit, so a round trip costs under 1.5 steps.
- ViGEmBus's virtual DualShock 4 takes a MAC address under `C0:13:37` (`GenerateRandomMacAddress`) and answers the pairing report with it byte-reversed from byte 1.
- Further vectors: `zlib.crc32(bytes([0xA1, 0x31, 0x00, 0x7F])) == 0x31804276`; the CRC is IEEE 802.3's. The Bridge tests' Link writer reports `CompatibleV2`, so their expected bytes are the v2 USB report checked in `dualsense_feedback.rs`. ViGEmBus's pairing answer is its `HID_REPORT_MAC_ADDRESSES_ID` template.
- Touch: x 1919 is `0x77F`, y 1079 is `0x437`, and rescaled to the DualShock 4 y 941 is `0x3AD`; x 960 is `0x3C0`, and y 540 becomes 540 × 941 / 1079 = 470.9, so 471 (`0x1D7`). DualShock 4 input fingers are at bytes 35 and 39.
- Virtual calibration, accelerometer x: plus `0x1E7F`, minus `0xDF2E`, bias 7807 − 8104, and one g is 8192 × 16209 / 16384 − 297 = 7807.5, rounded to 7808.
- PE fixtures put the fields of Microsoft's PE format (`NumberOfSections`, `NumberOfRvaAndSizes`, `VirtualSize`, `SizeOfRawData`) at their offsets; the name address is byte 12 of an import descriptor and byte 4 of a delay-load one.

## Fakes

Tests use fakes at the `heka-core` seams, a fake clock (`Instant`s passed in), and fake driver, journal and Run entry. The HidHide fakes model two real set-ups: a PC with other tools' entries and cloaking on, and one where DS4Windows left the DualSense hidden and is the only allowed application.

## Development tools

- `cargo run -p heka-app --example ui_preview [-- --empty] [--missing] [--first N] [--settings] [--log] [--seconds N]`: the window over made-up data, touching no device or driver. `--empty` shows no Controller, `--missing` the setup screen for both drivers, `--first N` selects the Nth Controller, `--settings` opens its settings, `--log` the log with made-up entries, `--seconds N` closes by itself. It prints the sampling rate every 2 seconds.
- `cargo run -p heka-app --example tray_spike`: proves a `tray-icon` tray works in GPUI's event loop, ending within seconds and touching no device, driver or registry. It opens a window with `QuitMode::Explicit`, closes it, posts the tray's own left-click message (class `tray_icon_app`, message 6002, from `tray-icon` 0.25.1 `src/platform_impl/windows/mod.rs`) to the tray's hidden window, and reopens on the click.
- `cargo run -p heka-windows --example capture_dualsense -- [COUNT] > reports.hex`: read-only. Lists DualSense collections, MAC address, firmware info, Feedback format and calibration on stderr, then writes COUNT reports (default 50, for at most 30 seconds) in fixture format. Hold the inputs to record while it runs.
- `cargo run -p heka-windows --example read_hidhide`: prints HidHide's configuration and what heka would hide, changing nothing.

# One Controller state for every model and Link

Every input report, whatever its model or Link, is parsed into one `ControllerState`, in DualSense terms.

- Button names follow the DualSense; a DualShock 4's Share is `CREATE`. The d-pad is four buttons, so a diagonal is two. L2 and R2 are digital clicks beside the triggers' travel, which runs 0 (released) to 255.
- Stick axes run 0 to 255, left to right and top to bottom.
- Touch uses DualSense touchpad units, x 0..=1919 and y 0..=1079 (`DS_TOUCHPAD_*`). Both pads are 1920 wide; a DualShock 4's is 942 high (`DS4_TOUCHPAD_HEIGHT`) and its y is rescaled with rounding, so that going from the shorter pad to the taller one and back gives the same y. A finger's contact number, 0..=127, stays the same while it stays down.
- Battery is 0..=100 in the Controller's steps of 10%, with a charge state that includes plugged in but not charging, and unknown, both of which come with a level of 0 that means nothing. A report may carry none.
- Motion is calibrated, in the units the kernel normalises both pads to: gyro in 1/1024 degree per second around x (pitch), y (yaw), z (roll), and acceleration in 1/8192 g along x, y, z. Timestamps are ticks of 1/3 µs, the DualSense's sensor clock, wrapping; a DualShock 4's 16-bit clock counts 16 of them per tick, so its timestamps wrap at 2^20.

## Calibration

Source: `dualsense_get_calibration_data`, `dualshock4_get_calibration_data` and the input report handlers in Linux `hid-playstation.c`.

- The resolutions are `DS_GYRO_RES_PER_DEG_S` and `DS_ACC_RES_PER_G`; touchpad sizes are `DS_TOUCHPAD_WIDTH` and `DS_TOUCHPAD_HEIGHT`. The virtual pad's paired layout is the `BUS_USB` branch of `dualshock4_get_calibration_data`.
- A calibrated value is `numer * (raw - bias) / denom`, truncating like the kernel's `mult_frac`. As in the kernel, the gyro biases only shape the sensitivity and are not subtracted from readings.
- The block is 17 little-endian `i16` fields starting after the report ID. The paired layout (DualSense `0x05`, DualShock 4 USB `0x02`) is: pitch, yaw, roll bias; pitch plus and minus, yaw plus and minus, roll plus and minus; speed plus and minus; then accelerometer x, y, z, each plus and minus. The DualShock 4's Bluetooth report groups the three gyro plus values before the three minus values.
- Axes whose data is unusable, and Controllers whose report cannot be read, get the kernel's full-scale fallback: the raw `i16` range spans 2048 degrees per second (`DS_GYRO_RANGE`) or 4 g (`DS_ACC_RANGE`).
- Calibration can be inverted, from calibrated units to the raw reading nearest to them (halves away from zero, clamped to `i16`), which the PS4 Virtual Controller needs ([ADR 0009](0009-translation-to-output-types.md)).

## Links

A Link is read through the `LinkReader` seam and written through `LinkWriter`, both implemented over HID in `heka-windows`. A read waits a short, bounded time so the caller can notice a stop request between reads, and ends as a report, a timeout (the Link is still up) or lost. Which `ReportFormat` and `FeedbackFormat` a Link has is fixed when it is opened; the former carries the Controller's calibration. A Controller with several Links reads the first USB Link, otherwise the first to arrive. A virtual DualShock 4 carries real DualShock 4 IDs, so only its MAC address tells it apart.

A MAC address is how heka tells Controllers apart whatever their Link, kept in the order printed on the label.

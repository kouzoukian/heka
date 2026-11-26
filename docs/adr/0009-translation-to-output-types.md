# Translation to Output Types

`heka-core` decides what a Virtual Controller shows; the backend only moves reports ([ADR 0001](0001-virtual-output-through-vigembus.md)).

## Xbox

- Button bits are the XInput `XINPUT_GAMEPAD_*` values (Microsoft's `XINPUT_GAMEPAD`), plus Guide from ViGEmBus's `XUSB_GAMEPAD_GUIDE`. `heka-core`'s `XboxReport` is laid out like `XUSB_REPORT` and uses the same bit values as `vigem-rust`'s `X360Button`. Start is labelled Menu on newer pads, Back is labelled View.
- Touchpad click, mute and the L2 and R2 clicks drive nothing: the triggers' travel already carries L2 and R2.
- Stick axes scale 0..=255 (centre 128) to -32768..=32767 so that 128 lands exactly on 0 and both ends reach the full range. Y is flipped, because it grows downward on the Controller and upward on Xbox.

## PS4

- The report is the DualShock 4 USB input report without its report ID (`struct dualshock4_input_report_usb`: common block, touch report count, three touch reports, three reserved bytes), 63 bytes, the same as ViGEmBus's `DS4_REPORT_EX`.
- Offsets: sticks 0..4, buttons 4..7, triggers 7 and 8, sensor timestamp (le16) 9, gyro 12, accelerometer 18, touch report count 32, first touch report (packet counter, then two points) 33.
- Button bits are the `DS_BUTTONS*` definitions; the DualShock 4 has no mute button. The hat is 0 = up clockwise, 8 = released, and opposite directions pressed together cancel out.
- A touch point is packed as contact, x low, `x_hi:4|y_lo:4`, y high, with bit 7 of the contact byte meaning no finger. The translator remembers the last touch so that the touch packet counter moves only when the touch changes; it is therefore one per Bridge.
- The timestamp is the state's shifted right by 4 (16 DualSense ticks of 1/3 µs make one DualShock 4 tick).
- Games calibrate a virtual pad's motion with the calibration ViGEmBus answers for feature report `0x02` (copied from `sys/Ds4Pdo.cpp`). Motion is written as the raw readings that this calibration, in the USB paired layout, turns back into the Controller's calibrated motion.

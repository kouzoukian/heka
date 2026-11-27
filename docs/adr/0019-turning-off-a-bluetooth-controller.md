# Turning off ends the Bluetooth Link, by button or when idle

## The call

A Controller is turned off by ending its Bluetooth Link with `IOCTL_BTH_DISCONNECT_DEVICE` (`0x0041000C`, in `bthioctl.h`), sent to a Bluetooth radio with the Controller's address as a 64-bit number, its MAC address read as printed. Other controller tools use the same call. It was tried on 2026-11-26 on a DualSense, from a process that was not elevated: it succeeded and the Controller left the system's device list. For an address that is not connected it fails with error 1167. It has not been tried on a DualShock 4.

Each radio is asked until one succeeds. The core reaches it through the `Radio` trait.

## What happens to the Bridge

Turning off is the player's choice, or the one they made for an idle Controller, so a Controller that loses its only Link this way is not Held: its Virtual Controller is removed at once, and Hiding restores it as for any removal. A Controller that still has a USB Link carries on over it. A failure changes nothing and is shown in the window until another attempt or until the Bluetooth Link is gone.

Windows may still list the HID path for a moment after the call. The Link is reported apart from dropped ones, and its path is left alone for 5 seconds, so that a Controller on its way out is not opened, Bridged and then Held. A path that leaves the list sooner is opened as soon as it returns.

## Idle time

- Off by default; 5, 10, 15 or 30 minutes, remembered per Controller beside the Output Type. It is not part of the Preferences, which shape Feedback. A saved value that is none of these loads as off, keeping the rest of the Controller's settings.
- Use is a change in a button, a stick or trigger by more than 8 of 255, or a finger landing, lifting or moving by more than 16 touchpad units. Each is measured from the state that last counted, so a slow movement counts too. Motion and battery never count.
- The thread that reads a Link counts use report by report, so a tap shorter than the supervisor's 250 ms wake is not missed. The Supervisor compares the count at each poll and owns the clock.
- The time runs only while the Link being read is Bluetooth, whether the Controller is Bridged, waiting for a free Bridge or Stopped. It starts over when USB takes over, when another idle time is chosen and when it runs out, so a failed idle attempt is made again after another whole idle time. A failed press of the button leaves it running.

## Unsafe code

Each `unsafe` call is sound because: `BluetoothFindFirstRadio` gets a parameter block that states its own size and a handle to write, both alive for the call; `BluetoothFindNextRadio` and `BluetoothFindRadioClose` get the search that call returned, closed once; each radio handle is closed once, after its one use; `DeviceIoControl` is synchronous, reads 8 bytes from an address that outlives it and has no output buffer.

# Windows integration

`heka-core` makes no Windows calls and takes no Windows-only dependencies; `heka-windows` and `heka-app` hold them.

## Opening Controllers

- Controllers are found as gamepad collections on the HID Generic Desktop usage page (HID Usage Tables 1.4), on USB or Bluetooth, in enumeration order, virtual DualShock 4s among them.
- Opening a Link asks for feature reports and nothing else: the MAC address first (a DualShock 4 on Bluetooth gives it as its serial number), then motion calibration, and for a DualSense the firmware info. Output reports are sent only by the Bridge's Feedback thread, when Feedback or the player's lights call for it.
- A Virtual Controller is recognised by its MAC address, and nothing more is asked of it; it is never opened for Feedback.
- Over Bluetooth these reads also switch a Controller to its full input report (SDL: reading the serial number or firmware version "will also enable enhanced reports over Bluetooth").
- A device is opened twice so that Feedback is written from another thread than the one reading input (`HidDevice` is not `Sync`). If the second open fails, the Link carries input only.
- A read waits 100 ms. Output reports go through `WriteFile`, which `hidapi` waits up to a second for. `hidapi` reports an unplugged device as a read error, which is a lost Link. A passing error looks the same, so a lost Link that is still listed is opened again.

## ViGEmBus

- Dropping the bus connection removes every Virtual Controller still plugged in.
- `vigem-rust` reports a missing or stopped driver as `BusError::BusNotFound` (`Client::connect` in its `client.rs`). Each driver check looks afresh.
- A PS4 notification carries rumble and lightbar together, so the lightbar shows whatever the game last asked for. The default Xbox subscription keeps a few notifications and drops the oldest, which suits rumble. A disconnect or bus error, which `vigem-rust` documents as often terminal, closes the Feedback.

## Start with Windows

The setting is the Run entry itself (`HKCU\...\Run`, value `heka`), so nothing else has to be kept in step: heka starts with Windows exactly when the entry is there. It runs heka with `--tray`. heka is a portable zip that may be moved, so at start an existing entry is pointed at the running copy; this never turns the setting on. An entry that cannot be read counts as off, and removing a missing one succeeds. The byte count the registry returns includes the terminating null. The registry code is never called by tests.

## One copy per session

The first copy creates the auto-reset event `Local\heka.show-window` and waits on it on its own thread. A later copy finds it, passes on its right to take the foreground (it was just started by the player), signals the event so that the running copy shows its window, and ends. The event handle may be sent between threads because it is only waited on and closed, which any thread may do.

## Tray and process lifetime

- Release builds open no console window; debug builds keep it for messages.
- Closing the window leaves heka in the tray; only Quit ends it, removing every Virtual Controller and restoring every Hidden Controller first. If the tray icon cannot be added, for example before the taskbar runs, closing the window quits and the window is shown.
- GPUI has no tray API. `tray-icon` creates a hidden window on the GPUI thread, so GPUI's message loop dispatches its messages (`examples/tray_spike.rs` proves it). Its events, and requests from a second copy, are drained every 100 ms. Left click opens the window; the menu is on the right button. The tray icon is gone before the app quits, so it never lingers.
- The tray icon is a placeholder drawn in code, to avoid an image decoder, until #21.

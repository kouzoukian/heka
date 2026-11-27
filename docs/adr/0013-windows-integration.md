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

The setting is the Run entry itself (`HKCU\...\Run`, value `heka`), so nothing else has to be kept in step: heka starts with Windows exactly when the entry is there. The window reads it with every sample and the tray menu's check follows it with every poll, so neither holds a copy. It runs heka with `--tray`. heka is a portable zip that may be moved, so at start an existing entry is pointed at the running copy; this never turns the setting on. An entry that cannot be read counts as off, and removing a missing one succeeds. The byte count the registry returns includes the terminating null. The registry code is never called by tests.

## One copy per session

The first copy creates the auto-reset event `Local\heka.show-window` and waits on it on its own thread. A later copy finds it, passes on its right to take the foreground (it was just started by the player), signals the event so that the running copy shows its window, and ends. The event handle may be sent between threads because it is only waited on and closed, which any thread may do.

## Tray and process lifetime

- Release builds open no console window; debug builds keep it for messages.
- Closing the window leaves heka in the tray, unless the player chose that it quits. Quitting, from the tray menu or by closing the window, removes every Virtual Controller and restores every Hidden Controller first. Either way it goes through the tray's task, so the tray icon is gone first.
- heka opens no window when started with `--tray`, or when the player chose to start in the tray. A second launch still opens it.
- If the tray icon cannot be added, for example before the taskbar runs, closing the window quits and the window is shown.
- GPUI has no tray API. `tray-icon` creates a hidden window on the GPUI thread, so GPUI's message loop dispatches its messages (`examples/tray_spike.rs` proves it). Its events, and requests from a second copy, are drained every 100 ms. Left click opens the window; the menu is on the right button. The tray icon is gone before the app quits, so it never lingers.

## Icon and version information

`heka.exe` carries the icon as resource 1 and a `VERSIONINFO` block, compiled by `heka-app`'s build script with the Windows SDK's `rc.exe` (through `embed-resource`). The product name is heka; the version and the license line come from the workspace manifest, so a release needs no second place edited. Only the `heka` binary is given them, not the examples.

- GPUI loads resource 1 as the window's icon (`load_icon` in `gpui-pre-windows` 0.3.14), which the taskbar shows too.
- The tray loads the same resource at the size `GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem())` gives, so Windows picks a drawing made for the PC's scaling instead of shrinking the 32 px one. A change of scaling while heka runs is not followed.
- `crates/heka-app/icon/draw.py` (Python with Pillow) draws `heka.ico`, and `heka.png` for the About view; all three are committed, so building needs no Python. The icon has a drawing for each small and large icon size from 100% to 200% scaling (16 to 64 px) and some larger ones, with the d-pad and the top of the body on whole pixels. Sizes below 256 px are stored as bitmaps and 256 px as PNG, the usual form of a Windows icon.

The two metric calls take no pointers, which is what makes that `unsafe` block sound.

## Windows' view of the window

`IsIconic` and `MonitorFromWindow` fail harmlessly for a handle that names no window, so any value is sound to pass. The monitor's handle is GPUI's display id.

## The program in front

Game rules ([ADR 0021](0021-profiles-and-the-mapper.md)) need to know which program is in front and which are running. `heka-windows` answers both and takes no decisions; `heka-core`'s `RuleSelector` decides. The supervisor thread asks about twice a second, and only while there is a rule or a rule still applies, so heka does nothing with processes unless the player has made a rule.

- The program in front is the window `GetForegroundWindow` names, the process that owns it by `GetWindowThreadProcessId`, and that process's executable by `QueryFullProcessImageNameW`, reduced to its file name. A window of heka's own process is its own, whatever that window is. No window, or one that is gone before it is asked about, is nothing.
- The process is opened with `PROCESS_QUERY_LIMITED_INFORMATION` only, the right Windows grants for reading a name even of an elevated program, so rules work for programs run as administrator in the usual case. Some processes still refuse, a protected one for example: the program is then unreadable, no rule applies, and the window says that a program heka cannot see exists. This has not been tried on a Windows machine.
- Whether a program runs is a snapshot of the process list (`CreateToolhelp32Snapshot`), compared by file name. It is taken only when a rule whose Profile sets an Output Type would otherwise end, so most polls never take one.
- The programs the player can choose from are the processes that own a visible top-level window, found with `EnumWindows`, without heka's own, by file name and without repeats.

What makes each `unsafe` block sound: `GetForegroundWindow` and `GetCurrentProcessId` take nothing. `GetWindowThreadProcessId` is given a handle that may have stopped naming a window, which it answers with a process of 0, and it writes to a local `u32`. A handle from `OpenProcess` is checked for null and owned by a type that closes it. `QueryFullProcessImageNameW` is given a buffer of 1024 units and told so, and on success reports a length within it, so the slice that is read is in bounds. A snapshot handle is checked against `INVALID_HANDLE_VALUE` and closed by the same type, `dwSize` is set before the first call, and `szExeFile` is a null-terminated array that is read up to its terminator. `EnumWindows` calls its callback on the calling thread before it returns, so the pointer to a local `Vec<u32>` that is passed through it is valid and unaliased for every call.

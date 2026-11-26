# Hiding undoes only what heka did, through a journal

HidHide keeps a cloak switch, a list of hidden devices (by device instance path) and a list of allowed applications (by full image name). Other tools share that configuration, so heka only ever undoes what it did itself. Behaviour and its sources are described in [`docs/hidhide.md`](../hidhide.md).

## Rules

- Every change is written to a journal before it is made, and the journal is saved durably first. If it cannot be saved or read, nothing is changed. Undoing happens in reverse order, and each change is undone before it leaves the journal, so a crash in between leaves an entry whose undo finds nothing to do.
- A device or application already listed, and cloaking already on, belong to the player or another tool: they are not journalled and never removed.
- The journal is a small text file at `%LOCALAPPDATA%\heka\hiding.journal`, written aside and renamed over, and removed when empty.
- heka allows itself before it hides a device, so it never loses sight of a Controller. With the inverse application list, listing heka would deny it, so it stays off the list (warning about that is #17).
- Device instance paths compare without regard to case, as HidHide compares them.
- Each Controller is a hiding owner, keyed by MAC address, and each of its Links is hidden. A Link another owner still wants stays hidden. When no owner wants anything, everything heka changed is undone, except that heka stays allowed until `restore_all`.

## Order at start

`recover` undoes whatever a killed run left, then `allow_heka`, both before looking for Controllers: a Controller another tool hid cannot be found until heka is allowed. heka stays allowed until it quits, and removes itself only if it added itself.

## In the app

- A Controller is Hidden while Bridged or Held and restored when its Virtual Controller is removed. Every Link is hidden, including one that arrives later; every Link is tried, so one that cannot be hidden does not leave the others visible. A Link's device is recorded before the Supervisor hears of it.
- A Controller the player lets games see is never hidden, and is restored as soon as they say so.
- Failed hides and a failed self-allow are retried every poll, for example while HidHide's own client holds the driver; Bridging goes on meanwhile. Retrying failed restores is #17.
- Without HidHide every hiding failure follows from that and the setup screen already says so, so no separate note is shown.

## Driver interface

Names and codes come from nefarius/HidHide `v1.2.128.0`: the control device `\\.\HidHide`, the IOCTL codes and buffer shapes in `FilterDriverProxy.cpp` checked against `Logic.c`. Custom device type 32769 (`IoControlDeviceType`), function codes from 2048, read access, buffered; codes are built as `CTL_CODE` (`devioctl.h`) does.

- The control device takes one client at a time, so heka opens it for one operation and closes it. Access denied on open means in use, as HidHide's own client reports it. It is opened as `HidHideCLI` does: read access, fully shared.
- Lists are `REG_MULTI_SZ`, asked once for their size in bytes and then read. Each setter replaces the whole list, and an empty input clears it.
- heka's allowed-application name is its native image name (`\Device\HarddiskVolumeN\...\heka.exe`), the form the driver matches at process load (`OnSystemLoadImage` in `Logic.c`).
- What is hidden is the device instance path of the HID collection behind a `hidapi` path. Instance IDs are at most 200 characters (`MAX_DEVICE_ID_LEN`).

## Unsafe code

Each `unsafe` call is sound because: the device name passed to `CreateFileW` is NUL-terminated and outlives the call; `DeviceIoControl` gets buffers valid for the lengths passed and is synchronous; the handle came from `CreateFileW` and is closed once; `QueryFullProcessImageNameW` gets the buffer's length in characters; the device interface property query gets a NUL-terminated interface name and the buffer's size in bytes.

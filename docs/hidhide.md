# Hiding Controllers with HidHide

How heka Hides a Controller, what the sources say about HidHide's behaviour, and how to
verify the parts nobody has verified yet (ticket #5).

Sources are nefarius/HidHide at tag `v1.2.128.0` (the version installed on the development
PC), MIT licensed. Paths below are relative to that tag:
`https://github.com/nefarius/HidHide/tree/v1.2.128.0/<path>`. The control device, IOCTL
codes and `FilterDriverProxy.cpp` are unchanged in `v1.5.230.0` (the latest release); only
the driver's blacklist check gained a session ID argument (`HidHide/src/Logic.h`).

## How heka uses HidHide

- **Interface.** heka opens `\\.\HidHide` (`HidHideCLI/HidHideCLI.rc`,
  `IDS_CONTROL_DEVICE_NAME`) and uses the eight buffered IOCTLs in
  `HidHideCLI/src/FilterDriverProxy.cpp`: get/set the allowed applications (whitelist),
  hidden devices (blacklist), cloak state (active) and inverse flag. Lists are
  REG_MULTI_SZ; each setter replaces the whole list. heka never sets the inverse flag.
- **One client at a time.** The control device is exclusive
  (`WdfDeviceInitSetExclusive(..., TRUE)` in `HidHide/src/ControlDevice.c`). heka opens
  it for a single read or write and closes it at once, so it never blocks HidHide's own
  tools for longer than one call.
- **What is hidden.** The device instance path of the Controller's HID collection, e.g.
  `HID\VID_054C&PID_0CE6&MI_03\8&15C30336&0&0000`. HidHide's own client hides the same
  thing per HID collection (`HidHideClient/src/BlacklistDlg.cpp`). The driver compares
  paths without regard to case (`Blacklisted` in `HidHide/src/Logic.c`), and so does heka.
- **heka itself is allowed** by its native image name
  (`\Device\HarddiskVolumeN\...\heka.exe`, from `QueryFullProcessImageNameW` with
  `PROCESS_NAME_NATIVE`). The driver matches allowed applications against the image name
  the kernel reports at process load, without regard to case
  (`HidHideProcessIdCheckFullImageNameAgainstWhitelist` in `HidHide/src/Config.c`).
  With the inverse flag on, the list names denied applications, so heka does not list
  itself then.
- **Order.** Hiding: allow heka, hide the device, turn cloaking on. Restoring runs in
  reverse. Each change is written to the journal
  (`%LOCALAPPDATA%\heka\hiding.journal`) before it is made, and removed from the journal
  after it is undone. The next start undoes whatever the journal still lists.
- **Only heka's entries.** A device or application that is already listed, and cloaking
  that is already on, are not journalled, so heka never removes or turns them off.
  Limitation: if the player lists the same device or heka by hand while heka has it
  journalled, heka removes it on restore, because the two cannot be told apart.

The logic lives in `crates/heka-core/src/hiding.rs` (unit tested in
`crates/heka-core/tests/hiding.rs` with a fake driver), the driver adapter in
`crates/heka-windows/src/hidhide.rs`.

## Findings

### Does changing HidHide's configuration need elevation?

**Sources say no.** Not verified by a write.

- `README.md`: the configuration utility "runs in the least privileged mode and doesn't
  require elevated rights."
- `HidHide/src/ControlDevice.c` creates the control device with
  `SDDL_DEVOBJ_SYS_ALL_ADM_RWX_WORLD_RWX_RES_RWX`, which grants everyone read, write and
  execute. All IOCTLs, setters included, require only `FILE_READ_DATA`
  (`FilterDriverProxy.cpp`). The driver writes its registry key itself, in kernel mode
  (`HidHideDriverSetMultiStringProperty` in `HidHide/src/Config.c`).
- Observed on the development PC from an unelevated process: opening `\\.\HidHide` and
  the four GET IOCTLs work (`cargo run -p heka-windows --example read_hidhide`). The
  registry key `HKLM\SYSTEM\CurrentControlSet\Services\HidHide\Parameters` is **not**
  readable unelevated ("Requested registry access is not allowed"), so heka must use the
  IOCTLs and never the registry.

### Does a game that already opened the Controller keep access?

**Sources say yes.** Not verified with a game.

- The driver decides only when a handle is opened: `OnDeviceFileCreate` in
  `HidHide/src/Logic.c` denies the create with `STATUS_ACCESS_DENIED`. Nothing revokes
  handles that are already open, so a game that opened the Controller before it was
  Hidden keeps reading it until it closes the handle.
- What heka should do: heka cannot take the handle away. It should tell the player that
  a game started before the Controller was Hidden may still see it twice, and that
  restarting the game fixes it. Replugging the Controller also works once ticket #6
  lands: the Controller stays Hidden while Held, so the game's new open is denied.
  Forcing a device restart (`CM_Query_And_Remove_SubTree` / disable and enable) would
  also cut the game off, but needs elevation and is out of scope.

### What error does heka see when HidHide's own client holds the driver?

**Sources say `ERROR_ACCESS_DENIED` on open.** Not verified.

- The control device is exclusive (see above), so a second `CreateFile` fails while
  HidHide's Configuration Client or HidHideCLI has it open. HidHide's client itself treats
  `ERROR_ACCESS_DENIED` on open as "in use" and tells the user to close the command line
  interface or other applications (`FilterDriverProxy::DeviceStatus` and
  `HidHideClient/src/HidHideClient.cpp`, string `IDS_STATIC_MESSAGEBOX_IN_USE` in
  `HidHideClient/HidHideClient.rc`).
- heka maps it to `HidHideError::InUse`, shown in the window as: "HidHide is busy: close
  the HidHide Configuration Client or HidHideCLI so heka can hide the Controller. Games may
  see the Controller twice." heka retries every poll and the message clears once hiding
  succeeds. `ERROR_FILE_NOT_FOUND` means the driver is not installed or not running.

## Do not use HidHideCLI to look

`HidHideCLI.exe` adds itself to the allowed applications every time it runs, whatever the
command, even `--help`: the constructor of `FilterDriverProxy` writes the whitelist when
the CLI is missing from it (`HidHideCLI/src/FilterDriverProxy.cpp`). On the development PC
it is already listed, so running it there changes nothing, but on a clean machine it does.
Use `cargo run -p heka-windows --example read_hidhide` instead: it only reads.

Configuration on the development PC on 2026-11-26, read with that example:
cloaking on, inverse off, no hidden devices, allowed: `HidHideCLI.exe`,
`HidHideClient.exe`, `DS4Windows.exe`, `x360ce.exe`. The DualSense on USB resolves to
`HID\VID_054C&PID_0CE6&MI_03\8&15c30336&0&0000`.

## Verifying by hand

Do this on a machine where changing HidHide is acceptable. Close DS4Windows, x360ce,
Steam and the HidHide Configuration Client first.

1. **Back up.** `cargo run -p heka-windows --example read_hidhide > before.txt`. Also take
   a screenshot of both tabs of the HidHide Configuration Client, then close it.
2. **Elevation.** From a normal (not administrator) terminal, `cargo run -p heka-app`.
   Connect the DualSense by USB. The window shows "Bridged" and no warning line.
   Run the example again: the DualSense's `HID\...` path is listed as hidden, heka's
   `heka.exe` is listed as allowed, cloaking is on. If so, no elevation is needed.
3. **Only the Virtual Controller is seen.** With heka running, open
   `joy.cpl` (Game Controllers): only the Xbox controller is listed. heka's window still
   shows live input when you press buttons.
4. **A game that already had it open.** Quit heka. Open
   <https://hardwaretester.com/gamepad> in a browser (it should show the DualSense), then
   start heka. Note whether the browser keeps showing the DualSense (expected: yes, until
   the browser is restarted). Restart the browser: only the Xbox controller remains.
5. **Restore on Bridge end.** Pull the USB cable. Run the example: the configuration
   matches `before.txt` again.
6. **Restore on quit.** Plug back in, wait for "Bridged", close heka from the window.
   Run the example: matches `before.txt`.
7. **Restore after a kill.** Plug in, wait for "Bridged", then
   `taskkill /F /IM heka.exe`. Run the example: the DualSense is still hidden and
   `%LOCALAPPDATA%\heka\hiding.journal` exists. Start heka with the Controller unplugged:
   the example matches `before.txt` and the journal file is gone.
8. **HidHide busy.** Open the HidHide Configuration Client, then start heka and plug in.
   The window shows the "HidHide is busy" line. Close the client: within a second the line
   disappears and the Controller is Hidden. If a different message appears, record it here.
9. **Other tools survive.** Compare the final example output with `before.txt`:
   DS4Windows and x360ce are still allowed and cloaking is in the state it was before.

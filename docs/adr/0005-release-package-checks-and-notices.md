# Release package checks and third-party notices

`cargo run -p heka-package` builds the release zip in `target/dist`: one folder with `heka.exe`, the README, both of heka's licenses and `THIRD-PARTY-NOTICES.txt`. It builds `heka.exe` and reads it, but never runs it. `--tag v1.2.3` also requires the tag to be `v<workspace version>`, so that the zip's name matches what is inside. Cargo's progress and errors go straight to the terminal.

The zip is written by the `tar` that comes with Windows, picking the format from the file name (`-a`). Another `tar` found on the path may not write zip files.

## What heka.exe may import

`heka.exe` must open no console window and import only DLLs that every Windows 10 and 11 PC has. It is read as a PE32+ image, with offsets from Microsoft's [PE Format](https://learn.microsoft.com/windows/win32/debug/pe-format): both the import table and the delay-load table are arrays of descriptors that end with an empty one, whose name address is zero. A windowless program has the subsystem `IMAGE_SUBSYSTEM_WINDOWS_GUI`.

- Add a DLL to `WINDOWS_DLLS` only if every Windows 10 and 11 PC has it. The Visual C++ runtime is not one of them.
- API sets (`api-ms-win-*`) are part of Windows, except the C runtime's (`api-ms-win-crt-*`), whose presence means the runtime was not linked in.
- `icuuc.dll` came with Windows 10 version 1703, the newest version any listed DLL needs.
- `gdiplus.dll` draws the icons of the menus `gpui-component` opens on text fields.

## Which crates are noticed

The notices cover every third-party crate that `heka-app` depends on, directly or not, in `cargo metadata` filtered to the target platform. Build dependencies count, because what they generate can end up in the binary. Dev dependencies do not.

## Which texts are printed

- Every license and notice file a crate ships is printed, each distinct text once with the files it came from. Files whose name starts like a license or a notice count, and so does everything in a directory of licenses such as `LICENSES/` or `.licenses/`.
- Source and asset files named like a license file are skipped, as are directories never built into a binary (tests, benches, examples, scripts).
- Only a license at the top of a crate covers the crate itself. A crate without one gets the standard Apache-2.0 or MIT text, taken from heka's own license files, if its SPDX expression offers one (`A OR B`, or the older `A/B`; anything with `AND` needs both texts). Otherwise packaging fails.
- heka takes HIDAPI under its BSD-style license, so `hidapi`'s `LICENSE-gpl3.txt` is left out.
- The notices also carry the built-in theme's and font's licenses, the Rust standard library's notices, and the Chromium code inside AccessKit.

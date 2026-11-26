//! Checks heka.exe before it goes into the release zip: it must open no console
//! window, and import only DLLs that Windows comes with.
//!
//! Offsets are from Microsoft's "PE Format":
//! <https://learn.microsoft.com/windows/win32/debug/pe-format>.

use crate::Error;

/// `IMAGE_SUBSYSTEM_WINDOWS_GUI`.
const GUI: u16 = 2;
/// The optional header magic of a 64-bit (PE32+) image.
const PE32_PLUS: u16 = 0x20b;
/// Data directory indexes.
const IMPORT_TABLE: usize = 1;
const DELAY_IMPORT_TABLE: usize = 13;

/// The DLLs of Windows 10 and 11 that heka.exe may import. The Visual C++ runtime
/// is not one of them. Add a DLL only if every Windows 10 and 11 PC has it.
/// `icuuc.dll` came with Windows 10 version 1703, the newest of these.
const WINDOWS_DLLS: [&str; 23] = [
    "advapi32.dll",
    "bcryptprimitives.dll",
    "cfgmgr32.dll",
    "combase.dll",
    "comctl32.dll",
    "d3d11.dll",
    "dcomp.dll",
    "dwmapi.dll",
    "dwrite.dll",
    "dxgi.dll",
    "gdi32.dll",
    "icuuc.dll",
    "imm32.dll",
    "kernel32.dll",
    "ntdll.dll",
    "ole32.dll",
    "oleaut32.dll",
    "setupapi.dll",
    "shell32.dll",
    "uiautomationcore.dll",
    "user32.dll",
    "userenv.dll",
    "winmm.dll",
];

/// Checks the executable and returns the DLLs it imports, delay-loaded ones last.
pub fn check(exe: &[u8]) -> Result<Vec<String>, Error> {
    let image = Image::read(exe).ok_or("not a 64-bit Windows executable, or a damaged one")?;
    if image.subsystem != GUI {
        return Err(
            "the executable opens a console window: build it in release mode, with the \
             windows subsystem"
                .into(),
        );
    }
    let foreign: Vec<&str> = image
        .imports
        .iter()
        .map(String::as_str)
        .filter(|dll| !comes_with_windows(dll))
        .collect();
    if !foreign.is_empty() {
        return Err(format!(
            "the executable needs DLLs that a PC may not have: {}",
            foreign.join(", ")
        )
        .into());
    }
    Ok(image.imports)
}

fn comes_with_windows(dll: &str) -> bool {
    let dll = dll.to_ascii_lowercase();
    // API sets are part of Windows, except the C runtime's, whose presence means
    // the runtime was not linked in.
    (dll.starts_with("api-ms-win-") && !dll.starts_with("api-ms-win-crt-"))
        || WINDOWS_DLLS.contains(&dll.as_str())
}

struct Image {
    subsystem: u16,
    imports: Vec<String>,
}

/// A section's place in memory and in the file.
struct Section {
    address: u32,
    len: u32,
    offset: u32,
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

impl Image {
    /// `None` for anything that is not a whole PE32+ image.
    fn read(exe: &[u8]) -> Option<Self> {
        if !exe.starts_with(b"MZ") {
            return None;
        }
        let pe = u32_at(exe, 0x3c)? as usize;
        if exe.get(pe..pe.checked_add(4)?)? != b"PE\0\0" {
            return None;
        }
        let section_count = u16_at(exe, pe + 6)?;
        let optional_len = usize::from(u16_at(exe, pe + 20)?);
        let optional = pe + 24;
        if u16_at(exe, optional)? != PE32_PLUS {
            return None;
        }
        let subsystem = u16_at(exe, optional + 68)?;
        let directory_count = u32_at(exe, optional + 108)? as usize;
        let sections = (0..usize::from(section_count))
            .map(|index| {
                let header = optional + optional_len + index * 40;
                Some(Section {
                    address: u32_at(exe, header + 12)?,
                    len: u32_at(exe, header + 16)?,
                    offset: u32_at(exe, header + 20)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        // Where in the file an address in memory comes from.
        let offset = |address: u32| {
            sections.iter().find_map(|section| {
                let within = address.checked_sub(section.address)?;
                (within < section.len).then(|| section.offset as usize + within as usize)
            })
        };

        let mut imports = Vec::new();
        // Each table is an array of descriptors that ends with an empty one, whose
        // name address is zero: (directory, descriptor length, name address at).
        for (directory, len, name_at) in [(IMPORT_TABLE, 20, 12), (DELAY_IMPORT_TABLE, 32, 4)] {
            if directory >= directory_count {
                continue;
            }
            let table = u32_at(exe, optional + 112 + directory * 8)?;
            if table == 0 {
                continue;
            }
            let mut descriptor = offset(table)?;
            loop {
                let name = u32_at(exe, descriptor + name_at)?;
                if name == 0 {
                    break;
                }
                let name = exe.get(offset(name)?..)?;
                let end = name.iter().position(|&byte| byte == 0)?;
                imports.push(String::from_utf8(name[..end].to_vec()).ok()?);
                descriptor += len;
            }
        }
        Some(Self { subsystem, imports })
    }
}

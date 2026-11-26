use heka_package::binary::check;

const GUI: u16 = 2;
const CONSOLE: u16 = 3;

fn executable(subsystem: u16, imports: &[&str], delay_imports: &[&str]) -> Vec<u8> {
    const PE: usize = 0x40;
    const OPTIONAL: usize = PE + 24;
    const OPTIONAL_LEN: usize = 240;
    const SECTION: usize = OPTIONAL + OPTIONAL_LEN;
    const RAW: usize = 0x200;
    const RVA: u32 = 0x1000;

    fn put(image: &mut [u8], at: usize, bytes: &[u8]) {
        image[at..at + bytes.len()].copy_from_slice(bytes);
    }

    let delay_table = (imports.len() + 1) * 20;
    let names = delay_table + (delay_imports.len() + 1) * 32;
    let mut data = vec![0; names];
    for (index, name) in imports.iter().chain(delay_imports).enumerate() {
        let rva = RVA + data.len() as u32;
        data.extend_from_slice(name.as_bytes());
        data.push(0);
        let at = match index.checked_sub(imports.len()) {
            None => index * 20 + 12,
            Some(delayed) => delay_table + delayed * 32 + 4,
        };
        put(&mut data, at, &rva.to_le_bytes());
    }

    let mut image = vec![0; RAW];
    put(&mut image, 0, b"MZ");
    put(&mut image, 0x3c, &(PE as u32).to_le_bytes());
    put(&mut image, PE, b"PE\0\0");
    put(&mut image, PE + 6, &1u16.to_le_bytes());
    put(&mut image, PE + 20, &(OPTIONAL_LEN as u16).to_le_bytes());
    put(&mut image, OPTIONAL, &0x20bu16.to_le_bytes());
    put(&mut image, OPTIONAL + 68, &subsystem.to_le_bytes());
    put(&mut image, OPTIONAL + 108, &16u32.to_le_bytes());
    let directory = OPTIONAL + 112;
    put(&mut image, directory + 8, &RVA.to_le_bytes());
    if !delay_imports.is_empty() {
        let rva = RVA + delay_table as u32;
        put(&mut image, directory + 13 * 8, &rva.to_le_bytes());
    }
    let len = (data.len() as u32).to_le_bytes();
    put(&mut image, SECTION + 8, &len);
    put(&mut image, SECTION + 12, &RVA.to_le_bytes());
    put(&mut image, SECTION + 16, &len);
    put(&mut image, SECTION + 20, &(RAW as u32).to_le_bytes());
    image.extend(data);
    image
}

#[test]
fn accepts_a_windowed_program_that_imports_only_windows_dlls() {
    let exe = executable(
        GUI,
        &[
            "KERNEL32.dll",
            "api-ms-win-core-synch-l1-2-0.dll",
            "gdiplus.dll",
        ],
        &["user32.dll"],
    );

    let imports = check(&exe).unwrap();

    assert_eq!(
        imports,
        [
            "KERNEL32.dll",
            "api-ms-win-core-synch-l1-2-0.dll",
            "gdiplus.dll",
            "user32.dll"
        ]
    );
}

#[test]
fn rejects_a_dll_that_windows_does_not_come_with() {
    for dll in [
        "VCRUNTIME140.dll",
        "MSVCP140.dll",
        "api-ms-win-crt-runtime-l1-1-0.dll",
        "ViGEmClient.dll",
    ] {
        for exe in [
            executable(GUI, &["KERNEL32.dll", dll], &[]),
            executable(GUI, &["KERNEL32.dll"], &[dll]),
        ] {
            let error = check(&exe).unwrap_err().to_string();

            assert!(error.contains(dll), "{error}");
            assert!(!error.contains("KERNEL32"), "{error}");
        }
    }
}

#[test]
fn rejects_a_program_that_opens_a_console_window() {
    let error = check(&executable(CONSOLE, &["KERNEL32.dll"], &[])).unwrap_err();

    assert!(error.to_string().contains("console"), "{error}");
}

#[test]
fn rejects_what_is_not_a_64_bit_executable() {
    let whole = executable(GUI, &["KERNEL32.dll"], &[]);
    for exe in [&b""[..], b"not an executable", &whole[..0x210]] {
        assert!(check(exe).is_err());
    }
}

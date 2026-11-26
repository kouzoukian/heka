use std::env;
use std::fs;
use std::path::PathBuf;

const ICON: &str = "icon/heka.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(variable("OUT_DIR")).join("heka.rc");
    let icon = PathBuf::from(variable("CARGO_MANIFEST_DIR")).join(ICON);
    fs::write(&out, resources(&icon.display().to_string())).expect("cannot write heka.rc");
    embed_resource::compile_for(&out, ["heka"], embed_resource::NONE)
        .manifest_required()
        .expect("cannot compile heka.rc with the Windows SDK's rc.exe");
}

fn variable(name: &str) -> String {
    env::var(name).unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn resources(icon: &str) -> String {
    let icon = icon.replace('\\', r"\\");
    let version = variable("CARGO_PKG_VERSION");
    let numbers = format!(
        "{},{},{},0",
        variable("CARGO_PKG_VERSION_MAJOR"),
        variable("CARGO_PKG_VERSION_MINOR"),
        variable("CARGO_PKG_VERSION_PATCH"),
    );
    let license = variable("CARGO_PKG_LICENSE");
    format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {numbers}
PRODUCTVERSION {numbers}
FILEOS 0x40004
FILETYPE 0x1
{{
    BLOCK "StringFileInfo"
    {{
        BLOCK "040904B0"
        {{
            VALUE "FileDescription", "heka"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "heka"
            VALUE "LegalCopyright", "Licensed under {license}"
            VALUE "OriginalFilename", "heka.exe"
            VALUE "ProductName", "heka"
            VALUE "ProductVersion", "{version}"
        }}
    }}
    BLOCK "VarFileInfo"
    {{
        VALUE "Translation", 0x409, 1200
    }}
}}
"#
    )
}

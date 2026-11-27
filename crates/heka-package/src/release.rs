use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Error;
use crate::crates::Crate;
use crate::notices::{Section, notices};

pub struct Release<'a> {
    pub version: &'a str,
    pub workspace: &'a Path,
    pub exe: &'a Path,
    pub rust_notices: &'a Path,
    pub crates: &'a [Crate],
}

const MIN_THEME_LICENSE: &str = "crates/heka-app/themes/LICENSE-MIN-THEME";
const FONT_LICENSE: &str = "crates/heka-app/fonts/LICENSE-GOOGLE-SANS-CODE";
const CHROMIUM: &str = include_str!("../notices/chromium.txt");

impl Release<'_> {
    pub fn zip(&self, dist: &Path) -> Result<PathBuf, Error> {
        let name = format!("heka-v{}-windows-x64", self.version);
        let folder = dist.join(&name);
        let zip = dist.join(format!("{name}.zip"));
        remove(fs::remove_dir_all(&folder), &folder)?;
        remove(fs::remove_file(&zip), &zip)?;
        fs::create_dir_all(&folder)
            .map_err(|error| format!("cannot create {}: {error}", folder.display()))?;

        let min_theme = self.workspace.join(MIN_THEME_LICENSE);
        let font = self.workspace.join(FONT_LICENSE);
        for (file, source) in [
            ("heka.exe", self.exe.to_path_buf()),
            ("README.md", self.workspace.join("README.md")),
            ("LICENSE-MIT", self.workspace.join("LICENSE-MIT")),
            ("LICENSE-APACHE", self.workspace.join("LICENSE-APACHE")),
            ("LICENSE-MIN-THEME", min_theme.clone()),
            ("LICENSE-GOOGLE-SANS-CODE", font.clone()),
            (
                "THIRD-PARTY-NOTICES-RUST.html",
                self.rust_notices.to_path_buf(),
            ),
        ] {
            fs::copy(&source, folder.join(file))
                .map_err(|error| format!("cannot copy {}: {error}", source.display()))?;
        }

        let read = |path: &Path| {
            fs::read_to_string(path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))
        };
        let sections = [
            Section {
                title: "Min Dark and Min Light themes, converted from the Zed port of Min Theme"
                    .into(),
                text: read(&min_theme)?,
            },
            Section {
                title: "Google Sans Code, the font built into heka.exe".into(),
                text: read(&font)?,
            },
            Section {
                title: "Code derived from Chromium, in the AccessKit crates".into(),
                text: CHROMIUM.into(),
            },
            Section {
                title: "The Rust standard library".into(),
                text: "Its notices are in THIRD-PARTY-NOTICES-RUST.html.".into(),
            },
            Section {
                title: "HIDAPI".into(),
                text: "heka uses the HIDAPI C library, built into the hidapi crate, under \
                       its BSD-style license."
                    .into(),
            },
        ];
        fs::write(
            folder.join("THIRD-PARTY-NOTICES.txt"),
            notices(self.crates, &sections)?,
        )?;

        let status = Command::new(tar())
            .arg("-a")
            .arg("-cf")
            .arg(&zip)
            .arg("-C")
            .arg(dist)
            .arg(&name)
            .status()
            .map_err(|error| format!("cannot run tar: {error}"))?;
        if !status.success() {
            return Err(format!("tar could not write {}: {status}", zip.display()).into());
        }
        Ok(zip)
    }
}

fn remove(result: std::io::Result<()>, path: &Path) -> Result<(), Error> {
    match result {
        Err(error) if error.kind() != ErrorKind::NotFound => {
            Err(format!("cannot remove {}: {error}", path.display()).into())
        }
        _ => Ok(()),
    }
}

pub fn tar() -> PathBuf {
    let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(windows).join("System32").join("tar.exe")
}

pub fn check_tag(tag: &str, version: &str) -> Result<(), Error> {
    if tag.strip_prefix('v') == Some(version) {
        Ok(())
    } else {
        Err(format!(
            "tag {tag} does not name the version being built: expected v{version} \
             (the version in Cargo.toml)"
        )
        .into())
    }
}

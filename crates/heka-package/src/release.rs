//! The portable zip: one folder with heka.exe, the README, both of heka's licenses
//! and the third-party notices.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Error;
use crate::crates::Crate;
use crate::notices::{Section, notices};

/// What goes into one release zip.
pub struct Release<'a> {
    /// heka's version, without the `v`.
    pub version: &'a str,
    /// The top of heka's repository.
    pub workspace: &'a Path,
    /// The built heka.exe.
    pub exe: &'a Path,
    /// The Rust toolchain's notices for its standard library.
    pub rust_notices: &'a Path,
    /// The third-party crates in heka.exe.
    pub crates: &'a [Crate],
}

const ONE_DARK_LICENSE: &str = "crates/heka-app/themes/LICENSE-ONE-DARK";
const CHROMIUM: &str = include_str!("../notices/chromium.txt");

impl Release<'_> {
    /// Writes `heka-v<version>-windows-x64.zip` into `dist`, replacing any earlier
    /// one, and returns its path.
    pub fn zip(&self, dist: &Path) -> Result<PathBuf, Error> {
        let name = format!("heka-v{}-windows-x64", self.version);
        let folder = dist.join(&name);
        let zip = dist.join(format!("{name}.zip"));
        remove(fs::remove_dir_all(&folder), &folder)?;
        remove(fs::remove_file(&zip), &zip)?;
        fs::create_dir_all(&folder)
            .map_err(|error| format!("cannot create {}: {error}", folder.display()))?;

        let one_dark = self.workspace.join(ONE_DARK_LICENSE);
        for (file, source) in [
            ("heka.exe", self.exe.to_path_buf()),
            ("README.md", self.workspace.join("README.md")),
            ("LICENSE-MIT", self.workspace.join("LICENSE-MIT")),
            ("LICENSE-APACHE", self.workspace.join("LICENSE-APACHE")),
            ("LICENSE-ONE-DARK", one_dark.clone()),
            (
                "THIRD-PARTY-NOTICES-RUST.html",
                self.rust_notices.to_path_buf(),
            ),
        ] {
            fs::copy(&source, folder.join(file))
                .map_err(|error| format!("cannot copy {}: {error}", source.display()))?;
        }

        let one_dark = fs::read_to_string(&one_dark)
            .map_err(|error| format!("cannot read {}: {error}", one_dark.display()))?;
        let sections = [
            Section {
                title: "One Dark theme, converted from Zed's assets/themes/one/one.json".into(),
                text: one_dark,
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
            // `-a` picks the zip format from the file name.
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

/// A removal that found nothing to remove succeeds.
fn remove(result: std::io::Result<()>, path: &Path) -> Result<(), Error> {
    match result {
        Err(error) if error.kind() != ErrorKind::NotFound => {
            Err(format!("cannot remove {}: {error}", path.display()).into())
        }
        _ => Ok(()),
    }
}

/// The tar that comes with Windows, which writes zip files. A `tar` found on the
/// path may be another one that cannot.
pub fn tar() -> PathBuf {
    let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(windows).join("System32").join("tar.exe")
}

/// A release is built only from the tag `v<version>`, so that its name matches
/// what is inside.
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

//! The portable zip published on GitHub Releases.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use heka_package::release::{Release, check_tag, tar};

mod common;
use common::TempDir;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Runs Windows' tar, which reads zip files, and returns what it printed.
fn run_tar(args: &[&str]) -> String {
    let output = Command::new(tar()).args(args).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn zips_heka_with_both_licenses_the_readme_and_every_notice_in_one_folder() {
    let temp = TempDir::new("release");
    let exe = temp.path().join("built.exe");
    fs::write(&exe, "the program").unwrap();
    let rust_notices = temp.path().join("COPYRIGHT-library.html");
    fs::write(&rust_notices, "the standard library's notices").unwrap();
    let dist = temp.path().join("dist");
    let release = Release {
        version: "9.8.7",
        workspace: &workspace(),
        exe: &exe,
        rust_notices: &rust_notices,
        crates: &[],
    };

    // Twice: a second run replaces what the first left behind.
    release.zip(&dist).unwrap();
    fs::write(dist.join("heka-v9.8.7-windows-x64/stale.txt"), "stale").unwrap();
    let zip = release.zip(&dist).unwrap();

    assert_eq!(zip, dist.join("heka-v9.8.7-windows-x64.zip"));
    let listing = run_tar(&["-tf", zip.to_str().unwrap()]);
    let mut files: Vec<&str> = listing
        .lines()
        .filter(|line| !line.ends_with('/'))
        .collect();
    files.sort_unstable();
    assert_eq!(
        files,
        [
            "LICENSE-APACHE",
            "LICENSE-MIT",
            "LICENSE-ONE-DARK",
            "README.md",
            "THIRD-PARTY-NOTICES-RUST.html",
            "THIRD-PARTY-NOTICES.txt",
            "heka.exe",
        ]
        .map(|file| format!("heka-v9.8.7-windows-x64/{file}"))
    );

    let out = temp.path().join("out");
    fs::create_dir_all(&out).unwrap();
    run_tar(&["-xf", zip.to_str().unwrap(), "-C", out.to_str().unwrap()]);
    let read =
        |file: &str| fs::read_to_string(out.join("heka-v9.8.7-windows-x64").join(file)).unwrap();
    assert_eq!(read("heka.exe"), "the program");
    assert_eq!(
        read("THIRD-PARTY-NOTICES-RUST.html"),
        "the standard library's notices"
    );
    let notices = read("THIRD-PARTY-NOTICES.txt");
    // One Dark's holder, and the code in AccessKit that comes from Chromium.
    assert!(notices.contains("Copyright (c) 2014 GitHub Inc."));
    assert!(notices.contains("The Chromium Authors"));
    assert!(read("LICENSE-ONE-DARK").contains("GitHub Inc."));
}

#[test]
fn accepts_only_the_tag_that_names_the_version_being_built() {
    check_tag("v0.1.0", "0.1.0").unwrap();
    for tag in ["0.1.0", "v0.1.1", "v0.1.0-rc.1", "heka-v1"] {
        let error = check_tag(tag, "0.1.0").unwrap_err().to_string();

        assert!(error.contains(tag) && error.contains("v0.1.0"), "{error}");
    }
}

use std::fs;
use std::path::Path;

use heka_package::crates::Crate;
use heka_package::notices::{Section, notices};

mod common;
use common::TempDir;

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn unpacked(root: &TempDir, name: &str, license: &str, files: &[(&str, &str)]) -> Crate {
    let dir = root.path().join(name);
    fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        write(&dir, file, text);
    }
    Crate {
        name: name.into(),
        version: "1.0.0".into(),
        license: Some(license.into()),
        authors: vec!["Ada <ada@example.com>".into(), "Grace".into()],
        repository: Some(format!("https://example.com/{name}")),
        dir,
    }
}

#[test]
fn lists_each_crate_and_prints_a_shared_license_text_once() {
    let root = TempDir::new("shared");
    let crates = [
        unpacked(&root, "alpha", "MIT", &[("LICENSE-MIT", "Shared terms.\n")]),
        unpacked(
            &root,
            "beta",
            "MIT",
            &[("LICENSE", "Shared terms.\r\n\r\n")],
        ),
        unpacked(&root, "gamma", "ISC", &[("COPYING", "Gamma's terms.\n")]),
    ];

    let text = notices(&crates, &[]).unwrap();

    assert!(text.contains("alpha 1.0.0 (MIT) https://crates.io/crates/alpha/1.0.0"));
    assert!(text.contains("gamma 1.0.0 (ISC) https://crates.io/crates/gamma/1.0.0"));
    assert_eq!(text.matches("Shared terms.").count(), 1);
    assert!(text.contains("alpha 1.0.0: LICENSE-MIT\nbeta 1.0.0: LICENSE\n"));
    assert!(text.contains("Gamma's terms."));
}

#[test]
fn includes_the_licenses_of_code_vendored_inside_a_crate_but_not_of_its_tests() {
    let root = TempDir::new("vendored");
    let crates = [unpacked(
        &root,
        "hidapi",
        "MIT",
        &[
            ("LICENSE.txt", "Binding terms."),
            ("etc/hidapi/LICENSE-bsd.txt", "Vendored BSD terms."),
            ("etc/hidapi/LICENSE-gpl3.txt", "GNU terms."),
            ("tests/fonts/LICENSE.txt", "Test font terms."),
            ("html5lib-tests/LICENSE", "Test suite terms."),
            ("assets/icons/copyright.svg", "<svg/>"),
            ("src/license.rs", "fn license() {}"),
        ],
    )];

    let text = notices(&crates, &[]).unwrap();

    assert!(text.contains("Binding terms."));
    assert!(text.contains("hidapi 1.0.0: etc/hidapi/LICENSE-bsd.txt"));
    assert!(text.contains("Vendored BSD terms."));
    for absent in [
        "GNU terms.",
        "Test font terms.",
        "Test suite terms.",
        "<svg/>",
        "fn license",
    ] {
        assert!(!text.contains(absent), "{absent}");
    }
}

#[test]
fn includes_every_text_in_a_directory_of_licenses() {
    let root = TempDir::new("directory");
    let crates = [unpacked(
        &root,
        "reuse",
        "MIT",
        &[
            ("LICENSE", "See the directory."),
            (".licenses/Ada-MIT", "Ada's terms."),
            ("LICENSES/0BSD.txt", "Zero terms."),
        ],
    )];

    let text = notices(&crates, &[]).unwrap();

    assert!(text.contains("reuse 1.0.0: .licenses/Ada-MIT"));
    assert!(text.contains("Ada's terms."));
    assert!(text.contains("Zero terms."));
}

#[test]
fn prints_the_standard_license_for_a_crate_that_ships_no_license_file() {
    let root = TempDir::new("missing");
    let either = unpacked(&root, "either", "MIT/Apache-2.0", &[]);
    let mit = unpacked(&root, "mit-only", "MIT", &[]);

    let apache = notices(std::slice::from_ref(&either), &[]).unwrap();
    let mit = notices(&[mit], &[]).unwrap();

    assert!(apache.contains("either 1.0.0: no license file; Apache-2.0 applies"));
    assert!(apache.contains("TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION"));
    assert!(mit.contains("mit-only 1.0.0: no license file; MIT applies"));
    assert!(mit.contains("Copyright (c) Ada <ada@example.com>, Grace"));
    assert!(mit.contains("Permission is hereby granted, free of charge"));
    assert!(!mit.contains("Lucas Kouzoukian"));
}

#[test]
fn fails_rather_than_leave_a_crate_without_its_license_text() {
    let root = TempDir::new("unknown");
    for license in ["BSD-3-Clause", "MIT AND Zlib"] {
        let crates = [unpacked(&root, "odd", license, &[])];

        let error = notices(&crates, &[]).unwrap_err();

        assert!(error.to_string().contains("odd 1.0.0"), "{error}");
    }
}

#[test]
fn prints_notices_that_come_from_no_crate() {
    let section = Section {
        title: "Min Dark theme".into(),
        text: "Copyright (c) 2014 GitHub Inc.\r\n".into(),
    };

    let text = notices(&[], &[section]).unwrap();

    assert!(text.contains("Min Dark theme"));
    assert!(text.contains("Copyright (c) 2014 GitHub Inc.\n"));
}

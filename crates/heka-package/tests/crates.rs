use heka_package::crates::shipped;
use serde_json::{Value, json};

fn package(name: &str, source: Option<&str>) -> Value {
    json!({
        "id": name,
        "name": name,
        "version": "1.2.3",
        "license": "MIT",
        "license_file": null,
        "authors": ["A. Author"],
        "repository": null,
        "source": source,
        "manifest_path": format!("C:/registry/{name}/Cargo.toml"),
    })
}

fn registry(name: &str) -> Value {
    package(
        name,
        Some("registry+https://github.com/rust-lang/crates.io-index"),
    )
}

fn node(id: &str, deps: &[(&str, Option<&str>)]) -> Value {
    let deps: Vec<Value> = deps
        .iter()
        .map(|(pkg, kind)| json!({"pkg": pkg, "dep_kinds": [{"kind": kind}]}))
        .collect();
    json!({"id": id, "deps": deps})
}

#[test]
fn lists_third_party_normal_and_build_dependencies_of_the_app_but_not_dev_ones() {
    let metadata = json!({
        "packages": [
            package("heka-app", None),
            package("heka-core", None),
            package("heka-package", None),
            registry("ui"),
            registry("serde"),
            registry("builder"),
            registry("only-for-tests"),
            registry("only-for-packaging"),
        ],
        "resolve": {"nodes": [
            node("heka-app", &[("ui", None), ("heka-core", None), ("only-for-tests", Some("dev"))]),
            node("heka-core", &[("serde", None)]),
            node("heka-package", &[("only-for-packaging", None)]),
            node("ui", &[("builder", Some("build")), ("serde", None)]),
            node("serde", &[]),
            node("builder", &[]),
            node("only-for-tests", &[]),
            node("only-for-packaging", &[]),
        ]},
    });

    let crates = shipped(&metadata.to_string(), "heka-app").unwrap();

    let names: Vec<&str> = crates.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["builder", "serde", "ui"]);
    assert_eq!(crates[1].version, "1.2.3");
    assert_eq!(crates[1].license.as_deref(), Some("MIT"));
    assert_eq!(crates[1].dir, std::path::Path::new("C:/registry/serde"));
}

#[test]
fn fails_when_the_app_is_not_in_the_metadata() {
    let metadata = json!({"packages": [], "resolve": {"nodes": []}});

    let error = shipped(&metadata.to_string(), "heka-app").unwrap_err();

    assert!(error.to_string().contains("heka-app"), "{error}");
}

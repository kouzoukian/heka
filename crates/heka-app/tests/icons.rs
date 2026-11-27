use std::fs;
use std::path::{Path, PathBuf};

use gpui::AssetSource;
use heka_app::ui::assets::Assets;

fn sources(folder: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

fn file_name(identifier: &str) -> String {
    let mut name = String::new();
    let mut last: Option<char> = None;
    for character in identifier.chars() {
        let boundary = match last {
            Some(before) if character.is_ascii_uppercase() => {
                before.is_ascii_lowercase() || before.is_ascii_digit()
            }
            Some(before) if character.is_ascii_digit() => before.is_ascii_alphabetic(),
            _ => false,
        };
        if boundary {
            name.push('-');
        }
        name.push(character.to_ascii_lowercase());
        last = Some(character);
    }
    name
}

fn used_icons() -> Vec<String> {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut icons = Vec::new();
    for file in files {
        let text = fs::read_to_string(file).unwrap();
        for part in text.split("IconName::").skip(1) {
            let identifier: String = part
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric())
                .collect();
            if !identifier.is_empty() && !icons.contains(&identifier) {
                icons.push(identifier);
            }
        }
    }
    icons.sort();
    icons
}

#[test]
fn the_names_of_icon_files_follow_the_identifiers() {
    assert_eq!(file_name("Layers"), "layers");
    assert_eq!(file_name("AppWindow"), "app-window");
    assert_eq!(file_name("Gamepad2"), "gamepad-2");
    assert_eq!(file_name("CircleArrowUp"), "circle-arrow-up");
    assert_eq!(file_name("X"), "x");
}

#[test]
fn every_icon_the_window_uses_is_bundled() {
    let used = used_icons();

    assert!(used.len() > 20, "found only {} icons", used.len());
    let missing: Vec<String> = used
        .into_iter()
        .filter(|identifier| {
            let path = format!("icons/{}.svg", file_name(identifier));
            !matches!(Assets.load(&path), Ok(Some(_)))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "icons that are not bundled: {missing:?}"
    );
}

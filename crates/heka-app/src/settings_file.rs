use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use heka_app::{log, text};
use heka_core::settings::Settings;

pub fn folder() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("heka"))
}

fn path() -> Option<PathBuf> {
    Some(folder()?.join("settings.json"))
}

pub fn load() -> (Settings, Option<String>) {
    let Some(path) = path() else {
        return (Settings::default(), None);
    };
    match fs::read(&path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) if Settings::can_read(&text) => (Settings::load(Some(&text)), None),
            _ => (Settings::load(None), Some(set_aside(&path))),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => (Settings::load(None), None),
        Err(error) => {
            let unreadable = text::log::cannot_read_settings(&path, &error.to_string());
            (Settings::load(None), Some(unreadable))
        }
    }
}

fn set_aside(path: &Path) -> String {
    let copy = path.with_extension("json.unreadable");
    match fs::copy(path, &copy) {
        Ok(_) => text::log::settings_set_aside(path, &copy),
        Err(error) => text::log::cannot_read_settings(path, &error.to_string()),
    }
}

pub fn save(settings: &Settings) {
    let Some(path) = path() else {
        log::line(text::log::NO_APPDATA);
        return;
    };
    let temporary = path.with_extension("json.tmp");
    let result = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(&temporary, settings.to_json()))
        .and_then(|()| fs::rename(&temporary, &path));
    if let Err(error) = result {
        log::line(&text::log::cannot_save_settings(&path, &error.to_string()));
    }
}

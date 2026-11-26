use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use heka_app::{log, text};
use heka_core::settings::Settings;

fn path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("heka").join("settings.json"))
}

pub fn load() -> Settings {
    let Some(path) = path() else {
        return Settings::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => Settings::load(Some(&text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Settings::load(None),
        Err(error) => {
            log::line(&text::log::cannot_read_settings(&path, &error.to_string()));
            Settings::load(None)
        }
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

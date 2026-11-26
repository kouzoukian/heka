//! Where the settings live: `%APPDATA%\heka\settings.json`.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use heka_core::settings::Settings;

fn path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("heka").join("settings.json"))
}

/// The saved settings. A missing, unreadable or corrupt file gives the defaults.
pub fn load() -> Settings {
    let Some(path) = path() else {
        return Settings::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => Settings::load(Some(&text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Settings::load(None),
        Err(error) => {
            eprintln!("heka: cannot read {}: {error}", path.display());
            Settings::load(None)
        }
    }
}

/// Writes the settings through a temporary file so a crash never leaves half a file.
pub fn save(settings: &Settings) {
    let Some(path) = path() else {
        eprintln!("heka: APPDATA is not set; settings are not saved");
        return;
    };
    let temporary = path.with_extension("json.tmp");
    let result = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(&temporary, settings.to_json()))
        .and_then(|()| fs::rename(&temporary, &path));
    if let Err(error) = result {
        eprintln!("heka: cannot save {}: {error}", path.display());
    }
}

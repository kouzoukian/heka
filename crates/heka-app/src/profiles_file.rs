use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use heka_app::{log, text};
use heka_core::profile::Profile;
use heka_core::profiles::Profiles;

use crate::settings_file;

fn folder() -> Option<PathBuf> {
    Some(settings_file::folder()?.join("profiles"))
}

pub fn load() -> (Profiles, Vec<String>) {
    let Some(folder) = folder() else {
        return (Profiles::default(), Vec::new());
    };
    let entries = match fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return (Profiles::default(), Vec::new());
        }
        Err(error) => {
            let problem = text::log::cannot_read_profiles(&folder, &error.to_string());
            return (Profiles::default(), vec![problem]);
        }
    };
    let mut files = Vec::new();
    let mut problems = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str().and_then(Profiles::id_of_file) else {
            continue;
        };
        match fs::read_to_string(entry.path()) {
            Ok(content) => files.push((id.to_owned(), content)),
            Err(error) => {
                problems.push(text::log::cannot_read_profile(
                    &entry.path(),
                    &error.to_string(),
                ));
            }
        }
    }
    let ids: Vec<String> = files.iter().map(|(id, _)| id.clone()).collect();
    let profiles = Profiles::load(files);
    for id in ids {
        if profiles.get(&id).is_none() {
            let path = folder.join(Profiles::file_name(&id));
            problems.push(text::log::profile_skipped(&path));
        }
    }
    (profiles, problems)
}

pub fn save(id: &str, profile: &Profile) {
    let Some(folder) = folder() else {
        log::line(text::log::NO_APPDATA);
        return;
    };
    let path = folder.join(Profiles::file_name(id));
    let temporary = path.with_extension("json.tmp");
    let result = fs::create_dir_all(&folder)
        .and_then(|()| fs::write(&temporary, profile.to_json()))
        .and_then(|()| fs::rename(&temporary, &path));
    if let Err(error) = result {
        log::line(&text::log::cannot_save_profile(&path, &error.to_string()));
    }
}

pub fn remove(id: &str) {
    let Some(folder) = folder() else {
        return;
    };
    let path = folder.join(Profiles::file_name(id));
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => log::line(&text::log::cannot_remove_profile(&path, &error.to_string())),
    }
}

pub fn read_import(path: &Path) -> Result<(String, String), String> {
    let content = fs::read_to_string(path)
        .map_err(|error| text::cannot_read_import(path, &error.to_string()))?;
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok((content, name))
}

pub fn write_export(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|error| text::cannot_export(path, &error.to_string()))
}

use std::fs;
use std::path::Path;

use crate::Error;
use crate::crates::Crate;

pub struct Section {
    pub title: String,
    pub text: String,
}

const APACHE: &str = include_str!("../../../LICENSE-APACHE");
const MIT: &str = include_str!("../../../LICENSE-MIT");

const LICENSE_NAMES: [&str; 4] = ["license", "licence", "copying", "unlicense"];
const NOTICE_NAMES: [&str; 2] = ["copyright", "notice"];
const NOT_TEXT: [&str; 6] = ["rs", "svg", "png", "json", "py", "toml"];
const NOT_BUILT: [&str; 5] = ["tests", "test", "benches", "examples", "scripts"];
const NOT_TAKEN: [(&str, &str); 1] = [("hidapi", "LICENSE-gpl3.txt")];

const RULE: &str =
    "\n================================================================================\n";

pub fn notices(crates: &[Crate], extra: &[Section]) -> Result<String, Error> {
    let mut texts: Vec<(String, Vec<String>)> = Vec::new();
    let mut add = |from: String, text: &str| {
        let text = clean(text);
        match texts.iter_mut().find(|(known, _)| *known == text) {
            Some((_, sources)) => sources.push(from),
            None => texts.push((text, vec![from])),
        }
    };

    let mut packages = String::new();
    for package in crates {
        let Crate { name, version, .. } = package;
        let license = package.license.as_deref().unwrap_or("no license field");
        packages +=
            &format!("{name} {version} ({license}) https://crates.io/crates/{name}/{version}\n");

        let mut files = Vec::new();
        license_files(&package.dir, "", &mut files)?;
        files.retain(|file| !NOT_TAKEN.contains(&(name.as_str(), file_name(file))));
        files.sort();
        if !files
            .iter()
            .any(|file| !file.contains('/') && starts_with_any(file, &LICENSE_NAMES))
        {
            let (license, text) = standard_text(package)?;
            add(
                format!("{name} {version}: no license file; {license} applies"),
                &text,
            );
        }
        for file in files {
            let bytes = fs::read(package.dir.join(&file))
                .map_err(|error| format!("{name} {version}: cannot read {file}: {error}"))?;
            add(
                format!("{name} {version}: {file}"),
                &String::from_utf8_lossy(&bytes),
            );
        }
    }

    let mut out = String::from(
        "THIRD-PARTY NOTICES\n\n\
         heka.exe includes the software listed here. heka's own licenses are in\n\
         LICENSE-MIT and LICENSE-APACHE.\n",
    );
    for section in extra {
        out += &format!("{RULE}{}\n\n{}\n", section.title, clean(&section.text));
    }
    out += &format!("{RULE}Packages\n\n{packages}");
    for (text, sources) in texts {
        out += &format!("{RULE}{}\n\n{text}\n", sources.join("\n"));
    }
    Ok(out)
}

fn clean(text: &str) -> String {
    text.trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .trim_start_matches('\n')
        .trim_end()
        .to_owned()
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn starts_with_any(name: &str, starts: &[&str]) -> bool {
    let name = name.to_ascii_lowercase();
    starts.iter().any(|start| name.starts_with(start))
}

fn license_files(dir: &Path, prefix: &str, found: &mut Vec<String>) -> Result<(), Error> {
    let entries =
        fs::read_dir(dir).map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() {
            if !NOT_BUILT.contains(&name.as_str()) && !name.ends_with("-tests") {
                license_files(&entry.path(), &format!("{prefix}{name}/"), found)?;
            }
        } else if (is_license_directory(prefix)
            || starts_with_any(&name, &LICENSE_NAMES)
            || starts_with_any(&name, &NOTICE_NAMES))
            && !name
                .rsplit_once('.')
                .is_some_and(|(_, extension)| NOT_TEXT.contains(&extension))
        {
            found.push(format!("{prefix}{name}"));
        }
    }
    Ok(())
}

fn is_license_directory(prefix: &str) -> bool {
    prefix.split('/').any(|dir| {
        let dir = dir.trim_start_matches('.');
        dir.eq_ignore_ascii_case("licenses") || dir.eq_ignore_ascii_case("licences")
    })
}

fn standard_text(package: &Crate) -> Result<(&'static str, String), Error> {
    let Crate { name, version, .. } = package;
    let license = package.license.as_deref().unwrap_or_default();
    let offers = |wanted: &str| {
        !license.contains(" AND ")
            && license
                .split(" OR ")
                .flat_map(|choice| choice.split('/'))
                .any(|choice| choice.trim() == wanted)
    };
    if offers("Apache-2.0") {
        Ok(("Apache-2.0", APACHE.to_owned()))
    } else if offers("MIT") {
        let holders = if package.authors.is_empty() {
            format!("the {name} authors")
        } else {
            package.authors.join(", ")
        };
        let text = clean(MIT)
            .lines()
            .map(|line| {
                if line.starts_with("Copyright (c)") {
                    format!("Copyright (c) {holders}")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(("MIT", text))
    } else {
        Err(format!(
            "{name} {version} ships no license file and heka-package has no text for \
             its license ({license:?})"
        )
        .into())
    }
}

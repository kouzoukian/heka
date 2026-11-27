use std::collections::BTreeMap;

use crate::profile::Profile;

const RESERVED_ID: &str = "default";
const MAX_ID_CHARS: usize = 40;
const FALLBACK_ID: &str = "profile";
const DEVICE_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profiles {
    items: BTreeMap<String, Profile>,
}

impl Profiles {
    pub fn load<I>(files: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let items = files
            .into_iter()
            .filter_map(|(id, text)| {
                let profile = Profile::load(&text, &id)?;
                Some((id, profile))
            })
            .collect();
        Self { items }
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.items.keys().map(String::as_str)
    }

    pub fn list(&self) -> impl Iterator<Item = (&str, &Profile)> {
        self.items
            .iter()
            .map(|(id, profile)| (id.as_str(), profile))
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.items.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Profile> {
        self.items.get_mut(id)
    }

    pub fn effective(&self, choice: Option<&str>) -> Profile {
        choice
            .and_then(|id| self.get(id))
            .cloned()
            .unwrap_or_default()
    }

    pub fn create(&mut self, name: &str) -> String {
        let profile = Profile::named(name);
        let id = self.free_id(&profile.name);
        self.items.insert(id.clone(), profile);
        id
    }

    pub fn remove(&mut self, id: &str) -> bool {
        self.items.remove(id).is_some()
    }

    pub fn file_name(id: &str) -> String {
        format!("{id}.json")
    }

    pub fn id_of_file(file_name: &str) -> Option<&str> {
        let id = file_name.strip_suffix(".json")?;
        (!id.is_empty()).then_some(id)
    }

    fn free_id(&self, name: &str) -> String {
        let base = file_safe(name);
        let taken = |id: &str| id == RESERVED_ID || self.items.contains_key(id);
        if !taken(&base) {
            return base;
        }
        (2..)
            .map(|number| format!("{base}-{number}"))
            .find(|id| !taken(id))
            .expect("a free number exists")
    }
}

fn file_safe(name: &str) -> String {
    let mut id = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character.to_ascii_lowercase());
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    let id: String = id.chars().take(MAX_ID_CHARS).collect();
    let id = id.trim_end_matches('-');
    if id.is_empty() {
        FALLBACK_ID.to_owned()
    } else if DEVICE_NAMES.contains(&id) {
        format!("{FALLBACK_ID}-{id}")
    } else {
        id.to_owned()
    }
}

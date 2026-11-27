use heka_core::profile::{Binding, DEFAULT_NAME, Input, Output, Profile, ProfileError};
use heka_core::profiles::{Profiles, suggest_name};

fn file(id: &str, text: &str) -> (String, String) {
    (id.to_owned(), text.to_owned())
}

fn ids(profiles: &Profiles) -> Vec<&str> {
    profiles.ids().collect()
}

#[test]
fn no_files_give_no_profiles() {
    assert_eq!(ids(&Profiles::load([])), Vec::<&str>::new());
}

#[test]
fn profiles_load_by_file_name() {
    let profiles = Profiles::load([
        file(
            "racing",
            r#"{"name": "Racing", "bindings": {"cross": "circle"}}"#,
        ),
        file("shooter", r#"{"name": "Shooter"}"#),
    ]);

    assert_eq!(ids(&profiles), ["racing", "shooter"]);
    let racing = profiles.get("racing").unwrap();
    assert_eq!(racing.name, "Racing");
    assert_eq!(racing.binding(Input::Cross), Binding::To(Output::Circle));
}

#[test]
fn a_broken_file_does_not_stop_the_others() {
    let profiles = Profiles::load([
        file("broken", "{"),
        file("list", "[]"),
        file("fine", r#"{"name": "Fine"}"#),
    ]);

    assert_eq!(ids(&profiles), ["fine"]);
}

#[test]
fn a_profile_without_a_name_is_named_after_its_file() {
    let profiles = Profiles::load([file("night-mode", "{}")]);

    assert_eq!(profiles.get("night-mode").unwrap().name, "night-mode");
}

#[test]
fn no_choice_and_an_unknown_choice_give_the_default_profile() {
    let profiles = Profiles::load([file("racing", r#"{"name": "Racing"}"#)]);

    assert_eq!(profiles.effective(None), Profile::default());
    assert_eq!(profiles.effective(Some("gone")), Profile::default());
    assert_eq!(profiles.effective(None).name, DEFAULT_NAME);
}

#[test]
fn a_known_choice_gives_that_profile() {
    let profiles = Profiles::load([file("racing", r#"{"name": "Racing"}"#)]);

    assert_eq!(profiles.effective(Some("racing")).name, "Racing");
}

#[test]
fn a_new_profile_gets_an_id_from_its_name() {
    let mut profiles = Profiles::default();

    let id = profiles.create("  Racing Wheel!  ");

    assert_eq!(id, "racing-wheel");
    assert_eq!(profiles.get(&id).unwrap().name, "Racing Wheel!");
    assert!(profiles.get(&id).unwrap().is_plain());
}

#[test]
fn a_taken_id_gets_a_number() {
    let mut profiles = Profiles::default();

    let first = profiles.create("Racing");
    let second = profiles.create("racing");
    let third = profiles.create("RACING");

    assert_eq!([first, second, third], ["racing", "racing-2", "racing-3"]);
}

#[test]
fn a_freed_id_is_used_again() {
    let mut profiles = Profiles::default();
    profiles.create("Racing");
    let second = profiles.create("Racing");
    profiles.remove("racing");

    assert_eq!(second, "racing-2");
    assert_eq!(profiles.create("Racing"), "racing");
}

#[test]
fn the_default_profile_never_has_a_file() {
    let mut profiles = Profiles::default();

    assert_eq!(profiles.create("Default"), "default-2");
    assert_eq!(profiles.create("default"), "default-3");
}

#[test]
fn an_id_is_safe_as_a_windows_file_name() {
    let mut profiles = Profiles::default();

    assert_eq!(profiles.create("NUL"), "profile-nul");
    assert_eq!(profiles.create("COM1"), "profile-com1");
    assert_eq!(profiles.create("???"), "profile");
    assert_eq!(profiles.create("a/b\\c:d*e"), "a-b-c-d-e");
    assert_eq!(profiles.create("Zażółć"), "za");
    assert_eq!(profiles.create(&"x".repeat(24)), "x".repeat(24));
}

#[test]
fn removing_a_profile_says_whether_it_was_there() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");

    assert!(profiles.remove(&id));
    assert!(!profiles.remove(&id));
    assert_eq!(ids(&profiles), Vec::<&str>::new());
}

#[test]
fn a_profile_can_be_edited_in_place() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");

    profiles
        .get_mut(&id)
        .unwrap()
        .bind(Input::Cross, Binding::To(Output::Circle));

    assert_eq!(
        profiles.effective(Some(&id)).binding(Input::Cross),
        Binding::To(Output::Circle)
    );
    assert!(profiles.get_mut("gone").is_none());
}

#[test]
fn files_are_named_after_ids() {
    assert_eq!(Profiles::file_name("racing"), "racing.json");
    assert_eq!(Profiles::id_of_file("racing.json"), Some("racing"));
    assert_eq!(Profiles::id_of_file("racing.json.tmp"), None);
    assert_eq!(Profiles::id_of_file("racing.txt"), None);
    assert_eq!(Profiles::id_of_file(".json"), None);
}

#[test]
fn a_suggested_name_is_the_first_free_number() {
    assert_eq!(suggest_name([]), "Profile 1");
    assert_eq!(suggest_name(["Profile 1"]), "Profile 2");
    assert_eq!(suggest_name(["Profile 1", "Profile 3"]), "Profile 2");
    assert_eq!(suggest_name(["Racing", "Shooter"]), "Profile 1");
}

#[test]
fn a_suggested_name_ignores_case() {
    assert_eq!(suggest_name(["PROFILE 1", "profile 2"]), "Profile 3");
}

fn name_of(profiles: &Profiles, id: &str) -> String {
    profiles.get(id).unwrap().name.clone()
}

#[test]
fn a_taken_name_gets_a_distinct_one() {
    let mut profiles = Profiles::default();

    let first = profiles.create("Racing");
    let second = profiles.create("racing");
    let third = profiles.create("RACING");

    assert_eq!(name_of(&profiles, &first), "Racing");
    assert_eq!(name_of(&profiles, &second), "racing 2");
    assert_eq!(name_of(&profiles, &third), "RACING 3");
}

#[test]
fn no_profile_can_take_the_name_of_the_default_one() {
    let mut profiles = Profiles::default();

    let id = profiles.create("default");

    assert_eq!(name_of(&profiles, &id), "default 2");
}

#[test]
fn a_blank_name_becomes_profile() {
    let mut profiles = Profiles::default();

    let first = profiles.create("   ");
    let second = profiles.create("");

    assert_eq!(name_of(&profiles, &first), "Profile");
    assert_eq!(name_of(&profiles, &second), "Profile 2");
}

#[test]
fn a_distinct_name_still_fits_the_length_limit() {
    let mut profiles = Profiles::default();
    let long = "x".repeat(24);
    profiles.create(&long);

    let id = profiles.create(&long);

    let name = name_of(&profiles, &id);
    assert_eq!(name.chars().count(), 24);
    assert!(name.ends_with(" 2"));
}

#[test]
fn a_duplicate_is_a_copy_with_its_own_id_and_name() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");
    profiles
        .get_mut(&id)
        .unwrap()
        .bind(Input::Cross, Binding::To(Output::Circle));
    profiles.get_mut(&id).unwrap().output_type =
        Some(heka_core::virtual_controller::OutputType::Ps4);

    let copy = profiles.duplicate(Some(&id)).unwrap();
    let again = profiles.duplicate(Some(&id)).unwrap();

    assert_eq!(copy, "racing-copy");
    assert_eq!(again, "racing-copy-2");
    assert_eq!(name_of(&profiles, &copy), "Racing copy");
    assert_eq!(name_of(&profiles, &again), "Racing copy 2");
    let (original, duplicate) = (profiles.get(&id).unwrap(), profiles.get(&copy).unwrap());
    assert_eq!(duplicate.output_type, original.output_type);
    assert_eq!(duplicate.binding(Input::Cross), Binding::To(Output::Circle));
}

#[test]
fn a_duplicate_is_independent_of_its_original() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");
    let copy = profiles.duplicate(Some(&id)).unwrap();

    profiles
        .get_mut(&copy)
        .unwrap()
        .bind(Input::Cross, Binding::Nothing);

    assert_eq!(
        profiles.get(&id).unwrap().binding(Input::Cross),
        Binding::Default
    );
}

#[test]
fn the_default_profile_can_be_duplicated() {
    let mut profiles = Profiles::default();

    let copy = profiles.duplicate(None).unwrap();

    assert_eq!(name_of(&profiles, &copy), "Default copy");
    assert!(profiles.get(&copy).unwrap().is_plain());
}

#[test]
fn a_profile_that_is_not_there_cannot_be_duplicated() {
    assert_eq!(Profiles::default().duplicate(Some("gone")), None);
}

#[test]
fn renaming_changes_the_name_and_keeps_the_file() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");

    assert!(profiles.rename(&id, "  Rally  "));

    assert_eq!(name_of(&profiles, &id), "Rally");
    assert_eq!(ids(&profiles), ["racing"]);
}

#[test]
fn renaming_to_a_taken_name_gives_a_distinct_one() {
    let mut profiles = Profiles::default();
    profiles.create("Racing");
    let other = profiles.create("Shooter");

    profiles.rename(&other, "racing");

    assert_eq!(name_of(&profiles, &other), "racing 2");
}

#[test]
fn renaming_to_its_own_name_changes_nothing() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");

    profiles.rename(&id, "Racing");

    assert_eq!(name_of(&profiles, &id), "Racing");
}

#[test]
fn a_rename_can_only_change_the_case_of_the_same_name() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");

    profiles.rename(&id, "RACING");

    assert_eq!(name_of(&profiles, &id), "RACING");
}

#[test]
fn a_blank_name_or_a_missing_profile_cannot_be_renamed_to() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");

    assert!(!profiles.rename(&id, "   "));
    assert!(!profiles.rename("gone", "Rally"));
    assert_eq!(name_of(&profiles, &id), "Racing");
}

#[test]
fn a_renamed_profile_may_not_take_the_default_name() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");

    profiles.rename(&id, "Default");

    assert_eq!(name_of(&profiles, &id), "Default 2");
}

#[test]
fn an_exported_profile_imports_as_the_same_profile() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");
    let profile = profiles.get_mut(&id).unwrap();
    profile.bind(Input::Cross, Binding::To(Output::RightTrigger));
    profile.bind(Input::Mute, Binding::Nothing);
    let (file, text) = profiles.export(&id).unwrap();
    let mut elsewhere = Profiles::default();

    let imported = elsewhere.import(&text, "ignored").unwrap();

    assert_eq!(file, "racing.json");
    assert_eq!(elsewhere.get(&imported), profiles.get(&id));
}

#[test]
fn importing_a_name_already_taken_gives_a_distinct_one() {
    let mut profiles = Profiles::default();
    let id = profiles.create("Racing");
    let (_, text) = profiles.export(&id).unwrap();

    let imported = profiles.import(&text, "racing").unwrap();

    assert_eq!(imported, "racing-2");
    assert_eq!(name_of(&profiles, &imported), "Racing 2");
    assert_eq!(name_of(&profiles, &id), "Racing");
}

#[test]
fn an_imported_profile_without_a_name_takes_the_files() {
    let mut profiles = Profiles::default();

    let id = profiles
        .import(r#"{"bindings": {"cross": "circle"}}"#, "Night mode")
        .unwrap();

    assert_eq!(name_of(&profiles, &id), "Night mode");
}

#[test]
fn a_file_that_cannot_be_read_says_why_and_changes_nothing() {
    let mut profiles = Profiles::default();
    profiles.create("Racing");
    let before = profiles.clone();

    let broken = profiles.import("{", "x");
    let list = profiles.import("[1, 2]", "x");
    let other = profiles.import(r#"{"controllers": {}}"#, "x");
    let empty = profiles.import("", "x");

    assert!(matches!(broken, Err(ProfileError::NotJson(_))));
    assert!(matches!(empty, Err(ProfileError::NotJson(_))));
    assert_eq!(list, Err(ProfileError::NotAProfile));
    assert_eq!(other, Err(ProfileError::NotAProfile));
    assert_eq!(profiles, before);
}

#[test]
fn a_profile_that_is_not_there_cannot_be_exported() {
    assert_eq!(Profiles::default().export("gone"), None);
}

#[test]
fn parsing_tells_a_profile_from_other_json() {
    assert!(Profile::parse(r#"{"name": "A"}"#, "").is_ok());
    assert!(Profile::parse(r#"{"output_type": "ps4"}"#, "A").is_ok());
    assert!(Profile::parse(r#"{"bindings": {}}"#, "A").is_ok());
    assert_eq!(Profile::parse("{}", "A"), Err(ProfileError::NotAProfile));
    assert_eq!(
        Profile::parse(r#"{"bindings": {}}"#, "  "),
        Err(ProfileError::NotAProfile)
    );
}

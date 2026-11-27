use heka_core::profile::{Binding, DEFAULT_NAME, Input, Output, Profile};
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

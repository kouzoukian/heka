use heka_app::text;
use heka_app::ui::games::programs_to_offer;
use heka_app::ui::model::{ProfileEntry, profile_name, sorted_profiles};
use heka_core::game_rules::GameRule;
use heka_core::profile::Profile;

fn entry(id: &str, name: &str) -> ProfileEntry {
    ProfileEntry {
        id: id.to_owned(),
        profile: Profile::named(name),
        users: 0,
    }
}

fn rule(program: &str) -> GameRule {
    GameRule::new(program, "racing").unwrap()
}

#[test]
fn programs_that_already_have_a_rule_are_not_offered_again() {
    let programs = ["cs2.exe", "hades2.exe", "steam.exe"].map(String::from);

    let offered = programs_to_offer(&programs, &[rule("CS2.exe"), rule("other.exe")]);

    assert_eq!(offered, ["hades2.exe", "steam.exe"]);
}

#[test]
fn every_program_is_offered_when_there_are_no_rules() {
    let programs = ["cs2.exe", "steam.exe"].map(String::from);

    assert_eq!(programs_to_offer(&programs, &[]), programs);
    assert!(programs_to_offer(&[], &[rule("cs2.exe")]).is_empty());
}

#[test]
fn profiles_are_listed_by_name_without_regard_to_case() {
    let profiles = [
        entry("b", "shooter"),
        entry("a", "Racing"),
        entry("c", "Souls"),
    ];

    let sorted: Vec<&str> = sorted_profiles(&profiles)
        .into_iter()
        .map(|profile| profile.id.as_str())
        .collect();

    assert_eq!(sorted, ["a", "b", "c"]);
}

#[test]
fn a_profile_is_found_by_its_id_and_a_missing_one_has_no_name() {
    let profiles = [entry("racing", "Racing")];

    assert_eq!(profile_name(&profiles, "racing"), Some("Racing"));
    assert_eq!(profile_name(&profiles, "gone"), None);
}

#[test]
fn the_strip_names_the_rule_and_the_choice_that_returns() {
    assert_eq!(
        text::rule_applies("eldenring.exe", "Souls", "Default"),
        "A rule applies: eldenring.exe uses Souls. Your choice, Default, returns afterwards."
    );
}

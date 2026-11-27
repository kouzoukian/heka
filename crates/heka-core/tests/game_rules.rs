use std::cell::RefCell;
use std::collections::BTreeSet;

use heka_core::app_settings::AppSettings;
use heka_core::game_rules::{Front, GameRule, GameRules, RuleSelector, Surroundings, program_name};
use heka_core::settings::Settings;

#[derive(Default)]
struct World {
    profiles: BTreeSet<&'static str>,
    sets_output: BTreeSet<&'static str>,
    running: RefCell<BTreeSet<&'static str>>,
}

impl World {
    fn start(&self, program: &'static str) {
        self.running.borrow_mut().insert(program);
    }

    fn exit(&self, program: &'static str) {
        self.running.borrow_mut().remove(program);
    }
}

impl Surroundings for World {
    fn profile_exists(&self, profile: &str) -> bool {
        self.profiles.contains(profile)
    }

    fn profile_sets_output_type(&self, profile: &str) -> bool {
        self.sets_output.contains(profile)
    }

    fn is_running(&self, program: &str) -> bool {
        self.running.borrow().contains(program)
    }
}

fn world() -> World {
    World {
        profiles: ["souls", "shooter", "racing"].into(),
        sets_output: ["souls"].into(),
        running: RefCell::default(),
    }
}

fn rules() -> GameRules {
    let mut rules = GameRules::default();
    rules.add("eldenring.exe", "souls");
    rules.add("cs2.exe", "shooter");
    rules.add("RocketLeague.exe", "racing");
    rules
}

fn applying(
    selector: &mut RuleSelector,
    rules: &GameRules,
    world: &World,
    front: Front<'_>,
) -> Option<String> {
    selector
        .choose(rules, world, front)
        .map(|rule| rule.profile.clone())
}

#[test]
fn a_program_is_known_by_its_file_name_without_regard_to_case() {
    assert_eq!(program_name("EldenRing.EXE"), "eldenring.exe");
    assert_eq!(
        program_name(r"C:\Games\Elden Ring\EldenRing.exe"),
        "eldenring.exe"
    );
    assert_eq!(program_name("/opt/games/cs2.exe"), "cs2.exe");
    assert_eq!(program_name("  cs2.exe  "), "cs2.exe");
    assert_eq!(program_name(""), "");
}

#[test]
fn a_rule_needs_a_program_and_a_profile() {
    assert!(GameRule::new("cs2.exe", "shooter").is_some());
    assert_eq!(GameRule::new("", "shooter"), None);
    assert_eq!(GameRule::new("  ", "shooter"), None);
    assert_eq!(GameRule::new("cs2.exe", ""), None);
    assert_eq!(
        GameRule::new(r"C:\x\CS2.exe", "s").unwrap().program,
        "cs2.exe"
    );
}

#[test]
fn rules_are_found_by_program_and_the_first_for_a_program_wins() {
    let mut rules = GameRules::default();

    assert!(rules.add("cs2.exe", "shooter"));
    assert!(!rules.add(r"D:\Games\CS2.EXE", "racing"));

    assert_eq!(rules.find("CS2.exe").unwrap().profile, "shooter");
    assert_eq!(rules.iter().count(), 1);
    assert!(rules.find("other.exe").is_none());
}

#[test]
fn a_rule_can_be_given_another_profile_or_removed() {
    let mut rules = rules();

    assert!(rules.set_profile("CS2.exe", "racing"));
    assert_eq!(rules.find("cs2.exe").unwrap().profile, "racing");
    assert!(!rules.set_profile("gone.exe", "racing"));
    assert!(!rules.set_profile("cs2.exe", ""));
    assert!(rules.remove("cs2.exe"));
    assert!(!rules.remove("cs2.exe"));
    assert!(rules.find("cs2.exe").is_none());
}

#[test]
fn the_rules_of_a_removed_profile_can_be_dropped() {
    let mut rules = rules();
    rules.add("other.exe", "racing");

    assert_eq!(rules.remove_profile("racing"), 2);
    assert_eq!(rules.remove_profile("racing"), 0);
    assert_eq!(rules.iter().count(), 2);
}

#[test]
fn nothing_applies_without_rules() {
    let mut selector = RuleSelector::default();

    let chosen = applying(
        &mut selector,
        &GameRules::default(),
        &world(),
        Front::Program("cs2.exe"),
    );

    assert_eq!(chosen, None);
}

#[test]
fn a_program_in_front_applies_its_rule_and_leaving_restores_the_controllers_own() {
    let (rules, world) = (rules(), world());
    let mut selector = RuleSelector::default();

    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Program("cs2.exe")).as_deref(),
        Some("shooter")
    );
    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Program("firefox.exe")),
        None
    );
}

#[test]
fn the_desktop_is_just_another_program_with_no_rule() {
    let (rules, world) = (rules(), world());
    let mut selector = RuleSelector::default();
    applying(&mut selector, &rules, &world, Front::Program("cs2.exe"));

    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Nothing),
        None
    );
}

#[test]
fn the_rule_follows_the_program_that_is_in_front() {
    let (rules, world) = (rules(), world());
    let mut selector = RuleSelector::default();

    applying(&mut selector, &rules, &world, Front::Program("cs2.exe"));
    let next = applying(
        &mut selector,
        &rules,
        &world,
        Front::Program("RocketLeague.exe"),
    );

    assert_eq!(next.as_deref(), Some("racing"));
    assert_eq!(selector.applying(), Some("rocketleague.exe"));
}

#[test]
fn a_path_and_a_different_case_find_the_rule_too() {
    let (rules, world) = (rules(), world());
    let mut selector = RuleSelector::default();

    let chosen = applying(
        &mut selector,
        &rules,
        &world,
        Front::Program(r"C:\Steam\CS2.EXE"),
    );

    assert_eq!(chosen.as_deref(), Some("shooter"));
}

#[test]
fn hekas_own_window_changes_nothing() {
    let (rules, world) = (rules(), world());
    let mut selector = RuleSelector::default();

    assert_eq!(applying(&mut selector, &rules, &world, Front::Own), None);
    applying(&mut selector, &rules, &world, Front::Program("cs2.exe"));
    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Own).as_deref(),
        Some("shooter")
    );
    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Own).as_deref(),
        Some("shooter")
    );
    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Program("firefox.exe")),
        None
    );
    assert_eq!(applying(&mut selector, &rules, &world, Front::Own), None);
}

#[test]
fn a_rule_whose_profile_sets_the_output_type_stays_across_alt_tab_while_the_game_runs() {
    let (rules, world) = (rules(), world());
    world.start("eldenring.exe");
    let mut selector = RuleSelector::default();

    applying(
        &mut selector,
        &rules,
        &world,
        Front::Program("eldenring.exe"),
    );

    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Program("firefox.exe")).as_deref(),
        Some("souls")
    );
    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Nothing).as_deref(),
        Some("souls")
    );
    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Own).as_deref(),
        Some("souls")
    );
}

#[test]
fn such_a_rule_ends_when_its_program_exits() {
    let (rules, world) = (rules(), world());
    world.start("eldenring.exe");
    let mut selector = RuleSelector::default();
    applying(
        &mut selector,
        &rules,
        &world,
        Front::Program("eldenring.exe"),
    );
    applying(&mut selector, &rules, &world, Front::Program("firefox.exe"));

    world.exit("eldenring.exe");

    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Program("firefox.exe")),
        None
    );
    assert_eq!(selector.applying(), None);
}

#[test]
fn such_a_rule_gives_way_to_another_rule() {
    let (rules, world) = (rules(), world());
    world.start("eldenring.exe");
    let mut selector = RuleSelector::default();
    applying(
        &mut selector,
        &rules,
        &world,
        Front::Program("eldenring.exe"),
    );

    let next = applying(&mut selector, &rules, &world, Front::Program("cs2.exe"));

    assert_eq!(next.as_deref(), Some("shooter"));
}

#[test]
fn a_rule_that_leaves_the_output_type_alone_ends_at_once() {
    let (rules, world) = (rules(), world());
    world.start("cs2.exe");
    let mut selector = RuleSelector::default();
    applying(&mut selector, &rules, &world, Front::Program("cs2.exe"));

    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Program("firefox.exe")),
        None
    );
}

#[test]
fn a_rule_whose_profile_is_gone_is_ignored() {
    let rules = rules();
    let mut world = world();
    let mut selector = RuleSelector::default();
    world.profiles.remove("shooter");

    assert_eq!(
        applying(&mut selector, &rules, &world, Front::Program("cs2.exe")),
        None
    );
    assert_eq!(selector.applying(), None);
}

#[test]
fn an_applying_rule_ends_when_its_profile_is_deleted() {
    let rules = rules();
    let mut world = world();
    world.start("eldenring.exe");
    let mut selector = RuleSelector::default();
    applying(
        &mut selector,
        &rules,
        &world,
        Front::Program("eldenring.exe"),
    );

    world.profiles.remove("souls");

    assert_eq!(applying(&mut selector, &rules, &world, Front::Own), None);
    assert_eq!(selector.applying(), None);
}

#[test]
fn an_applying_rule_ends_when_it_is_removed() {
    let mut rules = rules();
    let world = world();
    let mut selector = RuleSelector::default();
    applying(&mut selector, &rules, &world, Front::Program("cs2.exe"));

    rules.remove("cs2.exe");

    assert_eq!(applying(&mut selector, &rules, &world, Front::Own), None);
}

#[test]
fn rules_survive_a_save_and_load() {
    let mut settings = Settings::default();
    settings.set_app(AppSettings {
        game_rules: rules(),
        ..AppSettings::default()
    });

    let loaded = Settings::load(Some(&settings.to_json()));

    assert_eq!(loaded.app().game_rules, rules());
}

#[test]
fn no_rules_write_nothing_and_a_file_without_them_loads_none() {
    let json = Settings::default().to_json();

    assert!(!json.contains("game_rules"));
    assert!(Settings::load(Some(&json)).app().game_rules.is_empty());
    assert!(
        Settings::load(Some(r#"{"app": {"start_in_tray": true}}"#))
            .app()
            .game_rules
            .is_empty()
    );
}

#[test]
fn broken_rules_are_dropped_and_the_rest_load() {
    let text = r#"{"app": {"game_rules": [
        {"program": "CS2.exe", "profile": "shooter"},
        {"program": "cs2.exe", "profile": "racing"},
        {"program": 5, "profile": "x"},
        {"program": "a.exe"},
        {"program": "", "profile": "x"},
        {"program": "b.exe", "profile": ""},
        "nonsense",
        {"program": "C:\\Games\\Hades2.exe", "profile": "racing"}
    ]}}"#;

    let rules = Settings::load(Some(text)).app().game_rules.clone();

    let loaded: Vec<(&str, &str)> = rules
        .iter()
        .map(|rule| (rule.program.as_str(), rule.profile.as_str()))
        .collect();
    assert_eq!(loaded, [("cs2.exe", "shooter"), ("hades2.exe", "racing")]);
}

#[test]
fn rules_that_are_not_a_list_load_as_none() {
    for text in [
        r#"{"app": {"game_rules": 5}}"#,
        r#"{"app": {"game_rules": {"cs2.exe": "shooter"}}}"#,
        r#"{"app": {"game_rules": null}}"#,
    ] {
        assert!(
            Settings::load(Some(text)).app().game_rules.is_empty(),
            "{text}"
        );
    }
}

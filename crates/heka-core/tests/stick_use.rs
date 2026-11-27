use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use heka_core::desktop::{Chord, Desk, DesktopError, DesktopEvent, DesktopOutput, Key};
use heka_core::profile::{Mapper, Profile};
use heka_core::running::RunningMapper;
use heka_core::state::{ControllerState, Stick};
use heka_core::stick_use::{Direction, StickRole, StickUse, StickUseChange};
use heka_core::tuning::{Curve, StickChange, StickSide};
use heka_core::xbox::XboxReport;

#[derive(Clone, Default)]
struct FakeDesktop(Arc<Mutex<Vec<DesktopEvent>>>);

impl FakeDesktop {
    fn take(&self) -> Vec<DesktopEvent> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

impl DesktopOutput for FakeDesktop {
    fn send(&mut self, events: &[DesktopEvent]) -> Result<(), DesktopError> {
        self.0.lock().unwrap().extend_from_slice(events);
        Ok(())
    }
}

struct Rig {
    running: RunningMapper,
    mapper: Mapper,
    desktop: FakeDesktop,
    now: Instant,
}

impl Rig {
    fn new(profile: &Profile) -> Self {
        let desktop = FakeDesktop::default();
        let desk = Arc::new(Desk::new(Box::new(desktop.clone())));
        Self {
            running: RunningMapper::new(desk),
            mapper: Mapper::new(profile),
            desktop,
            now: Instant::now(),
        }
    }

    fn report_after(&mut self, millis: u64, state: ControllerState) -> ControllerState {
        self.now += Duration::from_millis(millis);
        let shown = self.running.report(&self.mapper, &state, self.now);
        self.running.act();
        shown
    }

    fn pointer_over(&mut self, millis: u64, step: u64, state: ControllerState) -> (i32, i32) {
        self.report_after(0, state);
        self.desktop.take();
        for _ in 0..millis / step {
            self.report_after(step, state);
        }
        moved(&self.desktop.take())
    }
}

fn moved(events: &[DesktopEvent]) -> (i32, i32) {
    events.iter().fold((0, 0), |(x, y), event| match event {
        DesktopEvent::Move { x: dx, y: dy } => (x + dx, y + dy),
        _ => (x, y),
    })
}

fn scrolled(events: &[DesktopEvent]) -> (i32, i32) {
    events.iter().fold((0, 0), |(x, y), event| match event {
        DesktopEvent::Scroll { x: dx, y: dy } => (x + dx, y + dy),
        _ => (x, y),
    })
}

fn using(side: StickSide, used_as: StickUse) -> Profile {
    let mut profile = Profile::named("Desk");
    profile.change_stick_use(side, StickUseChange::Use(used_as));
    profile
}

fn left(x: u8, y: u8) -> ControllerState {
    ControllerState {
        left_stick: Stick { x, y },
        ..ControllerState::NEUTRAL
    }
}

fn key(code: u8, down: bool) -> DesktopEvent {
    DesktopEvent::Key {
        key: Key::new(code),
        down,
    }
}

const W: u8 = 0x11;
const A: u8 = 0x1E;
const S: u8 = 0x1F;
const D: u8 = 0x20;

#[test]
fn a_stick_drives_the_virtual_controller_unless_it_is_used_otherwise() {
    let mut rig = Rig::new(&Profile::default());

    let shown = rig.report_after(0, left(255, 0));

    assert_eq!(shown, left(255, 0));
    assert_eq!(rig.desktop.take(), []);
}

#[test]
fn a_stick_used_otherwise_rests_centred_on_the_virtual_controller() {
    for used_as in [
        StickUse::Mouse,
        StickUse::Scroll,
        StickUse::Wasd,
        StickUse::Arrows,
        StickUse::Keys,
    ] {
        let mut rig = Rig::new(&using(StickSide::Left, used_as));
        let state = ControllerState {
            right_stick: Stick { x: 20, y: 200 },
            ..left(255, 0)
        };

        let shown = rig.report_after(0, state);

        assert_eq!(shown.left_stick, Stick::CENTER, "{used_as:?}");
        assert_eq!(shown.right_stick, state.right_stick, "{used_as:?}");
        let report = XboxReport::from_state(&shown);
        assert_eq!((report.thumb_lx, report.thumb_ly), (0, 0), "{used_as:?}");
    }
}

#[test]
fn with_the_sticks_swapped_the_virtual_stick_of_a_used_stick_rests() {
    let mut profile = using(StickSide::Left, StickUse::Mouse);
    profile.swap_sticks = true;
    let mut rig = Rig::new(&profile);
    let state = ControllerState {
        right_stick: Stick { x: 20, y: 200 },
        ..left(255, 0)
    };

    let shown = rig.report_after(0, state);

    assert_eq!(shown.left_stick, state.right_stick);
    assert_eq!(shown.right_stick, Stick::CENTER);
}

#[test]
fn a_stick_pushed_fully_moves_the_pointer_a_thousand_pixels_a_second() {
    let mut rig = Rig::new(&using(StickSide::Left, StickUse::Mouse));

    let (x, y) = rig.pointer_over(1000, 4, left(255, 128));

    assert!((999..=1000).contains(&x), "{x}");
    assert_eq!(y, 0);
}

#[test]
fn the_pointer_moves_as_far_however_often_reports_arrive() {
    let travel = |step| {
        let mut rig = Rig::new(&using(StickSide::Left, StickUse::Mouse));
        rig.pointer_over(1000, step, left(0, 128)).0
    };

    for step in [1, 4, 8, 10, 20] {
        assert!(
            (-1000..=-999).contains(&travel(step)),
            "{step}: {}",
            travel(step)
        );
    }
}

#[test]
fn slow_movement_adds_up_instead_of_being_rounded_away() {
    let mut profile = using(StickSide::Left, StickUse::Mouse);
    profile.change_stick_use(StickSide::Left, StickUseChange::Speed(10));
    profile.change_stick(StickSide::Left, StickChange::Curve(Curve::Softer));
    let mut rig = Rig::new(&profile);

    let (x, _) = rig.pointer_over(10_000, 1, left(192, 128));

    assert!((90..=130).contains(&x), "{x}");
}

#[test]
fn speed_curve_and_dead_zone_shape_the_pointer() {
    let travel = |change: &dyn Fn(&mut Profile), x| {
        let mut profile = using(StickSide::Left, StickUse::Mouse);
        change(&mut profile);
        Rig::new(&profile).pointer_over(1000, 4, left(x, 128)).0
    };
    let fast = travel(
        &|profile| profile.change_stick_use(StickSide::Left, StickUseChange::Speed(200)),
        255,
    );
    let soft = travel(
        &|profile| profile.change_stick(StickSide::Left, StickChange::Curve(Curve::Soft)),
        192,
    );
    let linear = travel(&|_| {}, 192);
    let resting = travel(
        &|profile| profile.change_stick(StickSide::Left, StickChange::DeadZone(30)),
        160,
    );

    assert!((1998..=2000).contains(&fast), "{fast}");
    assert!(soft < linear && soft > 0, "{soft} {linear}");
    assert_eq!(resting, 0);
}

#[test]
fn a_resting_stick_without_a_dead_zone_leaves_the_pointer_still() {
    let mut rig = Rig::new(&using(StickSide::Left, StickUse::Mouse));

    assert_eq!(rig.pointer_over(5000, 4, left(132, 124)), (0, 0));
}

#[test]
fn a_pause_between_reports_does_not_make_the_pointer_jump() {
    let mut rig = Rig::new(&using(StickSide::Left, StickUse::Mouse));
    rig.report_after(0, left(255, 128));
    rig.desktop.take();

    rig.report_after(3000, left(255, 128));

    let (x, _) = moved(&rig.desktop.take());
    assert!((99..=100).contains(&x), "{x}");
}

#[test]
fn a_stick_scrolls_the_way_it_is_pushed() {
    let scroll = |state| {
        let mut rig = Rig::new(&using(StickSide::Left, StickUse::Scroll));
        rig.report_after(0, state);
        rig.desktop.take();
        for _ in 0..250 {
            rig.report_after(4, state);
        }
        scrolled(&rig.desktop.take())
    };

    let (x, up) = scroll(left(128, 0));
    assert_eq!(x, 0);
    assert!((19..=20).contains(&up), "{up}");
    assert!((-20..=-19).contains(&scroll(left(128, 255)).1));
    assert!((19..=20).contains(&scroll(left(255, 128)).0));
}

#[test]
fn a_stick_presses_the_keys_of_its_direction_and_two_on_a_diagonal() {
    let mut rig = Rig::new(&using(StickSide::Left, StickUse::Wasd));

    rig.report_after(0, left(128, 0));
    assert_eq!(rig.desktop.take(), [key(W, true)]);

    rig.report_after(4, left(230, 26));
    assert_eq!(rig.desktop.take(), [key(D, true)]);

    rig.report_after(4, left(128, 255));
    assert_eq!(
        rig.desktop.take(),
        [key(W, false), key(D, false), key(S, true)]
    );

    rig.report_after(4, left(0, 128));
    assert_eq!(rig.desktop.take(), [key(S, false), key(A, true)]);

    rig.report_after(4, left(128, 128));
    assert_eq!(rig.desktop.take(), [key(A, false)]);
}

#[test]
fn a_key_stick_needs_a_quarter_of_its_travel_even_with_a_smaller_dead_zone() {
    let mut rig = Rig::new(&using(StickSide::Left, StickUse::Wasd));

    rig.report_after(0, left(155, 128));
    assert_eq!(rig.desktop.take(), []);

    rig.report_after(4, left(170, 128));
    assert_eq!(rig.desktop.take(), [key(D, true)]);
}

#[test]
fn arrows_press_the_arrow_keys() {
    let mut rig = Rig::new(&using(StickSide::Right, StickUse::Arrows));
    let up = ControllerState {
        right_stick: Stick { x: 128, y: 0 },
        ..ControllerState::NEUTRAL
    };

    rig.report_after(0, up);

    assert_eq!(
        rig.desktop.take(),
        [DesktopEvent::Key {
            key: Key::extended(0x48),
            down: true
        }]
    );
}

#[test]
fn four_chosen_keys_follow_the_stick() {
    let mut profile = using(StickSide::Left, StickUse::Keys);
    let chosen = Chord::new(&[Key::LEFT_SHIFT, Key::new(W)]).unwrap();
    profile.change_stick_use(StickSide::Left, StickUseChange::Key(Direction::Up, chosen));
    let mut rig = Rig::new(&profile);

    rig.report_after(0, left(128, 0));
    assert_eq!(
        rig.desktop.take(),
        [
            DesktopEvent::Key {
                key: Key::LEFT_SHIFT,
                down: true
            },
            key(W, true)
        ]
    );
    rig.report_after(4, left(255, 128));
    assert_eq!(
        rig.desktop.take(),
        [
            key(W, false),
            DesktopEvent::Key {
                key: Key::LEFT_SHIFT,
                down: false
            }
        ]
    );
}

#[test]
fn a_stick_use_survives_a_save_and_load() {
    let mut profile = using(StickSide::Left, StickUse::Keys);
    profile.change_stick_use(
        StickSide::Left,
        StickUseChange::Key(Direction::Left, Chord::of(Key::new(A))),
    );
    profile.change_stick_use(StickSide::Right, StickUseChange::Use(StickUse::Mouse));
    profile.change_stick_use(StickSide::Right, StickUseChange::Speed(150));

    let loaded = Profile::load(&profile.to_json(), "ignored");

    assert_eq!(loaded, Some(profile));
}

#[test]
fn a_profile_whose_sticks_are_sticks_writes_nothing_for_their_use() {
    let json: serde_json::Value = serde_json::from_str(&Profile::named("Plain").to_json()).unwrap();

    assert!(json.get("left_stick_use").is_none());
    assert!(json.get("right_stick_use").is_none());
}

#[test]
fn a_stick_use_names_its_fields_in_the_file() {
    let mut profile = using(StickSide::Left, StickUse::Scroll);
    profile.change_stick_use(StickSide::Left, StickUseChange::Speed(150));

    let json: serde_json::Value = serde_json::from_str(&profile.to_json()).unwrap();

    assert_eq!(
        json["left_stick_use"],
        serde_json::json!({"use": "scroll", "speed": 150})
    );
}

#[test]
fn a_broken_stick_use_falls_back_field_by_field() {
    let text = r#"{
        "left_stick_use": {"use": "mouse", "speed": 5000, "keys": {"up": [0], "left": [30]}},
        "right_stick_use": {"use": "joystick", "speed": "fast"}
    }"#;

    let profile = Profile::load(text, "Desk").unwrap();

    let left = profile.stick_use(StickSide::Left);
    assert_eq!(left.used_as, StickUse::Mouse);
    assert_eq!(left.speed, 300);
    assert_eq!(left.key(Direction::Up), None);
    assert_eq!(left.key(Direction::Left), Some(Chord::of(Key::new(A))));
    assert_eq!(*profile.stick_use(StickSide::Right), StickRole::default());
}

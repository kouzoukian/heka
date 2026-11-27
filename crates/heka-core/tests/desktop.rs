use std::sync::{Arc, Mutex};

use heka_core::desktop::{
    Chord, Desk, DesktopError, DesktopEvent, DesktopOutput, DesktopState, Hands, Key, MouseButton,
    WheelStep,
};

const W: Key = Key::new(0x11);
const C: Key = Key::new(0x2E);
const V: Key = Key::new(0x2F);

#[derive(Default)]
struct Record {
    sent: Vec<DesktopEvent>,
    refusing: bool,
}

#[derive(Clone, Default)]
struct FakeDesktop(Arc<Mutex<Record>>);

impl FakeDesktop {
    fn take(&self) -> Vec<DesktopEvent> {
        std::mem::take(&mut self.0.lock().unwrap().sent)
    }

    fn refuse(&self, refusing: bool) {
        self.0.lock().unwrap().refusing = refusing;
    }
}

impl DesktopOutput for FakeDesktop {
    fn send(&mut self, events: &[DesktopEvent]) -> Result<(), DesktopError> {
        let mut record = self.0.lock().unwrap();
        if record.refusing {
            return Err(DesktopError::new("access is denied"));
        }
        record.sent.extend_from_slice(events);
        Ok(())
    }
}

fn desk() -> (Arc<Desk>, FakeDesktop) {
    let desktop = FakeDesktop::default();
    (Arc::new(Desk::new(Box::new(desktop.clone()))), desktop)
}

fn keys(chords: &[Chord]) -> DesktopState {
    let mut held = DesktopState::default();
    for &chord in chords {
        held.add_chord(chord);
    }
    held
}

fn key(key: Key) -> DesktopState {
    keys(&[Chord::of(key)])
}

fn down(key: Key) -> DesktopEvent {
    DesktopEvent::Key { key, down: true }
}

fn up(key: Key) -> DesktopEvent {
    DesktopEvent::Key { key, down: false }
}

fn button(button: MouseButton, down: bool) -> DesktopEvent {
    DesktopEvent::Button { button, down }
}

#[test]
fn a_held_key_is_pressed_once_and_released_when_let_go() {
    let (desk, desktop) = desk();
    let mut hands = Hands::new(desk);

    hands.hold(key(W));
    hands.hold(key(W));
    assert_eq!(desktop.take(), [down(W)]);

    hands.hold(DesktopState::default());
    assert_eq!(desktop.take(), [up(W)]);
}

#[test]
fn a_combination_presses_its_modifiers_first_and_releases_them_last() {
    let (desk, desktop) = desk();
    let mut hands = Hands::new(desk);
    let chord = Chord::new(&[Key::LEFT_CTRL, Key::LEFT_SHIFT, W]).unwrap();

    hands.hold(keys(&[chord]));
    assert_eq!(
        desktop.take(),
        [down(Key::LEFT_CTRL), down(Key::LEFT_SHIFT), down(W)]
    );

    hands.hold(DesktopState::default());
    assert_eq!(
        desktop.take(),
        [up(W), up(Key::LEFT_CTRL), up(Key::LEFT_SHIFT)]
    );
}

#[test]
fn a_modifier_shared_by_two_combinations_stays_held_between_them() {
    let (desk, desktop) = desk();
    let mut hands = Hands::new(desk);
    let copy = Chord::new(&[Key::LEFT_CTRL, C]).unwrap();
    let paste = Chord::new(&[Key::LEFT_CTRL, V]).unwrap();

    hands.hold(keys(&[copy]));
    desktop.take();
    hands.hold(keys(&[paste]));

    assert_eq!(desktop.take(), [up(C), down(V)]);
}

#[test]
fn a_key_two_hands_hold_is_released_only_when_both_let_go() {
    let (desk, desktop) = desk();
    let mut first = Hands::new(Arc::clone(&desk));
    let mut second = Hands::new(desk);

    first.hold(key(W));
    second.hold(key(W));
    assert_eq!(desktop.take(), [down(W)]);

    first.hold(DesktopState::default());
    assert_eq!(desktop.take(), []);

    second.hold(DesktopState::default());
    assert_eq!(desktop.take(), [up(W)]);
}

#[test]
fn a_mouse_button_is_held_and_shared_like_a_key() {
    let (desk, desktop) = desk();
    let mut first = Hands::new(Arc::clone(&desk));
    let mut second = Hands::new(desk);
    let mut held = DesktopState::default();
    held.add_button(MouseButton::Right);

    first.hold(held);
    second.hold(held);
    first.let_go();
    assert_eq!(desktop.take(), [button(MouseButton::Right, true)]);

    second.let_go();
    assert_eq!(desktop.take(), [button(MouseButton::Right, false)]);
}

#[test]
fn every_new_press_of_a_wheel_binding_turns_the_wheel_one_step() {
    let (desk, desktop) = desk();
    let mut hands = Hands::new(desk);
    let mut first = DesktopState::default();
    first.add_wheel(0, WheelStep::Up);
    let mut both = first;
    both.add_wheel(1, WheelStep::Up);

    hands.hold(first);
    hands.hold(first);
    assert_eq!(desktop.take(), [DesktopEvent::Wheel(WheelStep::Up)]);

    hands.hold(both);
    assert_eq!(desktop.take(), [DesktopEvent::Wheel(WheelStep::Up)]);

    hands.hold(DesktopState::default());
    assert_eq!(desktop.take(), []);

    hands.hold(first);
    assert_eq!(desktop.take(), [DesktopEvent::Wheel(WheelStep::Up)]);
}

#[test]
fn letting_go_releases_everything_and_does_not_turn_the_wheel_again() {
    let (desk, desktop) = desk();
    let mut hands = Hands::new(desk);
    let mut held = key(W);
    held.add_button(MouseButton::Left);
    held.add_wheel(3, WheelStep::Down);

    hands.hold(held);
    desktop.take();
    hands.let_go();
    assert_eq!(desktop.take(), [button(MouseButton::Left, false), up(W)]);

    hands.hold(held);
    assert_eq!(desktop.take(), [down(W), button(MouseButton::Left, true)]);
}

#[test]
fn dropped_hands_release_what_they_held() {
    let (desk, desktop) = desk();
    let mut hands = Hands::new(desk);
    hands.hold(key(W));
    desktop.take();

    drop(hands);

    assert_eq!(desktop.take(), [up(W)]);
}

#[test]
fn a_release_the_system_refuses_is_tried_again() {
    let (desk, desktop) = desk();
    let mut hands = Hands::new(Arc::clone(&desk));
    hands.hold(key(W));
    desktop.take();

    desktop.refuse(true);
    hands.let_go();
    desk.settle();
    desktop.refuse(false);
    assert_eq!(desktop.take(), []);

    desk.settle();
    assert_eq!(desktop.take(), [up(W)]);

    desk.settle();
    assert_eq!(desktop.take(), []);
}

#[test]
fn a_refused_release_is_forgotten_once_the_key_is_held_again() {
    let (desk, desktop) = desk();
    let mut hands = Hands::new(Arc::clone(&desk));
    hands.hold(key(W));
    desktop.take();
    desktop.refuse(true);
    hands.let_go();
    desktop.refuse(false);

    hands.hold(key(W));
    desk.settle();

    assert_eq!(desktop.take(), []);
}

#[test]
fn a_refusal_is_reported_once_until_the_system_accepts_again() {
    let (desk, desktop) = desk();
    let mut hands = Hands::new(Arc::clone(&desk));
    desktop.refuse(true);
    hands.hold(key(W));
    hands.hold(key(C));

    assert_eq!(desk.take_failure().as_deref(), Some("access is denied"));
    assert_eq!(desk.take_failure(), None);
    hands.let_go();
    assert_eq!(desk.take_failure(), None);

    desktop.refuse(false);
    desk.settle();
    desktop.refuse(true);
    hands.hold(key(V));
    assert_eq!(desk.take_failure().as_deref(), Some("access is denied"));
}

#[test]
fn only_modifiers_may_come_before_the_key_of_a_combination() {
    assert_eq!(Chord::new(&[]), None);
    assert_eq!(Chord::new(&[W, C]), None);
    assert_eq!(Chord::new(&[Key::LEFT_ALT, W]).map(|c| c.key()), Some(W));
}

#[test]
fn the_keys_of_a_combination_are_listed_modifiers_first() {
    let chord = Chord::new(&[Key::LEFT_SHIFT, Key::LEFT_CTRL, W]).unwrap();

    assert_eq!(
        chord.keys().collect::<Vec<_>>(),
        [Key::LEFT_CTRL, Key::LEFT_SHIFT, W]
    );
}

#[test]
fn a_modifier_can_be_the_key_itself() {
    let chord = Chord::new(&[Key::LEFT_SHIFT, Key::LEFT_SHIFT]).unwrap();

    assert_eq!(chord, Chord::of(Key::LEFT_SHIFT));
}

#[test]
fn a_key_is_its_scan_code_with_the_extended_prefix() {
    assert_eq!(Key::from_scan(0x11), Some(W));
    assert_eq!(Key::from_scan(0xE01D), Some(Key::RIGHT_CTRL));
    assert_eq!(Key::RIGHT_CTRL.scan(), 0xE01D);
    assert_eq!(Key::RIGHT_CTRL.code(), 0x1D);
    assert!(Key::RIGHT_CTRL.is_extended());
    assert!(!Key::LEFT_CTRL.is_extended());
    assert_eq!(Key::from_scan(0), None);
    assert_eq!(Key::from_scan(0xE000), None);
    assert_eq!(Key::from_scan(0x1234), None);
}

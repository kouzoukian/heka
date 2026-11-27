use heka_core::capture::{Captured, KeyCapture, KeyStroke};
use heka_core::desktop::{Chord, Key};

const M: Key = Key::new(0x32);
const Q: Key = Key::new(0x10);

fn heard(strokes: &[(Key, bool)]) -> Vec<Option<Captured>> {
    let mut capture = KeyCapture::default();
    strokes
        .iter()
        .map(|&(key, down)| capture.hear(KeyStroke { key, down }))
        .collect()
}

fn ends_with(strokes: &[(Key, bool)]) -> Option<Captured> {
    let mut results = heard(strokes);
    let last = results.pop().expect("at least one stroke");
    assert!(
        results.iter().all(Option::is_none),
        "captured before every key was let go: {results:?}"
    );
    last
}

fn chord(keys: &[Key]) -> Option<Captured> {
    Some(Captured::Chord(Chord::new(keys).unwrap()))
}

#[test]
fn a_key_is_captured_once_it_is_let_go() {
    assert_eq!(ends_with(&[(M, true), (M, false)]), chord(&[M]));
}

#[test]
fn modifiers_held_with_a_key_make_a_combination() {
    let strokes = [
        (Key::LEFT_CTRL, true),
        (Key::LEFT_SHIFT, true),
        (M, true),
        (Key::LEFT_CTRL, false),
        (M, false),
        (Key::LEFT_SHIFT, false),
    ];

    assert_eq!(
        ends_with(&strokes),
        chord(&[Key::LEFT_CTRL, Key::LEFT_SHIFT, M])
    );
}

#[test]
fn a_modifier_pressed_alone_is_captured_as_the_key() {
    assert_eq!(
        ends_with(&[(Key::RIGHT_ALT, true), (Key::RIGHT_ALT, false)]),
        chord(&[Key::RIGHT_ALT])
    );
}

#[test]
fn the_first_modifier_let_go_is_the_key_of_a_combination_of_modifiers() {
    let strokes = [
        (Key::LEFT_CTRL, true),
        (Key::LEFT_SHIFT, true),
        (Key::LEFT_SHIFT, false),
        (Key::LEFT_CTRL, false),
    ];

    assert_eq!(
        ends_with(&strokes),
        chord(&[Key::LEFT_CTRL, Key::LEFT_SHIFT])
    );
}

#[test]
fn only_the_first_key_counts() {
    let strokes = [(M, true), (Q, true), (M, false), (Q, false)];

    assert_eq!(ends_with(&strokes), chord(&[M]));
}

#[test]
fn escape_alone_cancels() {
    assert_eq!(
        ends_with(&[(Key::ESCAPE, true), (Key::ESCAPE, false)]),
        Some(Captured::Cancelled)
    );
}

#[test]
fn escape_with_a_modifier_is_a_combination() {
    let strokes = [
        (Key::LEFT_SHIFT, true),
        (Key::ESCAPE, true),
        (Key::ESCAPE, false),
        (Key::LEFT_SHIFT, false),
    ];

    assert_eq!(ends_with(&strokes), chord(&[Key::LEFT_SHIFT, Key::ESCAPE]));
}

#[test]
fn a_release_of_a_key_held_before_the_capture_is_ignored() {
    assert_eq!(heard(&[(Q, false)]), [None]);
    assert_eq!(ends_with(&[(Q, false), (M, true), (M, false)]), chord(&[M]));
}

#[test]
fn repeated_presses_of_a_held_key_change_nothing() {
    let strokes = [
        (Key::LEFT_CTRL, true),
        (Key::LEFT_CTRL, true),
        (Key::LEFT_CTRL, false),
    ];

    assert_eq!(ends_with(&strokes), chord(&[Key::LEFT_CTRL]));
}

#[test]
fn print_screen_is_captured_although_windows_only_reports_its_release() {
    assert_eq!(
        ends_with(&[(Key::PRINT_SCREEN, false)]),
        chord(&[Key::PRINT_SCREEN])
    );
}

#[test]
fn a_capture_starts_afresh_after_it_ends() {
    let strokes = [(M, true), (M, false), (Q, true), (Q, false)];

    assert_eq!(heard(&strokes), [None, chord(&[M]), None, chord(&[Q])]);
}

fn with_alt_gr(strokes: &[(Key, bool)]) -> Vec<Captured> {
    let mut capture = KeyCapture::new(true);
    strokes
        .iter()
        .filter_map(|&(key, down)| capture.hear(KeyStroke { key, down }))
        .collect()
}

#[test]
fn the_ctrl_windows_adds_to_alt_gr_is_not_captured() {
    let alone = [
        (Key::LEFT_CTRL, true),
        (Key::RIGHT_ALT, true),
        (Key::LEFT_CTRL, false),
        (Key::RIGHT_ALT, false),
    ];
    let with_a_key = [
        (Key::LEFT_CTRL, true),
        (Key::RIGHT_ALT, true),
        (Q, true),
        (Q, false),
        (Key::LEFT_CTRL, false),
        (Key::RIGHT_ALT, false),
    ];

    assert_eq!(with_alt_gr(&alone), [chord(&[Key::RIGHT_ALT]).unwrap()]);
    assert_eq!(
        with_alt_gr(&with_a_key),
        [chord(&[Key::RIGHT_ALT, Q]).unwrap()]
    );
}

#[test]
fn a_layout_without_alt_gr_keeps_ctrl_with_the_right_alt() {
    let strokes = [
        (Key::LEFT_CTRL, true),
        (Key::RIGHT_ALT, true),
        (Q, true),
        (Q, false),
        (Key::LEFT_CTRL, false),
        (Key::RIGHT_ALT, false),
    ];

    assert_eq!(
        ends_with(&strokes),
        chord(&[Key::LEFT_CTRL, Key::RIGHT_ALT, Q])
    );
}

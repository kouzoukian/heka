use heka_app::text;
use heka_core::desktop::{Chord, Key, MouseButton, WheelStep};
use heka_core::profile::{Binding, Input, Output};

fn unlabelled(_: Key) -> Option<String> {
    None
}

fn words(binding: Binding) -> String {
    text::binding_words(binding, &unlabelled)
}

fn keys(keys: &[Key]) -> Binding {
    Binding::Keys(Chord::new(keys).unwrap())
}

#[test]
fn a_key_is_named_after_it() {
    assert_eq!(words(keys(&[Key::new(0x32)])), "Key M");
    assert_eq!(words(keys(&[Key::LEFT_SHIFT])), "Key Left Shift");
    assert_eq!(words(keys(&[Key::extended(0x48)])), "Key Up");
    assert_eq!(words(keys(&[Key::new(0x48)])), "Key Num 8");
    assert_eq!(words(keys(&[Key::new(0x3B)])), "Key F1");
}

#[test]
fn a_combination_lists_its_modifiers_first_and_shortly() {
    let combination = keys(&[Key::LEFT_SHIFT, Key::LEFT_CTRL, Key::new(0x32)]);
    let with_the_right_one = keys(&[Key::RIGHT_CTRL, Key::ESCAPE]);

    assert_eq!(words(combination), "Key Ctrl + Shift + M");
    assert_eq!(words(with_the_right_one), "Key Right Ctrl + Esc");
}

#[test]
fn a_character_key_takes_the_name_the_keyboard_layout_gives_it() {
    let layout = |_: Key| Some("Ç".to_owned());
    let semicolon = keys(&[Key::new(0x27)]);
    let enter = keys(&[Key::new(0x1C)]);

    assert_eq!(text::binding_words(semicolon, &layout), "Key Ç");
    assert_eq!(text::binding_words(semicolon, &unlabelled), "Key ;");
    assert_eq!(text::binding_words(enter, &layout), "Key Enter");
}

#[test]
fn the_us_names_follow_the_rows_of_the_keyboard() {
    let named = |code| words(keys(&[Key::new(code)]));

    assert_eq!(named(0x02), "Key 1");
    assert_eq!(named(0x0B), "Key 0");
    assert_eq!(named(0x10), "Key Q");
    assert_eq!(named(0x1E), "Key A");
    assert_eq!(named(0x29), "Key `");
    assert_eq!(named(0x2B), "Key \\");
    assert_eq!(named(0x2C), "Key Z");
    assert_eq!(named(0x35), "Key /");
}

#[test]
fn a_key_without_a_name_is_told_by_its_scan_code() {
    assert_eq!(words(keys(&[Key::extended(0x21)])), "Key E021");
}

#[test]
fn the_mouse_and_the_other_bindings_are_worded() {
    assert_eq!(words(Binding::Mouse(MouseButton::Forward)), "Mouse forward");
    assert_eq!(words(Binding::Wheel(WheelStep::Down)), "Wheel down");
    assert_eq!(words(Binding::To(Output::Circle)), "Circle");
    assert_eq!(words(Binding::Nothing), "Nothing");
    assert_eq!(words(Binding::Default), "Default");
    assert_eq!(text::input_name(Input::Cross), "Cross");
}

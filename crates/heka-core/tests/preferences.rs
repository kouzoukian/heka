use std::time::Duration;

use heka_core::feedback::{FLASH_PERIOD, Feedback, Lightbar, PlayerLights, RAINBOW_PERIOD, Rumble};
use heka_core::preferences::{
    Conditions, LightbarChoice, MuteLight, PreferenceChange, Preferences, RumbleStrength,
};
use heka_core::state::{Battery, Charge};

const PINK: Lightbar = Lightbar {
    red: 255,
    green: 0,
    blue: 128,
};
const GREEN: Lightbar = Lightbar {
    red: 0,
    green: 255,
    blue: 0,
};

fn after(elapsed: Duration) -> Conditions {
    Conditions {
        elapsed,
        ..Conditions::default()
    }
}

fn coloured(lightbar: Option<Lightbar>) -> Feedback {
    Feedback {
        lightbar,
        ..Feedback::OFF
    }
}

#[test]
fn the_default_preferences_change_nothing() {
    for lightbar in [None, Some(Lightbar::DARK), Some(GREEN)] {
        let asked = Feedback {
            rumble: Rumble {
                large: 255,
                small: 1,
            },
            lightbar,
            ..Feedback::OFF
        };
        assert_eq!(
            Preferences::default().apply(asked, Conditions::default()),
            asked
        );
    }
}

#[test]
fn rumble_strength_scales_both_motors_to_the_nearest_step() {
    let full = Rumble {
        large: 255,
        small: 3,
    };
    let at = |percent| RumbleStrength::percent(percent).scale(full);

    assert_eq!(at(100), full);
    assert_eq!(
        at(50),
        Rumble {
            large: 128,
            small: 2
        }
    );
    assert_eq!(at(0), Rumble::OFF);
    assert_eq!(at(200), full, "more than 100% counts as 100%");
}

#[test]
fn swapping_the_motors_exchanges_the_games_large_and_small_rumble() {
    let preferences = Preferences {
        rumble: RumbleStrength::percent(50),
        swap_motors: true,
        ..Preferences::default()
    };
    let asked = Feedback {
        rumble: Rumble {
            large: 200,
            small: 64,
        },
        ..Feedback::OFF
    };

    assert_eq!(
        preferences.apply(asked, Conditions::default()).rumble,
        Rumble {
            large: 32,
            small: 100
        }
    );
}

#[test]
fn the_chosen_colour_shows_while_no_game_has_set_one() {
    let preferences = Preferences {
        lightbar: Some(LightbarChoice::Colour(PINK)),
        ..Preferences::default()
    };
    let shown = |asked| {
        preferences
            .apply(coloured(asked), Conditions::default())
            .lightbar
    };

    assert_eq!(shown(None), Some(PINK));
    assert_eq!(shown(Some(Lightbar::DARK)), Some(PINK));
    assert_eq!(shown(Some(GREEN)), Some(GREEN));
}

#[test]
fn the_player_lights_are_the_players_choice() {
    let preferences = Preferences {
        player_lights: Some(PlayerLights::Three),
        ..Preferences::default()
    };
    assert_eq!(
        preferences
            .apply(Feedback::OFF, Conditions::default())
            .player_lights,
        Some(PlayerLights::Three)
    );
}

#[test]
fn the_rainbow_goes_round_the_colours_and_starts_again() {
    let at = |sixths| {
        let colour = Lightbar::rainbow(RAINBOW_PERIOD * sixths / 6);
        (colour.red, colour.green, colour.blue)
    };

    assert_eq!(at(0), (255, 0, 0));
    assert_eq!(at(1), (255, 255, 0));
    assert_eq!(at(2), (0, 255, 0));
    assert_eq!(at(3), (0, 255, 255));
    assert_eq!(at(4), (0, 0, 255));
    assert_eq!(at(5), (255, 0, 255));
    assert_eq!(at(6), at(0));
}

#[test]
fn the_rainbow_shows_while_no_game_has_set_a_colour() {
    let preferences = Preferences {
        lightbar: Some(LightbarChoice::RAINBOW),
        ..Preferences::default()
    };
    let later = RAINBOW_PERIOD / 3;
    let shown = |asked| preferences.apply(coloured(asked), after(later)).lightbar;

    assert_eq!(shown(None), Some(Lightbar::rainbow(later)));
    assert_eq!(shown(Some(Lightbar::DARK)), Some(Lightbar::rainbow(later)));
    assert_eq!(shown(Some(GREEN)), Some(GREEN));
}

const fn battery(percent: u8, charge: Charge) -> Option<Battery> {
    Some(Battery { percent, charge })
}

fn with_battery(battery: Option<Battery>, elapsed: Duration) -> Conditions {
    Conditions {
        elapsed,
        battery,
        mute_toggled: false,
    }
}

fn battery_coloured() -> Preferences {
    Preferences {
        battery_colour: true,
        lightbar: Some(LightbarChoice::Colour(PINK)),
        ..Preferences::default()
    }
}

#[test]
fn the_battery_colour_goes_from_red_through_yellow_to_green() {
    let at = |percent| {
        let colour = Lightbar::of_battery(percent);
        (colour.red, colour.green, colour.blue)
    };

    assert_eq!(at(0), (255, 0, 0));
    assert_eq!(at(25), (255, 127, 0));
    assert_eq!(at(50), (255, 255, 0));
    assert_eq!(at(75), (127, 255, 0));
    assert_eq!(at(100), (0, 255, 0));
}

#[test]
fn the_battery_colour_takes_the_place_of_the_chosen_one_and_a_games_colour_wins() {
    let conditions = with_battery(battery(55, Charge::Charging), Duration::ZERO);
    let shown = |asked| {
        battery_coloured()
            .apply(coloured(asked), conditions)
            .lightbar
    };

    assert_eq!(shown(None), Some(Lightbar::of_battery(55)));
    assert_eq!(shown(Some(Lightbar::DARK)), Some(Lightbar::of_battery(55)));
    assert_eq!(shown(Some(GREEN)), Some(GREEN));
}

#[test]
fn an_unknown_battery_leaves_the_chosen_colour() {
    for unknown in [
        None,
        battery(0, Charge::Unknown),
        battery(0, Charge::NotCharging),
    ] {
        let conditions = with_battery(unknown, Duration::ZERO);
        let shown = battery_coloured().apply(Feedback::OFF, conditions);
        assert_eq!(shown.lightbar, Some(PINK), "{unknown:?}");
    }
}

fn flashing() -> Preferences {
    Preferences {
        low_battery_flash: true,
        ..Preferences::default()
    }
}

#[test]
fn a_low_battery_flashes_the_colour_that_would_show_a_games_too() {
    let low = battery(15, Charge::Discharging);
    let at = |elapsed| {
        flashing()
            .apply(coloured(Some(GREEN)), with_battery(low, elapsed))
            .lightbar
    };

    assert_eq!(at(Duration::ZERO), Some(GREEN));
    assert_eq!(at(FLASH_PERIOD / 2), Some(Lightbar::DARK));
    assert_eq!(at(FLASH_PERIOD), Some(GREEN));
}

#[test]
fn a_flash_with_no_colour_to_show_uses_the_default_blue() {
    let low = battery(5, Charge::Discharging);
    for asked in [None, Some(Lightbar::DARK)] {
        let shown = flashing().apply(coloured(asked), with_battery(low, Duration::ZERO));
        assert_eq!(shown.lightbar, Some(Lightbar::DEFAULT));
    }
}

#[test]
fn only_twenty_percent_or_less_while_not_charging_flashes() {
    let flashes = |battery| {
        let conditions = with_battery(battery, FLASH_PERIOD / 2);
        flashing().apply(Feedback::OFF, conditions).lightbar == Some(Lightbar::DARK)
    };

    assert!(flashes(battery(20, Charge::Discharging)));
    assert!(flashes(battery(5, Charge::Discharging)));
    assert!(!flashes(battery(25, Charge::Discharging)));
    assert!(!flashes(battery(15, Charge::Charging)));
    assert!(!flashes(battery(0, Charge::NotCharging)));
    assert!(!flashes(battery(0, Charge::Unknown)));
    assert!(!flashes(None));
}

#[test]
fn the_low_battery_flash_is_off_until_chosen() {
    let conditions = with_battery(battery(5, Charge::Discharging), FLASH_PERIOD / 2);
    let shown = Preferences::default().apply(Feedback::OFF, conditions);
    assert_eq!(shown, Feedback::OFF);
}

#[test]
fn the_mute_light_follows_the_choice() {
    let lit = |mute_light, mute_toggled| {
        let preferences = Preferences {
            mute_light,
            ..Preferences::default()
        };
        let conditions = Conditions {
            mute_toggled,
            ..Conditions::default()
        };
        preferences.apply(Feedback::OFF, conditions).mute_light
    };

    assert_eq!(lit(None, true), None);
    assert_eq!(lit(Some(MuteLight::Off), true), Some(false));
    assert_eq!(lit(Some(MuteLight::On), false), Some(true));
    assert_eq!(lit(Some(MuteLight::Button), false), Some(false));
    assert_eq!(lit(Some(MuteLight::Button), true), Some(true));
}

#[test]
fn each_light_choice_changes_alone() {
    let mut preferences = Preferences::default();
    preferences.change(PreferenceChange::BatteryColour(true));
    preferences.change(PreferenceChange::LowBatteryFlash(true));
    preferences.change(PreferenceChange::MuteLight(Some(MuteLight::Off)));

    assert_eq!(
        preferences,
        Preferences {
            battery_colour: true,
            low_battery_flash: true,
            mute_light: Some(MuteLight::Off),
            ..Preferences::default()
        }
    );
}

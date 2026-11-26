use std::time::Duration;

use heka_core::feedback::{Feedback, Lightbar, PlayerLights, RAINBOW_PERIOD, Rumble};
use heka_core::preferences::{LightbarChoice, Preferences, RumbleStrength};

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
            player_lights: None,
        };
        assert_eq!(Preferences::default().apply(asked, Duration::ZERO), asked);
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
fn the_chosen_colour_shows_while_no_game_has_set_one() {
    let preferences = Preferences {
        lightbar: Some(LightbarChoice::Colour(PINK)),
        ..Preferences::default()
    };
    let shown = |asked| preferences.apply(coloured(asked), Duration::ZERO).lightbar;

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
            .apply(Feedback::OFF, Duration::ZERO)
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
    let shown = |asked| preferences.apply(coloured(asked), later).lightbar;

    assert_eq!(shown(None), Some(Lightbar::rainbow(later)));
    assert_eq!(shown(Some(Lightbar::DARK)), Some(Lightbar::rainbow(later)));
    assert_eq!(shown(Some(GREEN)), Some(GREEN));
}

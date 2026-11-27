use heka_core::app_settings::{AppChange, AppSettings, CloseAction};
use heka_core::mac::MacAddress;
use heka_core::settings::Settings;
use heka_core::window_placement::{Monitor, Rect, WindowPlacement};

const MAC: MacAddress = MacAddress([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

const fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn placed(bounds: Rect) -> WindowPlacement {
    WindowPlacement {
        bounds,
        maximized: false,
        monitor: None,
    }
}

fn monitor(id: &str, area: Rect) -> Monitor {
    Monitor {
        id: Some(id.into()),
        area,
    }
}

fn app(text: &str) -> AppSettings {
    Settings::load(Some(text)).app().clone()
}

#[test]
fn heka_keeps_running_and_opens_its_window_by_default() {
    let settings = Settings::load(None);

    assert_eq!(
        *settings.app(),
        AppSettings {
            start_in_tray: false,
            on_close: CloseAction::KeepRunning,
            check_for_releases: true,
            game_rules: Default::default(),
            window: None,
        }
    );
}

#[test]
fn a_v1_file_loads_with_default_app_settings_and_its_controllers() {
    let text = r#"{
        "controllers": {
            "11:22:33:44:55:66": {
                "stopped": true,
                "output_type": "ps4",
                "nickname": "Sofa",
                "rumble": 50,
                "hide": false
            }
        }
    }"#;
    let settings = Settings::load(Some(text));

    assert_eq!(*settings.app(), AppSettings::default());
    let controller = settings.controller(MAC);
    assert!(controller.stopped);
    assert_eq!(controller.nickname.as_deref(), Some("Sofa"));
    assert!(!controller.preferences.hide);
}

#[test]
fn app_settings_survive_a_save_and_load() {
    let mut settings = Settings::default();
    settings.set_app(AppSettings {
        start_in_tray: true,
        on_close: CloseAction::Quit,
        check_for_releases: false,
        game_rules: Default::default(),
        window: Some(WindowPlacement {
            bounds: rect(-1200, 40, 900, 700),
            maximized: true,
            monitor: Some("second".into()),
        }),
    });
    settings.set_stopped(MAC, true);

    assert_eq!(Settings::load(Some(&settings.to_json())), settings);
}

#[test]
fn the_file_names_the_app_settings() {
    let text = r#"{"app": {
        "start_in_tray": true,
        "on_close": "quit",
        "check_for_releases": false,
        "window": {"x": 10, "y": 20, "width": 700, "height": 600}
    }}"#;

    assert_eq!(
        app(text),
        AppSettings {
            start_in_tray: true,
            on_close: CloseAction::Quit,
            check_for_releases: false,
            game_rules: Default::default(),
            window: Some(placed(rect(10, 20, 700, 600))),
        }
    );
}

#[test]
fn a_broken_app_setting_leaves_the_others() {
    let text = r#"{"app": {
        "start_in_tray": true,
        "on_close": "minimize",
        "check_for_releases": "no",
        "window": {"x": "left"},
        "future": 1
    }}"#;

    assert_eq!(
        app(text),
        AppSettings {
            start_in_tray: true,
            ..AppSettings::default()
        }
    );
    assert_eq!(
        app(r#"{"app": {"start_in_tray": "yes", "on_close": "quit"}}"#),
        AppSettings {
            on_close: CloseAction::Quit,
            ..AppSettings::default()
        }
    );
}

#[test]
fn a_broken_app_section_gives_defaults_and_keeps_the_controllers() {
    let text = r#"{"app": 3, "controllers": {"11:22:33:44:55:66": {"stopped": true}}}"#;
    let settings = Settings::load(Some(text));

    assert_eq!(*settings.app(), AppSettings::default());
    assert!(settings.controller(MAC).stopped);
}

#[test]
fn a_window_without_a_size_is_not_remembered() {
    for window in [
        r#"{"x": 0, "y": 0, "width": 0, "height": 600}"#,
        r#"{"x": 0, "y": 0, "width": 700, "height": 0}"#,
    ] {
        let text = format!(r#"{{"app": {{"window": {window}}}}}"#);
        assert_eq!(app(&text).window, None, "{window}");
    }
}

#[test]
fn a_change_touches_only_what_it_names() {
    let mut app = AppSettings {
        window: Some(placed(rect(1, 2, 700, 600))),
        ..AppSettings::default()
    };

    app.change(AppChange::StartInTray(true));
    app.change(AppChange::OnClose(CloseAction::Quit));
    app.change(AppChange::CheckForReleases(false));

    assert_eq!(
        app,
        AppSettings {
            start_in_tray: true,
            on_close: CloseAction::Quit,
            check_for_releases: false,
            game_rules: Default::default(),
            window: Some(placed(rect(1, 2, 700, 600))),
        }
    );
}

#[test]
fn heka_starts_in_the_tray_when_windows_or_the_setting_asks() {
    let off = AppSettings::default();
    let on = AppSettings {
        start_in_tray: true,
        ..AppSettings::default()
    };

    assert!(!off.opens_in_tray(["heka.exe"]));
    assert!(off.opens_in_tray(["heka.exe", "--tray"]));
    assert!(on.opens_in_tray(["heka.exe"]));
    assert!(on.opens_in_tray(["heka.exe", "--tray"]));
}

const LAPTOP: Rect = rect(0, 0, 1920, 1080);

#[test]
fn a_window_on_a_screen_opens_there() {
    let monitors = [monitor("laptop", LAPTOP)];

    let found = placed(rect(100, 100, 840, 680)).monitor_in(&monitors);

    assert_eq!(found, Some(&monitors[0]));
}

#[test]
fn a_window_reaching_past_an_edge_still_opens_there() {
    let monitors = [monitor("laptop", LAPTOP)];

    for bounds in [
        rect(-300, 100, 840, 680),
        rect(1400, 100, 840, 680),
        rect(100, 700, 840, 680),
        rect(100, -16, 840, 680),
    ] {
        assert!(placed(bounds).monitor_in(&monitors).is_some(), "{bounds:?}");
    }
}

#[test]
fn a_window_mostly_past_an_edge_is_off_screen() {
    let monitors = [monitor("laptop", LAPTOP)];

    for bounds in [
        rect(-500, 100, 840, 680),
        rect(1600, 100, 840, 680),
        rect(100, 800, 840, 680),
        rect(2500, 100, 840, 680),
        rect(-32000, -32000, 840, 680),
    ] {
        assert_eq!(placed(bounds).monitor_in(&monitors), None, "{bounds:?}");
    }
}

#[test]
fn a_window_whose_title_bar_is_above_the_screen_is_off_screen() {
    let monitors = [monitor("laptop", LAPTOP)];

    assert_eq!(placed(rect(100, -17, 840, 680)).monitor_in(&monitors), None);
}

#[test]
fn a_window_may_have_its_title_bar_on_the_screen_above() {
    let monitors = [
        monitor("upper", rect(0, -1080, 1920, 1080)),
        monitor("lower", LAPTOP),
    ];

    let found = placed(rect(100, -200, 840, 680)).monitor_in(&monitors);

    assert_eq!(found, Some(&monitors[1]));
}

#[test]
fn a_window_on_a_monitor_that_is_gone_is_off_screen() {
    let saved = WindowPlacement {
        monitor: Some("second".into()),
        ..placed(rect(2000, 100, 840, 680))
    };

    assert_eq!(saved.monitor_in(&[monitor("laptop", LAPTOP)]), None);
    assert_eq!(saved.monitor_in(&[]), None);
}

#[test]
fn the_saved_screen_wins_where_monitors_overlap() {
    let monitors = [
        monitor("laptop", LAPTOP),
        monitor("sharp", rect(960, 0, 1920, 1080)),
    ];
    let saved = WindowPlacement {
        monitor: Some("sharp".into()),
        ..placed(rect(1000, 100, 840, 680))
    };

    assert_eq!(saved.monitor_in(&monitors), Some(&monitors[1]));
}

#[test]
fn a_renamed_monitor_is_still_found_by_its_place() {
    let monitors = [monitor("new name", LAPTOP)];
    let saved = WindowPlacement {
        monitor: Some("old name".into()),
        ..placed(rect(100, 100, 840, 680))
    };

    assert_eq!(saved.monitor_in(&monitors), Some(&monitors[0]));
}

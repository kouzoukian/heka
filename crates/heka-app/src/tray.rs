use std::sync::mpsc::Receiver;
use std::time::Duration;

use gpui::{App, AsyncApp};
use heka_app::text;
use heka_core::autostart::{Autostart, RunEntry};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

struct Items {
    open: MenuItem,
    start_with_windows: CheckMenuItem,
    quit: MenuItem,
}

pub fn install<E: RunEntry + 'static>(
    cx: &mut App,
    mut autostart: Autostart<E>,
    show_requests: Receiver<()>,
    show: impl Fn(&mut App) + 'static,
) -> Result<(), String> {
    let items = Items {
        open: MenuItem::new(text::TRAY_OPEN, true, None),
        start_with_windows: CheckMenuItem::new(
            text::START_WITH_WINDOWS,
            true,
            autostart.is_enabled(),
            None,
        ),
        quit: MenuItem::new(text::QUIT, true, None),
    };
    let menu = Menu::new();
    menu.append_items(&[
        &items.open,
        &items.start_with_windows,
        &PredefinedMenuItem::separator(),
        &items.quit,
    ])
    .map_err(|error| error.to_string())?;
    let tray = TrayIconBuilder::new()
        .with_icon(icon())
        .with_tooltip(text::APP_NAME)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| error.to_string())?;

    cx.spawn(async move |cx: &mut AsyncApp| {
        run(cx, tray, &items, &mut autostart, &show_requests, &show).await;
        cx.update(|cx| cx.quit());
    })
    .detach();
    Ok(())
}

async fn run<E: RunEntry>(
    cx: &mut AsyncApp,
    _tray: TrayIcon,
    items: &Items,
    autostart: &mut Autostart<E>,
    show_requests: &Receiver<()>,
    show: &impl Fn(&mut App),
) {
    loop {
        cx.background_executor().timer(POLL_INTERVAL).await;
        let mut wants_window = show_requests.try_iter().count() > 0;
        for event in TrayIconEvent::receiver().try_iter() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                wants_window = true;
            }
        }
        for event in MenuEvent::receiver().try_iter() {
            if event.id == items.quit.id() {
                return;
            } else if event.id == items.open.id() {
                wants_window = true;
            } else if event.id == items.start_with_windows.id() {
                let enable = !autostart.is_enabled();
                if let Err(error) = autostart.set_enabled(enable) {
                    eprintln!("heka: cannot change Start with Windows: {error}");
                }
                items.start_with_windows.set_checked(autostart.is_enabled());
            }
        }
        if wants_window {
            cx.update(show);
        }
    }
}

fn icon() -> Icon {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (x.abs_diff(SIZE / 2), y.abs_diff(SIZE / 2));
            let inside = dx < 14 && dy < 14 && dx + dy < 24;
            let bar = (10..22).contains(&x) && (14..18).contains(&y);
            let pixel = match (inside, bar) {
                (true, true) => [0xff, 0xff, 0xff, 0xff],
                (true, false) => [0x4f, 0x8c, 0xff, 0xff],
                (false, _) => [0, 0, 0, 0],
            };
            rgba.extend_from_slice(&pixel);
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("the icon has SIZE * SIZE pixels")
}

use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use gpui::{App, AsyncApp};
use heka_app::text;
use heka_app::ui::model::{Backend, Command};
use heka_app::ui::view::Page;
use heka_windows::metrics;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::window_backend::WindowBackend;

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const APP_ICON: u16 = 1;

pub enum Request {
    Show,
    Quit,
}

struct Items {
    open: MenuItem,
    settings: MenuItem,
    start_with_windows: CheckMenuItem,
    quit: MenuItem,
}

pub fn install(
    cx: &mut App,
    backend: Rc<WindowBackend>,
    requests: Receiver<Request>,
    show: impl Fn(&mut App, Option<Page>) + 'static,
) -> Result<(), String> {
    let items = Items {
        open: MenuItem::new(text::TRAY_OPEN, true, None),
        settings: MenuItem::new(text::APP_SETTINGS, true, None),
        start_with_windows: CheckMenuItem::new(
            text::START_WITH_WINDOWS,
            true,
            backend.starts_with_windows(),
            None,
        ),
        quit: MenuItem::new(text::QUIT, true, None),
    };
    let menu = Menu::new();
    menu.append_items(&[
        &items.open,
        &items.settings,
        &PredefinedMenuItem::separator(),
        &items.start_with_windows,
        &items.quit,
    ])
    .map_err(|error| error.to_string())?;
    let tray = TrayIconBuilder::new()
        .with_icon(icon()?)
        .with_tooltip(text::APP_NAME)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| error.to_string())?;

    cx.spawn(async move |cx: &mut AsyncApp| {
        run(cx, tray, &items, &backend, &requests, &show).await;
        cx.update(|cx| cx.quit());
    })
    .detach();
    Ok(())
}

async fn run(
    cx: &mut AsyncApp,
    _tray: TrayIcon,
    items: &Items,
    backend: &WindowBackend,
    requests: &Receiver<Request>,
    show: &impl Fn(&mut App, Option<Page>),
) {
    loop {
        cx.background_executor().timer(POLL_INTERVAL).await;
        let mut wants_window = false;
        let mut page = None;
        for request in requests.try_iter() {
            match request {
                Request::Quit => return,
                Request::Show => wants_window = true,
            }
        }
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
            } else if event.id == items.settings.id() {
                wants_window = true;
                page = Some(Page::AppSettings);
            } else if event.id == items.start_with_windows.id() {
                let enable = !backend.starts_with_windows();
                backend.command(Command::SetStartWithWindows(enable));
            }
        }
        let enabled = backend.starts_with_windows();
        if items.start_with_windows.is_checked() != enabled {
            items.start_with_windows.set_checked(enabled);
        }
        if wants_window {
            cx.update(|cx| show(cx, page));
        }
    }
}

fn icon() -> Result<Icon, String> {
    let side = metrics::small_icon_size();
    Icon::from_resource(APP_ICON, Some((side, side))).map_err(|error| error.to_string())
}

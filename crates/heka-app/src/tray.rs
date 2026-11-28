use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use gpui::{App, AsyncApp};
use heka_app::tray_model::{self, TrayController};
use heka_app::ui::model::{Backend, Command, ControllerEntry};
use heka_app::ui::view::Page;
use heka_app::{log, text};
use heka_windows::metrics;
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
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

const CONTROLLERS_AT: usize = 3;

struct Row {
    shown: TrayController,
    menu: Submenu,
    toggle: MenuItem,
}

impl Row {
    fn new(shown: TrayController) -> Result<Self, String> {
        let toggle = MenuItem::new(shown.action, true, None);
        let menu = Submenu::new(&shown.label, true);
        menu.append(&toggle).map_err(|error| error.to_string())?;
        Ok(Self {
            shown,
            menu,
            toggle,
        })
    }

    fn show(&mut self, latest: TrayController) {
        if latest.label != self.shown.label {
            self.menu.set_text(&latest.label);
        }
        if latest.action != self.shown.action {
            self.toggle.set_text(latest.action);
        }
        self.shown = latest;
    }
}

struct Controllers {
    menu: Menu,
    separator: PredefinedMenuItem,
    rows: Vec<Row>,
    tooltip: String,
}

impl Controllers {
    fn show(&mut self, tray: &TrayIcon, entries: &[ControllerEntry]) -> Result<(), String> {
        let latest = tray_model::controllers(entries);
        let same = latest.len() == self.rows.len()
            && latest
                .iter()
                .zip(&self.rows)
                .all(|(latest, row)| latest.mac == row.shown.mac);
        if same {
            for (row, latest) in self.rows.iter_mut().zip(latest) {
                row.show(latest);
            }
        } else {
            self.replace(latest)?;
        }
        let tooltip = tray_model::tooltip(entries);
        if tooltip != self.tooltip {
            tray.set_tooltip(Some(&tooltip))
                .map_err(|error| error.to_string())?;
            self.tooltip = tooltip;
        }
        Ok(())
    }

    fn replace(&mut self, latest: Vec<TrayController>) -> Result<(), String> {
        if !self.rows.is_empty() {
            for row in self.rows.drain(..) {
                self.menu
                    .remove(&row.menu)
                    .map_err(|error| error.to_string())?;
            }
            self.menu
                .remove(&self.separator)
                .map_err(|error| error.to_string())?;
        }
        for (index, shown) in latest.into_iter().enumerate() {
            let row = Row::new(shown)?;
            self.menu
                .insert(&row.menu, CONTROLLERS_AT + index)
                .map_err(|error| error.to_string())?;
            self.rows.push(row);
        }
        if !self.rows.is_empty() {
            self.menu
                .insert(&self.separator, CONTROLLERS_AT + self.rows.len())
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn command(&self, chosen: &MenuId) -> Option<Command> {
        self.rows
            .iter()
            .find(|row| row.toggle.id() == chosen)
            .map(|row| row.shown.command.clone())
    }
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
    let controllers = Controllers {
        menu: menu.clone(),
        separator: PredefinedMenuItem::separator(),
        rows: Vec::new(),
        tooltip: text::APP_NAME.to_owned(),
    };
    let tray = TrayIconBuilder::new()
        .with_icon(icon()?)
        .with_tooltip(text::APP_NAME)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| error.to_string())?;

    cx.spawn(async move |cx: &mut AsyncApp| {
        run(cx, tray, &items, controllers, &backend, &requests, &show).await;
        cx.update(|cx| cx.quit());
    })
    .detach();
    Ok(())
}

async fn run(
    cx: &mut AsyncApp,
    tray: TrayIcon,
    items: &Items,
    mut controllers: Controllers,
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
            } else if let Some(command) = controllers.command(&event.id) {
                backend.command(command);
            }
        }
        let enabled = backend.starts_with_windows();
        if items.start_with_windows.is_checked() != enabled {
            items.start_with_windows.set_checked(enabled);
        }
        if let Err(error) = controllers.show(&tray, &backend.controllers(Instant::now())) {
            log::line(&text::log::cannot_update_tray(&error));
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

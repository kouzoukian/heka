use std::fs;
use std::path::PathBuf;

use heka_app::ui::assets::APP_ICON_PNG;
use heka_app::{log, text};
use heka_core::alerts::Notification;
use heka_windows::notification::{self, Delivery};

const APP_ID: &str = "Kouzoukian.Heka";
const ICON_FILE: &str = "notification.png";

#[derive(Debug, Default)]
pub struct Notifier {
    registered: bool,
}

impl Notifier {
    pub fn tell(&mut self, wanted: &Notification) {
        self.register();
        let (title, body) = (&wanted.name, text::notification_body(&wanted.alert));
        let tag = text::notification_tag(wanted.mac, &wanted.alert);
        let line = match notification::show(APP_ID, &tag, title, &body) {
            Ok(Delivery::Shown) => text::log::notified(title, &body),
            Ok(Delivery::TurnedOffInWindows) => text::log::notifications_off(title, &body),
            Err(error) => text::log::cannot_notify(title, &body, &error),
        };
        log::line(&line);
    }

    fn register(&mut self) {
        if self.registered {
            return;
        }
        let icon = write_icon();
        match notification::register(APP_ID, text::APP_NAME, icon.as_deref()) {
            Ok(()) => self.registered = true,
            Err(error) => {
                log::line(&text::log::cannot_register_notifications(
                    &error.to_string(),
                ));
            }
        }
    }
}

fn write_icon() -> Option<PathBuf> {
    let folder = log::folder()?;
    let path = folder.join(ICON_FILE);
    let written = fs::create_dir_all(&folder).and_then(|()| fs::write(&path, APP_ICON_PNG));
    match written {
        Ok(()) => Some(path),
        Err(error) => {
            log::line(&text::log::cannot_register_notifications(
                &error.to_string(),
            ));
            None
        }
    }
}

use std::io;
use std::path::Path;

use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{
    NotificationSetting, ToastNotification, ToastNotificationManager,
};
use windows::core::HSTRING;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyW, RegSetKeyValueW,
};

const APP_IDS: &str = r"Software\Classes\AppUserModelId";
const TEMPLATE: &str =
    r#"<toast><visual><binding template="ToastGeneric"><text/><text/></binding></visual></toast>"#;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn check(status: u32) -> io::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

fn key_of(app_id: &str) -> Vec<u16> {
    wide(&format!(r"{APP_IDS}\{app_id}"))
}

fn set(key: &[u16], name: &str, text: &str) -> io::Result<()> {
    let (name, data) = (wide(name), wide(text));
    check(unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        )
    })
}

pub fn register(app_id: &str, name: &str, icon: Option<&Path>) -> io::Result<()> {
    let key = key_of(app_id);
    set(&key, "DisplayName", name)?;
    match icon {
        Some(icon) => set(&key, "IconUri", &icon.to_string_lossy()),
        None => Ok(()),
    }
}

pub fn unregister(app_id: &str) -> io::Result<()> {
    let key = key_of(app_id);
    match unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, key.as_ptr()) } {
        ERROR_FILE_NOT_FOUND => Ok(()),
        status => check(status),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Shown,
    TurnedOffInWindows,
}

pub fn show(app_id: &str, tag: &str, title: &str, body: &str) -> Result<Delivery, String> {
    deliver(app_id, tag, title, body).map_err(|error| error.to_string())
}

fn deliver(app_id: &str, tag: &str, title: &str, body: &str) -> windows::core::Result<Delivery> {
    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(TEMPLATE))?;
    let lines = document.GetElementsByTagName(&HSTRING::from("text"))?;
    for (index, line) in [(0, title), (1, body)] {
        lines.Item(index)?.SetInnerText(&HSTRING::from(line))?;
    }
    let toast = ToastNotification::CreateToastNotification(&document)?;
    toast.SetTag(&HSTRING::from(tag))?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))?;
    notifier.Show(&toast)?;
    let off = notifier
        .Setting()
        .is_ok_and(|setting| setting != NotificationSetting::Enabled);
    Ok(if off {
        Delivery::TurnedOffInWindows
    } else {
        Delivery::Shown
    })
}

pub fn in_notification_centre(app_id: &str) -> Result<u32, String> {
    let count = || {
        ToastNotificationManager::History()?
            .GetHistoryWithId(&HSTRING::from(app_id))?
            .Size()
    };
    count().map_err(|error: windows::core::Error| error.to_string())
}

pub fn clear(app_id: &str) -> Result<(), String> {
    ToastNotificationManager::History()
        .and_then(|history| history.ClearWithId(&HSTRING::from(app_id)))
        .map_err(|error| error.to_string())
}

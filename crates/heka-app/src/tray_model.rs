use heka_core::mac::MacAddress;

use crate::text;
use crate::ui::model::{Command, ControllerEntry, EntryState};

pub const TOOLTIP_LIMIT: usize = 127;

const CUT: char = '…';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayController {
    pub mac: MacAddress,
    pub label: String,
    pub action: &'static str,
    pub command: Command,
}

pub fn controllers(entries: &[ControllerEntry]) -> Vec<TrayController> {
    entries
        .iter()
        .map(|entry| {
            let battery = entry
                .battery
                .filter(|battery| battery.level().is_some())
                .map(|battery| text::battery(Some(battery)));
            let label = text::tray_controller(&title(entry), state(entry), battery.as_deref());
            TrayController {
                mac: entry.mac,
                label: label.replace('&', "&&"),
                action: match entry.state {
                    EntryState::Stopped => text::START,
                    _ => text::STOP,
                },
                command: entry.toggle(),
            }
        })
        .collect()
}

pub fn tooltip(entries: &[ControllerEntry]) -> String {
    let lines: Vec<(String, String)> = entries
        .iter()
        .map(|entry| {
            let level = entry
                .battery
                .and_then(|battery| battery.level())
                .map(text::percent);
            let status = text::tray_status(state(entry), level.as_deref());
            (title(entry), status)
        })
        .collect();
    let written = |room: usize| {
        lines
            .iter()
            .fold(text::APP_NAME.to_owned(), |all, (title, status)| {
                let line = text::tray_line(&cut(title, room), status);
                format!("{all}\n{line}")
            })
    };
    let longest = lines.iter().map(|(title, _)| units(title)).max();
    let full = longest.unwrap_or_default();
    let fitting = (1..=full)
        .rev()
        .map(written)
        .find(|tooltip| units(tooltip) <= TOOLTIP_LIMIT);
    cut(&fitting.unwrap_or_else(|| written(1)), TOOLTIP_LIMIT)
}

fn title(entry: &ControllerEntry) -> String {
    entry
        .nickname
        .clone()
        .unwrap_or_else(|| text::with_short_mac(entry.name, &entry.short_mac()))
}

fn state(entry: &ControllerEntry) -> &'static str {
    match entry.state {
        EntryState::Bridged => text::BRIDGED,
        EntryState::Held { .. } => text::HELD,
        EntryState::Stopped => text::STOPPED,
        EntryState::NotBridged => text::NOT_BRIDGED,
    }
}

fn units(words: &str) -> usize {
    words.encode_utf16().count()
}

fn cut(words: &str, room: usize) -> String {
    if units(words) <= room {
        return words.to_owned();
    }
    let mut kept = String::new();
    for character in words.chars() {
        if units(&kept) + character.len_utf16() + CUT.len_utf16() > room {
            break;
        }
        kept.push(character);
    }
    kept.push(CUT);
    kept
}

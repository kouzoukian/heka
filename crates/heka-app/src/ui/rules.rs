use gpui::{Div, ParentElement, Stateful, Styled};
use gpui_component::Theme;
use gpui_kit_assets::IconName;

use crate::text;
use crate::ui::banners::{clamped, strip};
use crate::ui::metrics::icon;
use crate::ui::model::{ControllerEntry, Screen, profile_name};

pub fn rule_words(screen: &Screen, entry: &ControllerEntry) -> Option<String> {
    let program = entry.rule_program.as_deref()?;
    let name_of = |id: Option<&str>| match id {
        Some(id) => profile_name(&screen.profiles, id)
            .unwrap_or(text::DEFAULT_PROFILE)
            .to_owned(),
        None => text::DEFAULT_PROFILE.to_owned(),
    };
    let applied = name_of(entry.applied_profile.as_deref());
    let own = name_of(entry.profile.as_deref());
    Some(text::rule_applies(program, &applied, &own))
}

pub fn rule_strip(
    screen: &Screen,
    entry: &ControllerEntry,
    theme: &Theme,
) -> Option<Stateful<Div>> {
    let words = rule_words(screen, entry)?;
    Some(
        strip("rule-applies", 2, theme)
            .bg(theme.info.opacity(0.08))
            .child(icon(IconName::AppWindow).text_color(theme.info))
            .child(clamped(words.into())),
    )
}

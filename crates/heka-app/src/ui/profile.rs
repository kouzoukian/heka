use std::rc::Rc;

use gpui::{
    AnyElement, App, Div, Entity, InteractiveElement, IntoElement, ParentElement, SharedString,
    Stateful, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input as TextInput, InputState};
use gpui_component::{ActiveTheme, Selectable, Sizable, Theme};
use gpui_kit_assets::IconName;
use heka_core::profile::{Binding, Input, Output};
use heka_core::profiles::suggest_name;
use heka_core::tuning::{StickSide, TriggerSide};
use heka_core::virtual_controller::OutputType;

use crate::text;
use crate::ui::banners::{clamped, strip, text_button};
use crate::ui::detail::row;
use crate::ui::metrics::{GAP, INSET, SMALL_TEXT, icon};
use crate::ui::model::{Command, ControllerEntry, ProfileEntry, Screen, SendCommand};
use crate::ui::panel::faint;
use crate::ui::segmented::{Pick, Segment, picked, segmented};
use crate::ui::settings::{explained, group};
use crate::ui::sticks::sticks_section;
use crate::ui::triggers::triggers_section;

pub type ToggleBinding = Rc<dyn Fn(Input, &mut Window, &mut App)>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Manage {
    #[default]
    Idle,
    Renaming(String),
    ConfirmingDelete(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Profiles,
    Buttons,
    Sticks,
    Triggers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileAction {
    Show(Section),
    Side(StickSide),
    TriggerSide(TriggerSide),
    Duplicate,
    StartRename,
    CommitRename,
    AskDelete,
    ConfirmDelete,
    Cancel,
    Import,
    Export,
}

pub type ProfileAct = Rc<dyn Fn(ProfileAction, &mut Window, &mut App)>;

pub struct ProfileView<'a> {
    pub editing: Option<Input>,
    pub section: Section,
    pub side: StickSide,
    pub trigger_side: TriggerSide,
    pub manage: Manage,
    pub rename: &'a Entity<InputState>,
    pub toggle: ToggleBinding,
    pub act: ProfileAct,
}

pub fn profile_page(
    screen: &Screen,
    entry: &ControllerEntry,
    view: &ProfileView<'_>,
    send: &SendCommand,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let chosen = entry
        .profile
        .as_ref()
        .and_then(|id| screen.profiles.iter().find(|profile| &profile.id == id));
    let body = match view.section {
        Section::Profiles => scrolled(profiles_section(screen, entry, chosen, view, send, theme)),
        Section::Buttons => scrolled(buttons_section(chosen, view, send, theme)),
        Section::Sticks => match chosen {
            Some(profile) => {
                sticks_section(profile, entry, view.side, &view.act, send, theme).into_any_element()
            }
            None => scrolled(default_note(text::STICKS_DEFAULT, theme)),
        },
        Section::Triggers => match chosen {
            Some(profile) => {
                triggers_section(profile, entry, view.trigger_side, &view.act, send, theme)
                    .into_any_element()
            }
            None => scrolled(default_note(text::TRIGGERS_DEFAULT, theme)),
        },
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(section_switch(view, theme))
        .child(body)
}

fn scrolled(content: Div) -> AnyElement {
    div()
        .id("profile-body")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .child(content)
        .into_any_element()
}

fn default_note(words: &'static str, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(strip("profile-fixed", 2, theme).child(clamped(words.into())))
}

fn section_switch(view: &ProfileView<'_>, theme: &Theme) -> Div {
    let items = [
        (Section::Profiles, text::SECTION_PROFILES),
        (Section::Buttons, text::SECTION_BUTTONS),
        (Section::Sticks, text::SECTION_STICKS),
        (Section::Triggers, text::SECTION_TRIGGERS),
    ]
    .map(|(section, words)| {
        let act = Rc::clone(&view.act);
        let pick: Pick = Rc::new(move |window, cx| act(ProfileAction::Show(section), window, cx));
        (
            SharedString::from(format!("profile-section-{section:?}")),
            words,
            view.section == section,
            pick,
        )
    });
    row(theme).justify_start().child(picked(items, theme))
}

fn profiles_section(
    screen: &Screen,
    entry: &ControllerEntry,
    chosen: Option<&ProfileEntry>,
    view: &ProfileView<'_>,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let mut sorted: Vec<&ProfileEntry> = screen.profiles.iter().collect();
    sorted.sort_by_key(|profile| profile.profile.name.to_lowercase());
    let page = div()
        .flex()
        .flex_col()
        .flex_none()
        .child(group(text::GROUP_PROFILES, theme))
        .child(profile_row(
            entry,
            None,
            text::DEFAULT_PROFILE.into(),
            chosen.is_none(),
            send,
            theme,
        ))
        .children(sorted.into_iter().map(|profile| {
            profile_row(
                entry,
                Some(&profile.id),
                profile.profile.name.clone().into(),
                chosen.is_some_and(|chosen| chosen.id == profile.id),
                send,
                theme,
            )
        }))
        .child(new_profile_row(screen, send, theme))
        .child(manage_row(chosen, view, theme));
    match chosen {
        None => page.child(
            strip("profile-fixed", 2, theme).child(clamped(text::DEFAULT_PROFILE_FIXED.into())),
        ),
        Some(profile) => page.child(output_row(profile, send, theme)),
    }
}

fn buttons_section(
    chosen: Option<&ProfileEntry>,
    view: &ProfileView<'_>,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    match chosen {
        None => default_note(text::DEFAULT_PROFILE_FIXED, theme),
        Some(profile) => div()
            .flex()
            .flex_col()
            .flex_none()
            .child(group(text::GROUP_BINDINGS, theme))
            .children(
                Input::ALL
                    .into_iter()
                    .map(|input| binding_block(profile, input, view, send, theme)),
            ),
    }
}

fn profile_row(
    entry: &ControllerEntry,
    id: Option<&str>,
    name: SharedString,
    selected: bool,
    send: &SendCommand,
    theme: &Theme,
) -> Stateful<Div> {
    let command = Command::ChooseProfile(entry.mac, id.map(str::to_owned));
    let send = Rc::clone(send);
    let element: SharedString = format!("profile-{}-{}", entry.mac, id.unwrap_or("default")).into();
    let hover = theme.accent;
    row(theme)
        .id(element)
        .cursor_pointer()
        .hover(move |this| this.bg(hover))
        .text_color(if selected {
            theme.foreground
        } else {
            theme.muted_foreground
        })
        .on_click(move |_, _, _| send(command.clone()))
        .child(name)
        .when(selected, |this| this.child(icon(IconName::Check)))
}

fn new_profile_row(screen: &Screen, send: &SendCommand, theme: &Theme) -> Div {
    let name = suggest_name(
        screen
            .profiles
            .iter()
            .map(|profile| profile.profile.name.as_str()),
    );
    let send = Rc::clone(send);
    row(theme).justify_start().child(
        text_button("new-profile", IconName::Plus, text::NEW_PROFILE)
            .on_click(move |_, _, _| send(Command::CreateProfile(name.clone()))),
    )
}

fn manage_row(chosen: Option<&ProfileEntry>, view: &ProfileView<'_>, theme: &Theme) -> Div {
    let button = |id: &'static str, name: IconName, words: &'static str, action: ProfileAction| {
        let act = Rc::clone(&view.act);
        text_button(id, name, words).on_click(move |_, window, cx| act(action, window, cx))
    };
    let bar = || {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(GAP)
            .px(INSET)
            .py(GAP)
            .border_b_1()
            .border_color(theme.border)
    };
    match (chosen, &view.manage) {
        (Some(profile), Manage::Renaming(id)) if *id == profile.id => bar()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(theme.foreground)
                    .child(TextInput::new(view.rename).small()),
            )
            .child(button(
                "rename-save",
                IconName::Check,
                text::RENAME,
                ProfileAction::CommitRename,
            ))
            .child(button(
                "rename-cancel",
                IconName::X,
                text::CANCEL,
                ProfileAction::Cancel,
            )),
        (Some(profile), Manage::ConfirmingDelete(id)) if *id == profile.id => bar()
            .text_size(SMALL_TEXT)
            .child(clamped(
                text::delete_profile_question(&profile.profile.name, profile.users).into(),
            ))
            .child(button(
                "delete-cancel",
                IconName::X,
                text::CANCEL,
                ProfileAction::Cancel,
            ))
            .child(button(
                "delete-confirm",
                IconName::Trash,
                text::DELETE,
                ProfileAction::ConfirmDelete,
            )),
        (Some(_), _) => bar()
            .child(button(
                "profile-duplicate",
                IconName::Copy,
                text::DUPLICATE,
                ProfileAction::Duplicate,
            ))
            .child(button(
                "profile-rename",
                IconName::Pencil,
                text::RENAME,
                ProfileAction::StartRename,
            ))
            .child(button(
                "profile-export",
                IconName::ArrowUp,
                text::EXPORT,
                ProfileAction::Export,
            ))
            .child(button(
                "profile-import",
                IconName::ArrowDown,
                text::IMPORT,
                ProfileAction::Import,
            ))
            .child(button(
                "profile-delete",
                IconName::Trash,
                text::DELETE,
                ProfileAction::AskDelete,
            )),
        (None, _) => bar()
            .child(button(
                "profile-duplicate",
                IconName::Copy,
                text::DUPLICATE,
                ProfileAction::Duplicate,
            ))
            .child(button(
                "profile-import",
                IconName::ArrowDown,
                text::IMPORT,
                ProfileAction::Import,
            )),
    }
}

fn output_row(profile: &ProfileEntry, send: &SendCommand, theme: &Theme) -> Div {
    let choices = [
        (text::OUTPUT_OWN, None),
        (text::XBOX, Some(OutputType::Xbox)),
        (text::PS4, Some(OutputType::Ps4)),
    ];
    let segments = choices.map(|(words, output_type)| {
        Segment::words(
            format!("profile-output-{}-{words}", profile.id),
            words,
            profile.profile.output_type == output_type,
            Some(Command::SetProfileOutputType(
                profile.id.clone(),
                output_type,
            )),
        )
    });
    row(theme)
        .child(explained(
            text::ROW_PROFILE_OUTPUT,
            text::PROFILE_OUTPUT_TIP,
            theme,
        ))
        .child(segmented(segments, send, theme))
}

fn binding_block(
    profile: &ProfileEntry,
    input: Input,
    view: &ProfileView<'_>,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let binding = profile.profile.binding(input);
    let open = view.editing == Some(input);
    let toggle = Rc::clone(&view.toggle);
    let changed = binding != Binding::Default;
    let current = Button::new(SharedString::from(format!(
        "binding-{}-{input:?}",
        profile.id
    )))
    .ghost()
    .selected(open)
    .text_color(if changed {
        theme.foreground
    } else {
        faint(theme)
    })
    .accessibility_label(text::binding_words(binding))
    .tooltip(text::BINDING_TIP)
    .child(
        div()
            .text_size(SMALL_TEXT)
            .child(text::binding_words(binding)),
    )
    .on_click(move |_, window, cx| toggle(input, window, cx));
    let block = div()
        .flex()
        .flex_col()
        .flex_none()
        .child(row(theme).child(text::input_name(input)).child(current));
    if open {
        block.child(options(profile, input, binding, view, send, theme))
    } else {
        block
    }
}

fn options(
    profile: &ProfileEntry,
    input: Input,
    binding: Binding,
    view: &ProfileView<'_>,
    send: &SendCommand,
    theme: &Theme,
) -> Div {
    let choices = [Binding::Default, Binding::Nothing]
        .into_iter()
        .chain(Output::ALL.into_iter().map(Binding::To));
    let buttons = choices.map(|choice| {
        let command = Command::BindInput(profile.id.clone(), input, choice);
        let send = Rc::clone(send);
        let toggle = Rc::clone(&view.toggle);
        Button::new(SharedString::from(format!(
            "bind-{}-{input:?}-{}",
            profile.id,
            text::binding_words(choice)
        )))
        .outline()
        .selected(binding == choice)
        .child(
            div()
                .text_size(SMALL_TEXT)
                .child(text::binding_words(choice)),
        )
        .on_click(move |_, window, cx| {
            send(command.clone());
            toggle(input, window, cx);
        })
    });
    div()
        .flex()
        .flex_wrap()
        .gap(GAP)
        .px(INSET)
        .py(GAP)
        .border_b_1()
        .border_color(theme.border)
        .children(buttons)
}

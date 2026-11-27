use std::rc::Rc;

use gpui::{
    App, Div, InteractiveElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{ActiveTheme, Selectable, Theme};
use gpui_kit_assets::IconName;
use heka_core::profile::{Binding, Input, Output};
use heka_core::profiles::suggest_name;
use heka_core::virtual_controller::OutputType;

use crate::text;
use crate::ui::banners::{clamped, strip, text_button};
use crate::ui::detail::row;
use crate::ui::metrics::{GAP, INSET, SMALL_TEXT, icon};
use crate::ui::model::{Command, ControllerEntry, ProfileEntry, Screen, SendCommand};
use crate::ui::panel::faint;
use crate::ui::segmented::{Segment, segmented};
use crate::ui::settings::{explained, group};

pub type ToggleBinding = Rc<dyn Fn(Input, &mut Window, &mut App)>;

pub struct ProfileView {
    pub editing: Option<Input>,
    pub toggle: ToggleBinding,
}

pub fn profile_page(
    screen: &Screen,
    entry: &ControllerEntry,
    view: &ProfileView,
    send: &SendCommand,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let chosen = entry
        .profile
        .as_ref()
        .and_then(|id| screen.profiles.iter().find(|profile| &profile.id == id));
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
        .child(new_profile_row(screen, send, theme));
    match chosen {
        None => page.child(
            strip("profile-fixed", 2, theme).child(clamped(text::DEFAULT_PROFILE_FIXED.into())),
        ),
        Some(profile) => page
            .child(output_row(profile, send, theme))
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
    view: &ProfileView,
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
    view: &ProfileView,
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

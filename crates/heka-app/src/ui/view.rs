use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyWindowHandle, App, AppContext, Context, Div, Entity, Hsla, IntoElement, KeyBinding,
    ParentElement, PathPromptOptions, Render, Result, Styled, TitlebarOptions, Window,
    WindowHandle, WindowOptions, actions, div, prelude::FluentBuilder as _, px, size,
};
use gpui_component::button::Button;
use gpui_component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::status_bar::StatusBar;
use gpui_component::{ActiveTheme, Root, TitleBar};
use gpui_kit_assets::IconName;
use heka_core::feedback::Lightbar;
use heka_core::mac::MacAddress;
use heka_core::preferences::{LightbarChoice, PreferenceChange};
use heka_core::profile::{Input, MAX_NAME_CHARS};
use heka_core::profiles::Profiles;
use heka_core::settings::MAX_NICKNAME_CHARS;
use heka_core::tuning::{StickSide, TriggerSide};

use crate::ui::detail::{Showing, ToggleSettings, Toggles, selected_button, tool_button};
use crate::ui::metrics::{GAP, INSET, ROW, SMALL_TEXT, TEXT, icon};
use crate::ui::model::{Backend, Command, Screen, Selection, SendCommand};
use crate::ui::profile::{Manage, ProfileAct, ProfileAction, ProfileView, Section, ToggleBinding};
use crate::ui::settings::Fields;
use crate::ui::{detail, panel, picker, placement};
use crate::{text, theme};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(15);

actions!(heka, [OpenAppSettings]);

pub fn init(cx: &mut App) {
    gpui_component::init(cx);
    theme::apply(cx).expect("the built-in font and theme load");
    cx.bind_keys([KeyBinding::new("ctrl-,", OpenAppSettings, None)]);
    cx.on_action(|_: &OpenAppSettings, cx| {
        cx.defer(|cx| {
            if let Some(&window) = cx.windows().first() {
                show_page(window, Page::AppSettings, cx);
            }
        });
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Controller,
    ControllerSettings,
    Profile,
    AppSettings,
    Log,
    About,
}

impl Page {
    pub fn toggled(self, page: Self) -> Self {
        if self == page { Self::Controller } else { page }
    }

    pub fn of_a_controller(self) -> Self {
        match self {
            Self::Controller | Self::ControllerSettings | Self::Profile => self,
            Self::AppSettings | Self::Log | Self::About => Self::Controller,
        }
    }
}

pub fn show_page(window: AnyWindowHandle, page: Page, cx: &mut App) {
    let Some(window) = window.downcast::<Root>() else {
        return;
    };
    let _ = window.update(cx, |root, _, cx| {
        if let Ok(view) = root.view().clone().downcast::<MainView>() {
            view.update(cx, |view, cx| view.show(page, cx));
        }
    });
}

pub fn open_window(
    cx: &mut App,
    backend: Rc<dyn Backend>,
    page: Page,
) -> Result<WindowHandle<Root>> {
    let (window_bounds, display_id) = placement::opening(backend.window_placement().as_ref(), cx);
    let min_size = size(px(placement::MIN_SIZE.0), px(placement::MIN_SIZE.1));
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(text::APP_NAME.into()),
            ..TitleBar::title_bar_options()
        }),
        window_bounds: Some(window_bounds),
        display_id,
        window_min_size: Some(min_size),
        ..TitleBar::window_options()
    };
    cx.open_window(options, move |window, cx| {
        placement::keep_size(window, window_bounds);
        let view = cx.new(|cx| MainView::new(backend, page, window, cx));
        cx.new(|cx| Root::new(view, window, cx))
    })
}

struct MainView {
    backend: Rc<dyn Backend>,
    screen: Screen,
    selection: Selection,
    page: Page,
    name: Entity<InputState>,
    colour: Entity<ColorPickerState>,
    held_colour: Lightbar,
    filled: Option<MacAddress>,
    editing: Option<Input>,
    rename: Entity<InputState>,
    manage: Manage,
    section: Section,
    side: StickSide,
    trigger_side: TriggerSide,
}

impl MainView {
    fn new(
        backend: Rc<dyn Backend>,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(|text, _| text.chars().count() <= MAX_NICKNAME_CHARS)
        });
        cx.subscribe(&name, |this: &mut Self, name, event: &InputEvent, cx| {
            if let (InputEvent::Change, Some(mac)) = (event, this.filled) {
                let typed = name.read(cx).value().to_string();
                this.backend.command(Command::SetNickname(mac, typed));
            }
        })
        .detach();
        let rename = cx.new(|cx| {
            InputState::new(window, cx).validate(|text, _| text.chars().count() <= MAX_NAME_CHARS)
        });
        cx.subscribe(&rename, |this: &mut Self, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.commit_rename(cx);
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SAMPLE_INTERVAL).await;
                if this.update(cx, Self::sample).is_err() {
                    break;
                }
            }
        })
        .detach();
        let colour = cx.new(|cx| ColorPickerState::new(window, cx));
        cx.observe(&colour, |_, _, cx| cx.notify()).detach();
        let hex = colour.read(cx).hex_input().clone();
        cx.subscribe(&hex, |this: &mut Self, hex, event: &InputEvent, cx| {
            if let InputEvent::Change = event
                && let Some(chosen) = picker::typed(&hex.read(cx).value())
            {
                this.choose_colour(chosen);
            }
        })
        .detach();
        cx.subscribe(
            &colour,
            |this: &mut Self, _, event: &ColorPickerEvent, _| {
                let ColorPickerEvent::Change(chosen) = event;
                if let Some(chosen) = chosen {
                    this.choose_colour(picker::picked(*chosen));
                }
            },
        )
        .detach();
        cx.on_release(|this: &mut Self, _| this.leave_settings())
            .detach();
        cx.observe_window_bounds(window, |this: &mut Self, window, cx| {
            if let Some(placement) = placement::current(window, cx) {
                this.backend.command(Command::RememberWindow(placement));
            }
        })
        .detach();
        let screen = backend.screen(Instant::now());
        let mut selection = Selection::default();
        selection.follow(&screen.controllers);
        Self {
            backend,
            screen,
            selection,
            page,
            name,
            colour,
            held_colour: Lightbar::DEFAULT,
            filled: None,
            editing: None,
            rename,
            manage: Manage::Idle,
            section: Section::default(),
            side: StickSide::Left,
            trigger_side: TriggerSide::Left,
        }
    }

    fn sample(&mut self, cx: &mut Context<Self>) {
        let screen = self.backend.screen(Instant::now());
        if screen != self.screen {
            self.screen = screen;
            self.selection.follow(&self.screen.controllers);
            cx.notify();
        }
    }

    fn choose(&mut self, mac: MacAddress, cx: &mut Context<Self>) {
        self.selection.choose(mac, &self.screen.controllers);
        self.page = self.page.of_a_controller();
        self.editing = None;
        self.manage = Manage::Idle;
        cx.notify();
    }

    fn toggle_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.show(self.page.toggled(page), cx);
    }

    fn toggle_binding(&mut self, input: Input, cx: &mut Context<Self>) {
        self.editing = if self.editing == Some(input) {
            None
        } else {
            Some(input)
        };
        cx.notify();
    }

    fn chosen_profile(&self) -> Option<String> {
        self.selection
            .shown(&self.screen.controllers)
            .and_then(|entry| entry.profile.clone())
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        if let Manage::Renaming(id) = std::mem::take(&mut self.manage) {
            let name = self.rename.read(cx).value().to_string();
            self.backend.command(Command::RenameProfile(id, name));
        }
        cx.notify();
    }

    fn profile_action(
        &mut self,
        action: ProfileAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chosen = self.chosen_profile();
        match action {
            ProfileAction::Show(section) => {
                self.section = section;
                self.editing = None;
            }
            ProfileAction::Side(side) => self.side = side,
            ProfileAction::TriggerSide(side) => self.trigger_side = side,
            ProfileAction::Duplicate => {
                self.manage = Manage::Idle;
                self.backend.command(Command::DuplicateProfile(chosen));
            }
            ProfileAction::StartRename => {
                if let Some(id) = chosen {
                    let name = self
                        .screen
                        .profiles
                        .iter()
                        .find(|profile| profile.id == id)
                        .map(|profile| profile.profile.name.clone())
                        .unwrap_or_default();
                    self.rename
                        .update(cx, |input, cx| input.set_value(name, window, cx));
                    self.manage = Manage::Renaming(id);
                }
            }
            ProfileAction::CommitRename => self.commit_rename(cx),
            ProfileAction::AskDelete => {
                if let Some(id) = chosen {
                    self.manage = Manage::ConfirmingDelete(id);
                }
            }
            ProfileAction::ConfirmDelete => {
                if let Manage::ConfirmingDelete(id) = std::mem::take(&mut self.manage) {
                    self.backend.command(Command::DeleteProfile(id));
                }
            }
            ProfileAction::Cancel => self.manage = Manage::Idle,
            ProfileAction::Import => self.import_profile(cx),
            ProfileAction::Export => {
                if let Some(id) = chosen {
                    self.export_profile(id, cx);
                }
            }
        }
        cx.notify();
    }

    fn import_profile(&self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(text::IMPORT_PROMPT.into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = chosen.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, _| {
                    this.backend.command(Command::ImportProfile(path));
                });
            }
        })
        .detach();
    }

    fn export_profile(&self, id: String, cx: &mut Context<Self>) {
        let directory = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_default();
        let name = Profiles::file_name(&id);
        let chosen = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = chosen.await {
                let _ = this.update(cx, |this, _| {
                    this.backend.command(Command::ExportProfile(id, path));
                });
            }
        })
        .detach();
    }

    fn show(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.editing = None;
        self.manage = Manage::Idle;
        self.leave_settings();
        cx.notify();
    }

    fn leave_settings(&mut self) {
        if let Some(mac) = self.filled.take() {
            self.backend.command(Command::EndRumbleTest(mac));
        }
    }

    fn choose_colour(&mut self, colour: Lightbar) {
        self.held_colour = colour;
        if let Some(mac) = self.filled {
            let change = PreferenceChange::Lightbar(Some(LightbarChoice::Colour(colour)));
            self.backend.command(Command::ChangePreference(mac, change));
        }
    }

    fn fill_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page != Page::ControllerSettings {
            return;
        }
        let mac = self
            .selection
            .shown(&self.screen.controllers)
            .map(|entry| entry.mac);
        let another = mac != self.filled;
        if another {
            self.leave_settings();
        }
        self.filled = mac;
        let Some(entry) = self.selection.shown(&self.screen.controllers) else {
            return;
        };
        let chosen = match entry.preferences.lightbar {
            Some(LightbarChoice::Colour(colour)) => colour,
            _ => Lightbar::DEFAULT,
        };
        if another {
            let nickname = entry.nickname.clone().unwrap_or_default();
            let model = entry.name;
            self.name.update(cx, |name, cx| {
                name.set_placeholder(model, window, cx);
                name.set_value(nickname, window, cx);
            });
        }
        let state = self.colour.read(cx);
        let open = state.is_open();
        let stale =
            chosen != self.held_colour || state.preview().map(picker::picked) != Some(chosen);
        if another || (!open && stale) {
            self.held_colour = chosen;
            self.colour.update(cx, |colour, cx| {
                colour.set_open(false, cx);
                colour.set_value(picker::drawn(chosen), window, cx);
            });
        }
    }

    fn body(&self, cx: &mut Context<Self>) -> Div {
        let view = cx.weak_entity();
        let choose: panel::Choose = Rc::new(move |mac, cx: &mut App| {
            let _ = view.update(cx, |view, cx| view.choose(mac, cx));
        });
        let backend = Rc::clone(&self.backend);
        let send: SendCommand = Rc::new(move |command: Command| backend.command(command));
        let view = cx.weak_entity();
        let toggle_settings: ToggleSettings = Rc::new(move |_, cx: &mut App| {
            let _ = view.update(cx, |view, cx| {
                view.toggle_page(Page::ControllerSettings, cx)
            });
        });
        let view = cx.weak_entity();
        let toggle_profile: ToggleSettings = Rc::new(move |_, cx: &mut App| {
            let _ = view.update(cx, |view, cx| view.toggle_page(Page::Profile, cx));
        });
        let toggles = Toggles {
            settings: toggle_settings,
            profile: toggle_profile,
        };
        let view = cx.weak_entity();
        let toggle_binding: ToggleBinding = Rc::new(move |input, _, cx: &mut App| {
            let _ = view.update(cx, |view, cx| view.toggle_binding(input, cx));
        });
        let view = cx.weak_entity();
        let act: ProfileAct = Rc::new(move |action, window: &mut Window, cx: &mut App| {
            let _ = view.update(cx, |view, cx| view.profile_action(action, window, cx));
        });
        let showing = match self.page {
            Page::Controller => Showing::Live,
            Page::ControllerSettings => Showing::ControllerSettings(Fields {
                name: &self.name,
                colour: &self.colour,
            }),
            Page::Profile => Showing::Profile(ProfileView {
                editing: self.editing,
                section: self.section,
                side: self.side,
                trigger_side: self.trigger_side,
                manage: self.manage.clone(),
                rename: &self.rename,
                toggle: toggle_binding,
                act,
            }),
            Page::AppSettings => Showing::AppSettings(self.screen.app),
            Page::Log => Showing::Log(&self.screen.log),
            Page::About => Showing::About,
        };
        let entry = self.selection.shown(&self.screen.controllers);
        let selected = entry
            .filter(|_| self.page.of_a_controller() == self.page)
            .map(|entry| entry.mac);
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(panel::panel(&self.screen, selected, &choose, cx))
            .child(detail::pane(
                &self.screen,
                entry,
                showing,
                &toggles,
                &send,
                cx,
            ))
    }

    fn page_button(
        &self,
        page: Page,
        name: IconName,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> Button {
        let button = tool_button(label, name, label);
        selected_button(button, self.page == page, cx.theme())
            .on_click(cx.listener(move |view, _, _, cx| view.toggle_page(page, cx)))
    }

    fn status_bar(&self, cx: &mut Context<Self>) -> StatusBar {
        let settings = self.page_button(
            Page::AppSettings,
            IconName::Settings,
            text::APP_SETTINGS_TIP,
            cx,
        );
        let log = self.page_button(Page::Log, IconName::ScrollText, text::LOG, cx);
        let about = self.page_button(Page::About, IconName::Info, text::ABOUT, cx);
        let screen = &self.screen;
        let theme = cx.theme();
        let count = |name: IconName, count: usize, color: Hsla| {
            div()
                .flex()
                .items_center()
                .gap(GAP)
                .when(count > 0, |this| this.text_color(color))
                .child(icon(name))
                .child(count.to_string())
        };
        let counts = div()
            .flex()
            .gap(INSET)
            .child(count(IconName::CircleX, screen.errors(), theme.danger))
            .child(count(
                IconName::TriangleAlert,
                screen.warnings(),
                theme.warning,
            ));
        StatusBar::new()
            .h(ROW)
            .px(INSET)
            .text_size(SMALL_TEXT)
            .left(counts)
            .right(screen.summary())
            .right(
                div()
                    .flex()
                    .gap(GAP)
                    .child(settings)
                    .child(log)
                    .child(about),
            )
    }
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.fill_fields(window, cx);
        let body = self.body(cx);
        let status_bar = self.status_bar(cx);
        let theme = cx.theme();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_size(TEXT)
            .text_color(theme.muted_foreground)
            .child(
                TitleBar::new()
                    .bg(theme.title_bar)
                    .child(div().text_size(TEXT).child(text::APP_NAME)),
            )
            .child(body)
            .child(status_bar)
    }
}

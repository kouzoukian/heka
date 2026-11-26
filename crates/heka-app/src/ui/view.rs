use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Bounds, Context, Div, Entity, Hsla, IntoElement, ParentElement, Render,
    Result, Styled, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div,
    prelude::FluentBuilder as _, px, size,
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
use heka_core::settings::MAX_NICKNAME_CHARS;

use crate::ui::detail::{Showing, ToggleSettings, selected_button, tool_button};
use crate::ui::metrics::{GAP, INSET, ROW, SMALL_TEXT, TEXT, icon};
use crate::ui::model::{Backend, Command, Screen, Selection, SendCommand};
use crate::ui::settings::Fields;
use crate::ui::{detail, panel, picker};
use crate::{text, theme};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(15);

const SIZE: (f32, f32) = (840., 680.);
const MIN_SIZE: (f32, f32) = (640., 560.);

pub fn init(cx: &mut App) {
    gpui_component::init(cx);
    theme::apply(cx).expect("the built-in font and theme load");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Controller,
    Settings,
    Log,
}

impl Page {
    pub fn toggled(self, page: Self) -> Self {
        if self == page { Self::Controller } else { page }
    }

    pub fn of_a_controller(self) -> Self {
        match self {
            Self::Controller | Self::Settings => self,
            Self::Log => Self::Controller,
        }
    }
}

pub fn open_window(cx: &mut App, backend: Rc<dyn Backend>) -> Result<WindowHandle<Root>> {
    open_window_on(cx, backend, Page::default())
}

pub fn open_window_on(
    cx: &mut App,
    backend: Rc<dyn Backend>,
    page: Page,
) -> Result<WindowHandle<Root>> {
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(text::APP_NAME.into()),
            ..TitleBar::title_bar_options()
        }),
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(SIZE.0), px(SIZE.1)),
            cx,
        ))),
        window_min_size: Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))),
        ..TitleBar::window_options()
    };
    cx.open_window(options, |window, cx| {
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
        cx.observe(&colour, |this: &mut Self, colour, cx| {
            let state = colour.read(cx);
            let whole = state.hex_input().read(cx).value().len() >= "#RRGGBB".len();
            if let Some(shown) = state.preview().map(picker::picked)
                && whole
                && shown != this.held_colour
            {
                this.choose_colour(shown);
            }
            cx.notify();
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
        cx.notify();
    }

    fn toggle_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = self.page.toggled(page);
        self.filled = None;
        cx.notify();
    }

    fn choose_colour(&mut self, colour: Lightbar) {
        self.held_colour = colour;
        if let Some(mac) = self.filled {
            let change = PreferenceChange::Lightbar(Some(LightbarChoice::Colour(colour)));
            self.backend.command(Command::ChangePreference(mac, change));
        }
    }

    fn fill_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page != Page::Settings {
            return;
        }
        let entry = self.selection.shown(&self.screen.controllers);
        let another = entry.map(|entry| entry.mac) != self.filled;
        self.filled = entry.map(|entry| entry.mac);
        let Some(entry) = entry else {
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
            let _ = view.update(cx, |view, cx| view.toggle_page(Page::Settings, cx));
        });
        let showing = match self.page {
            Page::Controller => Showing::Live,
            Page::Settings => Showing::Settings(Fields {
                name: &self.name,
                colour: &self.colour,
            }),
            Page::Log => Showing::Log(&self.screen.log),
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
                &toggle_settings,
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
        let log = self.page_button(Page::Log, IconName::ScrollText, text::LOG, cx);
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
            .right(log)
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

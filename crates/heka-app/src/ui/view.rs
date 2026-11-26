use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Bounds, Context, Div, Entity, Hsla, IntoElement, ParentElement, Render,
    Result, Styled, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div,
    prelude::FluentBuilder as _, px, size,
};
use gpui_component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::status_bar::StatusBar;
use gpui_component::{ActiveTheme, Root, TitleBar};
use gpui_kit_assets::IconName;
use heka_core::feedback::Lightbar;
use heka_core::mac::MacAddress;
use heka_core::preferences::LightbarChoice;
use heka_core::settings::MAX_NICKNAME_CHARS;

use crate::ui::detail::{Showing, ToggleSettings};
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

pub fn open_window(cx: &mut App, backend: Rc<dyn Backend>) -> Result<WindowHandle<Root>> {
    open(cx, backend, false)
}

pub fn open_window_in_settings(
    cx: &mut App,
    backend: Rc<dyn Backend>,
) -> Result<WindowHandle<Root>> {
    open(cx, backend, true)
}

fn open(cx: &mut App, backend: Rc<dyn Backend>, in_settings: bool) -> Result<WindowHandle<Root>> {
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
        let view = cx.new(|cx| MainView::new(backend, in_settings, window, cx));
        cx.new(|cx| Root::new(view, window, cx))
    })
}

struct MainView {
    backend: Rc<dyn Backend>,
    screen: Screen,
    selection: Selection,
    in_settings: bool,
    name: Entity<InputState>,
    colour: Entity<ColorPickerState>,
    held_colour: Lightbar,
    filled: Option<MacAddress>,
}

impl MainView {
    fn new(
        backend: Rc<dyn Backend>,
        in_settings: bool,
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
            in_settings,
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
        cx.notify();
    }

    fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.in_settings = !self.in_settings;
        self.filled = None;
        cx.notify();
    }

    fn choose_colour(&mut self, colour: Lightbar) {
        self.held_colour = colour;
        let entry = self
            .screen
            .controllers
            .iter()
            .find(|entry| Some(entry.mac) == self.filled);
        if let Some(entry) = entry {
            let mut preferences = entry.preferences;
            preferences.lightbar = Some(LightbarChoice::Colour(colour));
            self.backend
                .command(Command::SetPreferences(entry.mac, preferences));
        }
    }

    fn fill_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.in_settings {
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
            let _ = view.update(cx, |view, cx| view.toggle_settings(cx));
        });
        let showing = if self.in_settings {
            Showing::Settings(Fields {
                name: &self.name,
                colour: &self.colour,
            })
        } else {
            Showing::Live
        };
        let entry = self.selection.shown(&self.screen.controllers);
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(panel::panel(
                &self.screen,
                entry.map(|entry| entry.mac),
                &choose,
                cx,
            ))
            .child(detail::pane(
                &self.screen,
                entry,
                showing,
                &toggle_settings,
                &send,
                cx,
            ))
    }
}

fn status_bar(screen: &Screen, cx: &App) -> StatusBar {
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
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.fill_fields(window, cx);
        let body = self.body(cx);
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
            .child(status_bar(&self.screen, cx))
    }
}

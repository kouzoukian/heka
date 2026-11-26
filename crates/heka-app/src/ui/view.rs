//! The window's root view, in Zed's anatomy: title bar, the Controller panel, the
//! selected Controller's pane and the status bar.
//!
//! The view keeps what only the window knows: which Controller is selected,
//! whether its settings show, and what their fields hold.

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
use heka_core::settings::MAX_NICKNAME_CHARS;

use crate::ui::detail::{Showing, ToggleSettings};
use crate::ui::metrics::{GAP, INSET, ROW, SMALL_TEXT, TEXT, icon};
use crate::ui::model::{Backend, Command, Screen, Selection, SendCommand};
use crate::ui::settings::Fields;
use crate::ui::{detail, panel, picker};
use crate::{text, theme};

/// How often the window samples the screen, about 60 times a second.
///
/// A little under a 60 Hz frame, to leave room for the timer waking late. The
/// `ui_preview` example prints the rate: 54 to 63 a second in a release build.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(15);

const SIZE: (f32, f32) = (840., 680.);
/// Room for the panel beside the drawing, and for the drawing below the
/// selected Controller's properties.
const MIN_SIZE: (f32, f32) = (640., 560.);

/// Sets up gpui-component, the font and the theme. Call once, before opening the
/// window, in an application given [`crate::ui::assets::Assets`].
pub fn init(cx: &mut App) {
    // Must run before any gpui-component element is created.
    gpui_component::init(cx);
    theme::apply(cx).expect("the built-in font and theme load");
}

/// Opens heka's window onto `backend`.
pub fn open_window(cx: &mut App, backend: Rc<dyn Backend>) -> Result<WindowHandle<Root>> {
    open(cx, backend, false)
}

/// Opens heka's window on the selected Controller's settings.
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
        // gpui-component expects a `Root` as the window's first view.
        cx.new(|cx| Root::new(view, window, cx))
    })
}

struct MainView {
    backend: Rc<dyn Backend>,
    /// The last sample, which is what is drawn.
    screen: Screen,
    selection: Selection,
    /// The pane shows the selected Controller's settings.
    in_settings: bool,
    /// The field for the selected Controller's name.
    name: Entity<InputState>,
    /// The picker for the selected Controller's own lightbar colour.
    colour: Entity<ColorPickerState>,
    /// The colour the picker last held, so that only the player's changes to it
    /// are sent.
    held_colour: Lightbar,
    /// Whose settings the fields hold, `None` until the settings show.
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
        // The name is kept as it is typed, so that closing the window loses none.
        cx.subscribe(&name, |this: &mut Self, name, event: &InputEvent, cx| {
            if let (InputEvent::Change, Some(mac)) = (event, this.filled) {
                let typed = name.read(cx).value().to_string();
                this.backend.command(Command::SetNickname(mac, typed));
            }
        })
        .detach();
        // The window only samples. Input is carried by each Bridge's own thread.
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
        // The Controller follows the colour as it is dragged, and as soon as a
        // whole hex value is typed: shorter ones pass for other colours on the way.
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
        // Confirming the colour the picker started with chooses it too.
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

    /// Redraws only if something shown has changed, so an idle window costs nothing.
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
        // Filled again when the settings next show.
        self.filled = None;
        cx.notify();
    }

    /// The player picked this lightbar colour for the Controller whose settings
    /// show.
    fn choose_colour(&mut self, colour: Lightbar) {
        self.held_colour = colour;
        let entry = self
            .screen
            .controllers
            .iter()
            .find(|entry| Some(entry.mac) == self.filled);
        if let Some(entry) = entry {
            // Sent even if the last sample already shows it: that may be older than
            // the colour sent before this one.
            let mut preferences = entry.preferences;
            preferences.lightbar = Some(colour);
            self.backend
                .command(Command::SetPreferences(entry.mac, preferences));
        }
    }

    /// Puts the selected Controller's name and colour in the fields. The name is
    /// filled only when the settings show another Controller, leaving the player's
    /// typing alone; the colour also follows the switch while the picker is closed.
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
        let chosen = entry.preferences.lightbar.unwrap_or(Lightbar::DEFAULT);
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
        // Another choice on the switch, or a value left half typed.
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

    /// The panel beside the selected Controller's pane.
    fn body(&self, cx: &mut Context<Self>) -> Div {
        let view = cx.weak_entity();
        let choose: panel::Choose = Rc::new(move |mac, cx: &mut App| {
            // The view is gone only while its window closes.
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

/// How many errors and warnings there are, as Zed counts its diagnostics, and
/// the summary of Controllers.
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
    // One item, so that the counts are spaced as the rest of the window is.
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
                    // Flat, as in Zed, instead of gpui-component's gradient.
                    .bg(theme.title_bar)
                    .child(div().text_size(TEXT).child(text::APP_NAME)),
            )
            .child(body)
            .child(status_bar(&self.screen, cx))
    }
}

//! The window's root view, in Zed's anatomy: title bar, the Controller panel, the
//! selected Controller's pane and the status bar.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Bounds, Context, Div, Hsla, IntoElement, ParentElement, Render, Result,
    Styled, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div,
    prelude::FluentBuilder as _, px, size,
};
use gpui_component::status_bar::StatusBar;
use gpui_component::{ActiveTheme, Root, TitleBar};
use gpui_kit_assets::IconName;
use heka_core::mac::MacAddress;

use crate::ui::metrics::{GAP, INSET, ROW, SMALL_TEXT, TEXT, icon};
use crate::ui::model::{Backend, Command, Screen, Selection, SendCommand};
use crate::ui::{detail, panel};
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
        let view = cx.new(|cx| MainView::new(backend, cx));
        // gpui-component expects a `Root` as the window's first view.
        cx.new(|cx| Root::new(view, window, cx))
    })
}

struct MainView {
    backend: Rc<dyn Backend>,
    /// The last sample, which is what is drawn.
    screen: Screen,
    selection: Selection,
}

impl MainView {
    fn new(backend: Rc<dyn Backend>, cx: &mut Context<Self>) -> Self {
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
        let screen = backend.screen(Instant::now());
        let mut selection = Selection::default();
        selection.follow(&screen.controllers);
        Self {
            backend,
            screen,
            selection,
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

    /// The panel beside the selected Controller's pane.
    fn body(&self, cx: &mut Context<Self>) -> Div {
        let view = cx.weak_entity();
        let choose: panel::Choose = Rc::new(move |mac, cx: &mut App| {
            // The view is gone only while its window closes.
            let _ = view.update(cx, |view, cx| view.choose(mac, cx));
        });
        let backend = Rc::clone(&self.backend);
        let send: SendCommand = Rc::new(move |command: Command| backend.command(command));
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
            .child(detail::pane(&self.screen, entry, &send, cx))
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

//! The window's root view: title bar, notices, the cards or the empty state, and
//! the status bar.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Bounds, Context, Div, InteractiveElement, IntoElement, ParentElement, Render,
    Result, StatefulInteractiveElement, Styled, TitlebarOptions, Window, WindowBounds,
    WindowHandle, WindowOptions, div, px, size,
};
use gpui_component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_component::status_bar::StatusBar;
use gpui_component::{ActiveTheme, Icon, Root, Sizable, TitleBar};
use gpui_kit_assets::IconName;

use crate::ui::card;
use crate::ui::model::{Backend, Command, Level, Notice, Screen};
use crate::{text, theme};

/// How often the window samples the screen, about 60 times a second.
///
/// A little under a 60 Hz frame on purpose: Windows timers run on a tick of up to
/// 15.6 ms, so asking for 16 ms can wake on the second tick, at half the rate.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(15);

/// Sets up gpui-component and the theme. Call once, before opening the window, in
/// an application given [`crate::ui::assets::Assets`].
pub fn init(cx: &mut App) {
    // Must run before any gpui-component element is created.
    gpui_component::init(cx);
    theme::apply(cx).expect("the built-in theme loads");
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
            size(px(840.), px(680.)),
            cx,
        ))),
        // Room for one card across.
        window_min_size: Some(size(px(card::WIDTH + 34.), px(360.))),
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
        Self { backend, screen }
    }

    /// Redraws only if something shown has changed, so an idle window costs nothing.
    fn sample(&mut self, cx: &mut Context<Self>) {
        let screen = self.backend.screen(Instant::now());
        if screen != self.screen {
            self.screen = screen;
            cx.notify();
        }
    }

    fn cards(&self, cx: &App) -> impl IntoElement + use<> {
        let backend = Rc::clone(&self.backend);
        let send: card::Send = Rc::new(move |command: Command| backend.command(command));
        let cards: Vec<Div> = self
            .screen
            .cards
            .iter()
            .map(|model| card::card(model, &send, cx))
            .collect();
        div()
            .id("controllers")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .justify_center()
                    .gap_3()
                    .p_4()
                    .children(cards),
            )
    }
}

fn notice(notice: &Notice, cx: &App) -> Div {
    let theme = cx.theme();
    let (icon, color) = match notice.level {
        Level::Warning => (IconName::TriangleAlert, theme.warning),
        Level::Error => (IconName::CircleX, theme.danger),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_2()
        .px_4()
        .py_2()
        .bg(color.opacity(0.1))
        .border_b_1()
        .border_color(theme.border)
        .child(Icon::new(icon).small().text_color(color))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .child(notice.text.clone()),
        )
}

/// Shown while there is no card to show.
fn empty_state(starting: bool, cx: &App) -> Empty {
    let (title, hint) = if starting {
        (text::STARTING, None)
    } else {
        (text::NO_CONTROLLER, Some(text::CONNECT_HINT))
    };
    let mut header = EmptyHeader::new()
        .media(
            EmptyMedia::new()
                .with_variant(EmptyMediaVariant::Icon)
                .child(Icon::new(IconName::Gamepad2).text_color(cx.theme().muted_foreground)),
        )
        .title(EmptyTitle::new().child(title));
    if let Some(hint) = hint {
        header = header.description(EmptyDescription::new().child(hint));
    }
    Empty::new().header(header)
}

impl Render for MainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.screen.cards.is_empty() {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(empty_state(self.screen.starting, cx))
                .into_any_element()
        } else {
            self.cards(cx).into_any_element()
        };
        let theme = cx.theme();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(TitleBar::new().child(div().text_sm().child(text::APP_NAME)))
            .children(self.screen.notices.iter().map(|n| notice(n, cx)))
            .child(div().flex_1().min_h_0().child(body))
            .child(StatusBar::new().left(div().text_xs().child(self.screen.summary())))
    }
}

use gpui::{
    App, AppContext, Context, IntoElement, Render, TitlebarOptions, Window, WindowOptions, div,
};
use gpui_component::Root;

struct MainView;

impl Render for MainView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        // Must run before any gpui-component element is created.
        gpui_component::init(cx);

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("heka".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            let view = cx.new(|_| MainView);
            // gpui-component expects a `Root` as the window's first view.
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("failed to open the main window");
    });
}

mod bridging;
mod settings_file;
mod window_backend;

use std::rc::Rc;

use gpui::App;
use heka_app::ui::assets::Assets;
use heka_app::ui::view;

use bridging::Bridging;

fn main() {
    gpui_platform::application()
        .with_assets(Assets)
        .run(|cx: &mut App| {
            view::init(cx);

            let bridging = Rc::new(Bridging::start());
            // Quitting removes every Virtual Controller before the process ends.
            cx.on_app_quit({
                let bridging = Rc::clone(&bridging);
                move |_| {
                    bridging.quit();
                    async {}
                }
            })
            .detach();

            view::open_window(cx, bridging).expect("failed to open the main window");
        });
}

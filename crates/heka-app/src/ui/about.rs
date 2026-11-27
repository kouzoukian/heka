use gpui::{App, Div, ParentElement, SharedString, Styled, div, img, px};
use gpui_component::link::Link;
use gpui_component::{ActiveTheme, Theme};

use crate::text;
use crate::ui::assets::APP_ICON;
use crate::ui::banners::{clamped, strip};
use crate::ui::detail::row;
use crate::ui::metrics::{GAP, TEXT};
use crate::ui::panel::faint;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

const ICON_SIZE: f32 = 36.;

pub fn about(cx: &App) -> Div {
    let theme = cx.theme();
    let license = |file: &str| format!("{REPOSITORY}/blob/main/{file}");
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(heading(theme))
        .child(fact(text::ROW_VERSION, VERSION, theme))
        .child(
            row(theme).child(text::ROW_LICENSE).child(
                div()
                    .flex()
                    .gap(GAP)
                    .child(
                        Link::new("license-mit")
                            .href(license("LICENSE-MIT"))
                            .child(text::LICENSE_MIT),
                    )
                    .child(text::LICENSE_OR)
                    .child(
                        Link::new("license-apache")
                            .href(license("LICENSE-APACHE"))
                            .child(text::LICENSE_APACHE),
                    ),
            ),
        )
        .child(fact(text::ROW_THEME, text::THEME_CREDIT, theme))
        .child(fact(text::ROW_FONT, text::FONT_CREDIT, theme))
        .child(fact(text::ROW_NOTICES, text::NOTICES_FILE, theme))
        .child(strip("notices-tip", 1, theme).child(clamped(text::NOTICES_TIP.into())))
        .child(
            row(theme).child(text::ROW_SOURCE).child(
                Link::new("repository")
                    .href(REPOSITORY)
                    .child(SharedString::from(text::setup::short_url(REPOSITORY))),
            ),
        )
}

fn heading(theme: &Theme) -> impl gpui::IntoElement {
    strip("about-heading", 2, theme)
        .gap(px(12.))
        .child(img(APP_ICON).flex_none().size(px(ICON_SIZE)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(TEXT)
                        .text_color(theme.foreground)
                        .child(text::APP_NAME),
                )
                .child(clamped(text::TAGLINE.into()).text_color(faint(theme))),
        )
}

fn fact(label: &'static str, value: &'static str, theme: &Theme) -> Div {
    row(theme)
        .child(label)
        .child(div().text_color(theme.foreground).child(value))
}

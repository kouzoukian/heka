//! The setup screen: which drivers are missing, what that costs and where to get
//! them. It only links to the official installers: heka never downloads or runs one.
//!
//! It shows above the Controller list, which stays in view.

use gpui::{App, IntoElement, ParentElement, SharedString, Styled, div, px};
use gpui_component::ActiveTheme;
use gpui_component::button::Button;
use gpui_component::link::Link;
use heka_core::setup::{Missing, Setup};

/// Every string of the setup screen.
pub mod text {
    use heka_core::setup::{Driver, Missing};

    pub const CHECK_AGAIN: &str = "Check again";
    pub const AFTER_INSTALLING: &str =
        "Run the installer yourself, restart Windows if it asks, then check again.";
    /// A Controller's state while ViGEmBus is missing.
    pub const NOT_BRIDGED: &str = "Not Bridged: ViGEmBus is missing";

    pub fn heading(missing: &Missing) -> String {
        let name = name(missing.driver);
        match &missing.error {
            None => format!("{name} is not installed or not running"),
            Some(error) => format!("{name} cannot be used: {error}"),
        }
    }

    pub fn consequence(driver: Driver) -> &'static str {
        match driver {
            Driver::ViGEmBus => {
                "Controllers are listed and show live input, but none is Bridged until it is installed."
            }
            Driver::HidHide => {
                "Bridges work, but games may see each Controller twice (double input): the Controller and its Virtual Controller."
            }
        }
    }

    pub fn name(driver: Driver) -> &'static str {
        match driver {
            Driver::ViGEmBus => "ViGEmBus",
            Driver::HidHide => "HidHide",
        }
    }
}

/// The setup screen, or nothing when no driver is missing.
pub fn setup_screen(
    setup: &Setup,
    check_again: impl Fn() + 'static,
    cx: &App,
) -> Option<impl IntoElement> {
    let missing = setup.missing();
    if missing.is_empty() {
        return None;
    }
    Some(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(missing.iter().map(|missing| driver_card(missing, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(text::AFTER_INSTALLING),
                    )
                    .child(
                        Button::new("check-drivers")
                            .label(text::CHECK_AGAIN)
                            .on_click(move |_, _, _| check_again()),
                    ),
            ),
    )
}

fn driver_card(missing: &Missing, cx: &App) -> impl IntoElement {
    let url = missing.driver.installer_url();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .p_3()
        .rounded(px(6.))
        .border_1()
        .border_color(cx.theme().warning)
        .child(div().child(SharedString::from(text::heading(missing))))
        .child(
            div()
                .text_color(cx.theme().warning)
                .child(text::consequence(missing.driver)),
        )
        .child(Link::new(text::name(missing.driver)).href(url).child(url))
}

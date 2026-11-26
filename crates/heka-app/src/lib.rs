//! heka's interface, in Zed's layout and the One Dark theme (ADR 0002).
//!
//! Nothing here touches HID, ViGEmBus or HidHide: the window draws a
//! [`ui::model::Screen`] that a [`ui::model::Backend`] gives it. The `heka`
//! binary is the real backend; the `ui_preview` example is one made of fake data.

pub mod text;
pub mod ui;

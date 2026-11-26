//! The sizes the window is laid out with, in whole pixels: edges stay sharp, and
//! rows beside each other line up. gpui's rem sizes would not be whole at the
//! font's 15 px base.

use gpui::{Pixels, px};
use gpui_component::{Icon, Sizable};
use gpui_kit_assets::IconName;

/// The height of every row: headers, tabs, list entries, properties, one-line
/// banners and the status bar.
pub const ROW: Pixels = px(32.);
/// From a pane's edge to what it holds, as the title bar insets its title.
pub const INSET: Pixels = px(12.);
/// Between an icon and its words, and between neighbours in a row.
pub const GAP: Pixels = px(8.);
/// A button, or any control, inside a row.
pub const CONTROL: Pixels = px(24.);
pub const TEXT: Pixels = px(13.);
pub const SMALL_TEXT: Pixels = px(11.);
/// A line of text that may wrap. With 6 above, 6 below and the border, one line
/// fills a row.
pub const LINE: Pixels = px(19.);

/// An icon of the one size the window uses.
pub fn icon(name: IconName) -> Icon {
    Icon::new(name).with_size(px(14.))
}

use gpui::{Pixels, px};
use gpui_component::{Icon, Sizable};
use gpui_kit_assets::IconName;

pub const ROW: Pixels = px(32.);
pub const INSET: Pixels = px(12.);
pub const GAP: Pixels = px(8.);
pub const CONTROL: Pixels = px(24.);
pub const TEXT: Pixels = px(13.);
pub const SMALL_TEXT: Pixels = px(11.);
pub const LINE: Pixels = px(14.);

pub fn icon(name: IconName) -> Icon {
    Icon::new(name).with_size(px(14.))
}

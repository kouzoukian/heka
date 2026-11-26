# Window layout and the live drawing

Builds on the look chosen in [ADR 0002](0002-zed-look-through-gpui-component.md). Sizes are in `ui/metrics.rs`.

## Rows

- Rows (headers, tabs, list entries, properties, one-line banners, status bar) are one height. A line of text in a strip is 14 px, so two fit a row.
- Banners are a whole number of rows high whatever they say: text is cut at two lines, with a tooltip holding all of it. A driver banner is two rows: what is missing with its installer link, then what that costs. Notices stay in view while the rest of the pane scrolls.
- The tab's box and line match the panel's header, and panel rows carry the same divider as pane rows, so each line runs the width of the window. Panel rows start right under the header.
- The player's name for a Controller tells it apart, so the end of its MAC address is shown only without one.
- The Active Link, battery and Output Type share a row without labels; icons, words and tooltips name them. Setting rows explain themselves in a tooltip on the label.
- Ids are per Controller, so each keeps its own hover and tooltip.
- Buttons take icons as children, because `.icon` and `.label` would impose the button's own sizes.
- Status-bar error and warning counts are one item, so they are spaced like the rest of the window. The buttons for the log and About are at its right end, where Zed puts the buttons that open panels. The title bar is flat, as in Zed, instead of `gpui-component`'s gradient.
- A log entry is one row: its time, then its message cut at two lines, with a tooltip holding the whole line. Equal rows let the list build only those in view.
- About's heading is three rows high and the rest of it one row each.
- The window's minimum size leaves room for the panel beside the drawing and the drawing below the properties.

## Colour picker

The picker is heka's own popup around `gpui-component`'s picker state, which keeps the hex field and sliders in step, because a lightbar has no transparency and the ready-made popup offers it. Transparency is dropped. It opens leftwards, as the switch ends at the pane's right edge. Slider tracks show what their positions give (hue in flat steps; lightness from black through the colour to white), with no filled part and a thumb that stands out.

## Live drawing

- Shaped like the Output Type: an Xbox pad or a DualShock 4, both heka's own drawings. Positions are pixels from the top left of a drawing of fixed width, scaled as a whole to the room it gets, within limits and with a margin of 20 px beside and 12 px above and below; it takes the rest of its column and never less than its smallest size, and is dimmed and at rest without input.
- Shapes are painted on one canvas with lettering over them, through the `Surface` trait, so that a test builds every shape of both pads without a window. A shape or label that cannot be painted is logged once, not every frame. A shape described by its right half starts on the middle line, goes clockwise and returns to it; the left half is the right walked backwards with control points swapped.
- Each trigger's lower edge is the middle of the bumper's upper edge, 2 px above it, and each bumper's lower edge follows the body's upper one. These coordinates are kept in step by hand.
- The Xbox pad is lettered and placed as the Virtual Controller maps it. It has no touchpad or mute button, which drive nothing there, and draws a share button that a Virtual Controller does not have.
- A touch dot's travel is the pad less the dot's size, so it stays inside. Touch positions are stored in thousandths of the pad.
- Stick offsets are -1 to 1, positive right and down; trigger fill is 0 to 1.

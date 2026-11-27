# Zed look through gpui-component

The interface follows Zed's anatomy, font and density with the Min Dark (Solid) or Min Light (Solid) theme (MIT, from Miguel Solorio's Min Theme and its Zed port), built with `gpui-component`. The player chooses in the app settings; dark is the default, and heka does not follow the Windows colour mode. It first used Zed's One Dark; the maintainer chose Min Dark instead. Zed's own `ui` and `theme` crates are GPL-3.0-or-later and unpublished, so using them would force heka off its MIT OR Apache-2.0 license and pull in much of Zed.

A panel on the left lists the Controllers, and the pane beside it shows the selected one, between the title bar and the status bar. Surfaces are flat, divided by 1 px lines. It first showed a grid of cards, which looked nothing like Zed. `gpui-component` supplies the title bar, status bar, buttons, tooltips, links and icons; the rows, the tab and the segmented switch are plain elements, to get Zed's density.

Google Sans Code (SIL OFL 1.1) is built into the binary, at the 15 px base size the maintainer uses in Zed. Spacing, icons and the window's own text sizes are whole pixels (`ui/metrics.rs`) rather than rems, which are not whole at 15 px, so that edges stay sharp. Rows are 32 px, and everything in the pane above the drawing is a whole number of rows high at any width, so it stays level with the panel's rows; text that does not fit its rows is cut short, with a tooltip holding all of it.

`min-dark.json` and `min-light.json` in `crates/heka-app/themes/` are converted from the Zed port of Min Theme; the theme and the port are MIT licensed, with notices in `themes/LICENSE-MIN-THEME`. The font's license is `fonts/LICENSE-GOOGLE-SANS-CODE`. One variable font file holds every weight.

`gpui-component` ignores a theme key it does not know, so a typo would silently leave a default colour; a test checks every key the theme sets.

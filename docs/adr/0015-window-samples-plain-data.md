# The window samples plain data through a backend

`heka-app`'s library touches no HID, ViGEmBus or HidHide. The window draws a `Screen` that a `Backend` gives it and sends `Command`s back; the `heka` binary is the real backend and the `ui_preview` example a fake one.

## Sampling

- `screen` is called from the window's thread about 60 times a second, so no backend call may wait on a Bridge, a driver or the supervisor thread's work.
- The window samples every 15 ms, a little under a 60 Hz frame to leave room for the timer waking late (`ui_preview` measured 54 to 63 samples a second in a release build). It redraws only when two samples differ, so that nothing is repainted while idle. Motion and battery are left out of the live drawing's data, so motion alone never redraws.
- `ControllerEntry` and `PadModel` take the core's structs apart field by field, so a new field cannot go unnoticed.
- English only; all text is in `text.rs`. `heka-core` returns typed errors for it to word, and only the `{error}` parts that come from Windows, `hidapi` and the drivers are shown as they arrive. Hold countdowns round up, so 0 shows only once the time is over.

## View state

The view keeps what only the window knows: the selected Controller, whether its settings show, and what their fields hold.

- Selection starts on the first Controller and follows it wherever it moves; when it leaves, the one taking its place is shown.
- The name field is filled only when the settings show another Controller, leaving typing alone. The name is sent as it is typed, so closing the window loses none.
- The colour picker also follows the lightbar switch while closed. Only the player's changes are sent. The Controller follows a dragged colour, and a typed one once the whole hex value is there (#50). Confirming the colour the picker opened with chooses it too. Every choice is sent, even one the last sample already shows, since that sample may be older than the previous command.
- Ignoring a failed update of the weak view handle is correct: the view is gone only while its window closes.

## Start-up order

`gpui_component::init` runs before any `gpui-component` element is created, and the window's first view is a `Root`, which `gpui-component` expects. The built-in font is added before the theme names it, so it is never looked up while missing. The application needs `ui::assets::Assets`, because without an asset source `gpui-component`'s icons render blank.

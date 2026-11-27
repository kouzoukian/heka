# Settings never fail to load, and Preferences only shape Feedback

## Settings

Per Controller, keyed by MAC address, heka remembers Stopped, the Output Type, the player's name for it, its idle time ([ADR 0019](0019-turning-off-a-bluetooth-controller.md)) and its Preferences, as JSON at `%APPDATA%\heka\settings.json`. The Preferences are written beside the other fields.

- Reading never fails: a missing, unreadable or corrupt file gives the defaults, a broken entry gives that Controller the defaults, and unknown fields are ignored, so a later version may add fields.
- A file that is not a JSON object is copied to `settings.json.unreadable` when read, because remembering the window writes over it without the player changing anything.
- The file is written through a temporary file, so a crash never leaves half of it.
- A name is trimmed and at most 24 characters; a blank one removes it.
- A rumble strength above 100% loads as 100%.

## App settings

What concerns heka as a whole is under `app` in the same file: whether it starts in the tray, what closing the window does, and where the window was. A file without it, as v1 wrote, gives the defaults; a broken field gives that field alone its default. Start with Windows is not in the file ([ADR 0013](0013-windows-integration.md)).

- The window's thread owns the app settings, so the tray, a closing window and a second launch read them without waiting. It hands every change to the supervisor thread, which alone writes the file.
- The window is remembered whenever it moves, resizes or is maximized, so nothing has to be caught at quit. A minimized window is not.
- It reopens where it was if its centre and the middle of its title bar (16 px below its top) are on a monitor, preferring the monitor it was on, because monitors with different scaling can overlap in logical pixels. Otherwise it opens centred on the primary monitor, at its size cut to that monitor's and then raised to the minimum size.
- The monitor has to be named when the window opens: GPUI moves a window whose centre is off the named monitor, the primary one if none is named, to that monitor's default place.
- Windows is asked which monitor the window is on: GPUI changes a window's display only once its centre leaves the old display's logical bounds, which between monitors of different scaling can be never.
- A window that opens at another size than asked is resized, since crossing to a monitor of another scaling while opening can rescale it.
- What is remembered is the window's normal place, so a snapped window reopens at its place from before the snap.

## Preferences

The defaults keep full rumble, leave the motors as games address them, leave the lights to the Controller and Hide it.

- The window sends only the preference the player changed, and the Supervisor applies it to what it remembers, so a change made from an older sample never undoes another.
- Rumble strength is a percentage, rounded per motor. A running Bridge follows changes through shared `LivePreferences`.
- Swapping the motors exchanges the large and small motor of the rumble a game asks for. A rumble test names a motor of the Controller, so it is not swapped.
- The player's lightbar choice is a colour or an effect, saved as a colour object or a name (`"rainbow"`). An effect moves in 50 ms steps, so it adds at most 20 output reports a second, and the Feedback thread draws it from the time since its Bridge started.
- A colour a game set wins over the player's. A dark lightbar counts as no colour, because a PS4 Virtual Controller reports one with every rumble, also for games that never set a colour.
- `Feedback` with no lightbar leaves the Controller's as it is (an Xbox Virtual Controller has none). Player lights are only ever chosen by the player, never by a game; a DualShock 4 has none.
- Rumble in `Feedback` is in Xbox terms: 0 to 255, large low-frequency (left) and small high-frequency (right) motors.

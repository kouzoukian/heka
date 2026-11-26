# Settings never fail to load, and Preferences only shape Feedback

## Settings

Per Controller, keyed by MAC address, heka remembers Stopped, the Output Type, the player's name for it and its Preferences, as JSON at `%APPDATA%\heka\settings.json`. The Preferences are written beside the other fields.

- Reading never fails: a missing, unreadable or corrupt file gives the defaults, a broken entry gives that Controller the defaults, and unknown fields are ignored, so a later version may add fields.
- The file is written through a temporary file, so a crash never leaves half of it.
- A name is trimmed and at most 24 characters; a blank one removes it.

## Preferences

The defaults keep full rumble, leave the lights to the Controller and Hide it.

- Rumble strength is a percentage, rounded per motor. A running Bridge follows changes through shared `LivePreferences`.
- A colour a game set wins over the player's. A dark lightbar counts as no colour, because a PS4 Virtual Controller reports one with every rumble, also for games that never set a colour.
- `Feedback` with no lightbar leaves the Controller's as it is (an Xbox Virtual Controller has none). Player lights are only ever chosen by the player, never by a game; a DualShock 4 has none.
- Rumble in `Feedback` is in Xbox terms: 0 to 255, large low-frequency (left) and small high-frequency (right) motors.

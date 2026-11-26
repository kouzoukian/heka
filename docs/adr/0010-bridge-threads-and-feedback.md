# A Bridge owns its threads, and Feedback never delays input

## Input thread

Each Bridge reads its Link on its own thread, parses each report, updates the Virtual Controller and publishes the state to `LiveInput` for the window to sample. The window is never in the input path. Reads go into a 128-byte buffer, larger than any report heka reads (full Bluetooth reports are 78 bytes), and a longer read, such as one Windows pads, is cut off. A report that does not parse (another report ID, a short read, a bad CRC) carries no input and is skipped.

## Endings

An ended Bridge hands back what is still usable, so that the caller decides what happens next:

- **Stopped**: both ends are live and the Virtual Controller shows the last state sent.
- **Link lost**: the Virtual Controller is sent everything released and sticks centred, and stays plugged in for a Hold.
- **Virtual Controller failed**: it refused an update and is removed; the Link is handed back.

Dropping a Bridge instead of calling `stop` removes its Virtual Controller. When the input thread ends on its own it also stops the Feedback thread. Failures to release or unplug are currently ignored, which is owed work: #46.

## Feedback thread

When both ends carry Feedback, a second thread passes it from the Virtual Controller to the Link, so that a slow write, or one that is stuck, never holds up input. A write may block for up to a second in `hidapi`.

- `OpenLink` keeps the Link's Feedback writer and `Plugged` the Virtual Controller's Feedback source and the Feedback games last sent, so a later Bridge carries rumble too. Halves without a counterpart are kept for whatever follows.
- A new Bridge first shows what games last asked for, so a Controller that moves to another Link keeps its rumble and lightbar.
- Feedback is shaped by the player's Preferences on the way ([ADR 0014](0014-settings-and-preferences.md)). The thread waits at most 50 ms at a time, so it notices a stop or changed Preferences. A light the player no longer chooses goes back to how it started: the default colour, player lights off.
- A Controller is assumed at rest until heka first sends Feedback, so a Bridge that asks for nothing never sends an output report.
- When the Bridge ends the motors are stopped, since nothing else will. On a lost Link that write fails, which is harmless. Retrying failed writes is owed: #45.

## Watch

Without ViGEmBus, input is still shown: a Watch runs a Bridge's input thread with a stand-in Virtual Controller that shows its state to nobody. Nothing is presented to games and nothing is sent to the Controller.

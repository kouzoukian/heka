# Keyboard and mouse output goes through SendInput, counted across Bridges

## Bindings

Beside what [ADR 0021](0021-profiles-and-the-mapper.md) lists, a Binding can hold a key or a key with modifiers, hold one of the five mouse buttons, or turn the wheel one step up or down. An input with such a Binding no longer drives the Virtual Controller, and a trigger gives up its travel as with any other Binding.

A key is a scan code with its extended prefix, because many games read scan codes and ignore virtual key codes. A profile file writes it as `{"keys": [29, 50]}`, the modifiers first and the key last, an extended key as its code plus 57344 (`0xE000`), and the mouse as `{"mouse": "left"}` or `{"wheel": "up"}`. Only modifiers may come before the key. A Binding that breaks these rules is dropped when read, like any other, and an earlier heka drops all three forms.

## From a Binding to an event

The Mapper stays pure: with the state the Virtual Controller shows it returns what the Controller's present state holds down on the keyboard and mouse, as fixed-size sets that cost no allocation. Each Bridge has its `Hands`, which remember what it held last and report only the difference.

All Bridges share one `Desk`, which counts how many Bindings of how many Controllers hold each key and mouse button. A key is pressed when the first takes hold and released when the last lets go, so two Bindings of one key do not cut each other short. Modifiers go down before the key and come up after it. A wheel Binding turns one step each time its input goes down and nothing when it comes up, and each input counts on its own.

`DesktopOutput` in `heka-core` is the trait that sends the events, as `VirtualBus` is for Virtual Controllers, so everything above is tested without Windows. `heka-windows` implements it with `SendInput`, by scan code. Pause alone is sent by virtual key, because its scan code on its own is Num Lock's.

## Releasing

- The input thread owns its Bridge's `Hands` and lets go before it ends, whatever ends it: Stop, a lost Link, a Held Bridge, a failed Virtual Controller, a move to another Link, quitting or a panic. `Bridge::stop` returns only after that.
- When its Mapper changes, a Bridge lets go of everything and then holds what the next report asks for under the new one. It looks after every read, reports or not. A Mapper set again without a change releases nothing. A wheel Binding does not turn for an input that was already down. Any change to a Profile counts, stick tuning too, so a key held through one is released and pressed again.
- A Bridge dropped by a panicking thread tells its input thread to end without waiting for it, and the input thread still lets go.
- A release Windows refuses, as it does while a secure desktop is shown, stays owed. The Supervisor tries again at every poll and once more when quitting. The first refusal of a run of them is logged. A refused press is not tried again, but is released later in case part of it went through.
- A heka that is ended by force releases nothing.

## The input thread

Events are sent from the input thread after the Virtual Controller has the report, only when what is held changes, under a lock the Bridges share. `SendInput` normally returns at once. It waits for other programs' low-level keyboard and mouse hooks, so a hung one could hold back the reports that follow for as long as Windows allows a hook. A thread of its own would avoid that at the price of a hand-off on every key and of releases that outlive their Bridge, which is not worth it unless it is seen to happen.

## What Windows does not allow

Windows silently ignores input sent to a window of an elevated program unless the sender is elevated too, and `SendInput` does not say so. That includes releases: a key held while such a program comes to the front can stay down, and the `Desk` cannot know, so the documentation says to press it once. Anti-cheat software may ignore or penalise sent input. heka neither elevates itself nor tries to get round either; the documentation and the Profile page say so.

## Capturing a key

The window captures a key with a `WH_KEYBOARD` hook on its own thread, installed only while it waits for one. It sees only keys sent to heka's window, never those typed elsewhere, and takes them all, so that pressing Tab or Space does not also drive the window. GPUI's key events carry neither the scan code nor the side of a modifier, which is why they are not used.

`KeyCapture` in `heka-core` turns the strokes into a choice once every key is up again, so that no stray release reaches the window: the first key that is not a modifier, with the modifiers held then, or the modifiers themselves if none was pressed. Esc alone cancels, so the Profile page offers a button that binds Esc. Losing the window's focus cancels too, which also covers the combinations Windows acts on itself, such as Alt+Tab: those can only be written into the Profile's file. A key is shown with a fixed English name, or for a character key the character the current keyboard layout gives it.

Windows adds strokes of its own, which a capture has to leave out:

- On a layout with AltGr, a left Ctrl comes just before the right Alt. `KeyCapture` drops it, and takes the layout's having AltGr from the listener.
- Around the arrows and the keys beside them, a Shift with the extended prefix, which no real key has, is released and pressed. The hook drops it.
- Around the numeric keypad with Num Lock on and Shift held, a plain Shift is released, which cannot be told from a real release, so such a combination captures as Shift. Windows does not keep Shift down for it either.

## Unsafe code

- `SendInput` is given a pointer to, and the length of, an array of `INPUT` values that lives until it returns, and the size of one. Each value's union holds the member its type names.
- `SetWindowsHookExW` installs a hook for the calling thread only, with a procedure in this executable, so no module handle is needed. `KeyHook` is not `Send`, so it is dropped, calling `UnhookWindowsHookEx` once, on the thread that installed it. The hook procedure reads only its arguments and a thread-local, which no other borrow can hold while it runs, and passes on what it does not take with `CallNextHookEx`.
- `GetKeyboardLayout`, `MapVirtualKeyExW` and `VkKeyScanExW` take values only.

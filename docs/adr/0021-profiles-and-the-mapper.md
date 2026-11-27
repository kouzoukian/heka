# A Profile is data, and the Mapper is a pure step on the input thread

## Profiles

A Profile is a name, an optional Output Type and a set of Bindings. It is JSON, one file per Profile in `%APPDATA%\heka\profiles`, the file name without `.json` being its identity. Reading never fails, as for settings ([ADR 0014](0014-settings-and-preferences.md)): a file that is not a JSON object is skipped, a Binding that names an unknown input or output is dropped and the rest load, an unknown Output Type means the Controller's own, and unknown fields are ignored. A blank or missing name falls back to the file's. A name is trimmed and at most 24 characters.

The Default Profile is not a file. It has no Bindings and no Output Type, so a Controller that uses it behaves as it did before Profiles existed, and it cannot be changed or deleted because nothing stores it. A Controller uses it until the player chooses another, and the choice is remembered with the Controller's other settings.

## Bindings

A Binding says what one input of the Controller drives on the Virtual Controller: another button, a d-pad direction, a trigger pressed all the way, or nothing. An input with no Binding drives itself. Inputs are the buttons and d-pad directions, the L2 and R2 clicks, mute, the DualSense Edge's Fn and back buttons, and the touchpad click, whole or as its left and right halves.

- Several inputs may drive the same output; it is pressed while any of them is.
- Bindings are independent: binding one input away leaves its output free for another input, which is how two buttons swap places.
- A trigger has an input (its click) and an output (full travel). A trigger input with a Binding gives up its travel as well as its click, so rebinding L2 leaves the left trigger at rest. Another input may still press it all the way.
- A click is in the left half when the first finger's x is below the middle of the touchpad, otherwise in the right. A half with a Binding wins over the whole touchpad for its clicks; a half without one, and a click with no finger down, follow the whole touchpad's Binding or, without one, the click itself.
- Output types do not matter to the Binding: the Xbox and PS4 translators drop what they cannot show ([ADR 0009](0009-translation-to-output-types.md)).

## The Mapper

`Mapper` in `heka-core` is a pure function from a Controller state to the state the Virtual Controller shows. It runs on the input thread between parsing a report and `VirtualController::update`, holds no time and no history, and looks up each Binding in a fixed array, so it adds no allocation and no lock to the input path. Sticks, motion, touch and battery pass through unchanged; buttons and the two triggers are the only things it rewrites.

Because it depends only on the Controller's present state, a change of Profile needs no release step: the next report produces the new state, and whatever the old Binding held is simply no longer produced. The Mapper replaces the Controller state and never the Virtual Controller, which is replaced only when the effective Output Type changes.

## Storage

`Profiles` in `heka-core` holds the Profiles read from the folder, by id, and touches no file: the app passes in file names and text and writes back the one that changed. A new id comes from the name: lower-case ASCII letters and digits with single hyphens, at most 40 characters, a number added when taken. `default` is never an id, so no file can pose as the Default Profile, and names Windows reserves for devices (`con`, `nul`, `com1` and the like) are prefixed.

The player's choice is the id, kept in the Controller's entry in `settings.json` beside its other fields ([ADR 0014](0014-settings-and-preferences.md)). No choice, a blank one, and an id whose file is gone all give the Default Profile, and the id is kept, so a file that comes back is used again.

## Following a Profile

A running Bridge follows its Controller's Profile the way it follows Preferences ([ADR 0014](0014-settings-and-preferences.md)): the Supervisor keeps a `LiveMapper` for each Controller and sets it whenever the choice or the Profile changes, and the input thread picks the change up with its next report. A version counter lets the thread skip the lock until something has changed, so a report that finds no change takes no lock and allocates nothing.

- The window's drawing shows what the Virtual Controller receives: `LiveInput` holds the mapped state. The idle timer ([ADR 0019](0019-turning-off-a-bluetooth-controller.md)) still watches the Controller's own state, so a button bound to nothing counts as use.
- A Profile's Output Type replaces the Controller's own while the Profile is chosen, and `Lifecycle` asks for the one that applies, so a new Bridge, a returning Held Controller and a change of Profile all agree. The Virtual Controller is replaced only when that type differs from the one it has; the player's own Output Type choice is kept untouched under the Profile's.
- Replacing the set of Profiles refreshes every Controller. A Profile that has gone gives the Default Profile until it returns.

## The Profile page

The pane has a Profile page beside Controller settings, for the selected Controller. Choosing a Profile in its list assigns it to that Controller and shows it for editing, as in the approved mockup (`docs/mockups/v2.html`). A Binding opens in place under its row, with every output as a button and Default and Nothing first; choosing one saves at once, closes the row, and reaches every Controller that uses the Profile. A row is faint while its Binding is the default and bright once changed. The Default Profile shows no Bindings and says it cannot be changed.

While the chosen Profile sets an Output Type, the Output Type switch on the Controller page asks for nothing and its tooltip says why. The Profile's own Output Type is chosen on the Profile page, among the Controller's own, Xbox and PS4.

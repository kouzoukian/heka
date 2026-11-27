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

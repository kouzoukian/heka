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

## Managing Profiles

Names are distinct, compared without regard to case, and the Default Profile's name is taken: a name already in use gets a number added (`Racing 2`), cut so that it still fits 24 characters. This holds for a new Profile, a duplicate, a rename and an import alike.

- A duplicate is named `<name> copy` and is a Profile of its own. The Default Profile can be duplicated and nothing else: it has no id, so it can be neither renamed nor deleted.
- Renaming changes the name only. The file, and so the id that Controllers remember, stays.
- Deleting a Profile clears the choice of every Controller that used it, so they use the Default Profile, and `Settings::controllers_using` tells the window how many to name beforehand. Clearing the choice, rather than leaving the id as for a missing file, keeps a later Profile that happens to get the same id from taking those Controllers over.
- Export is the Profile's JSON under its file name. Import reads a file that is a JSON object with a `name`, `output_type` or `bindings`; anything else is refused with a reason, as not JSON or not a Profile, and changes nothing. Bindings that cannot be read are dropped as when reading the folder. DS4Windows files are not read.

## Files, import and export in the app

The supervisor thread does all Profile file work, so the window never waits on the disk. Duplicating and renaming write the Profile's file; renaming rewrites the name inside it and leaves the file where it is. Deleting removes the file after the Supervisor has cleared the choices, and a file that is already gone is not an error.

Import and export take a path, which the window gets from the platform's file dialogs (`prompt_for_paths` and `prompt_for_new_path` in GPUI). A failure to read or write the file, or a file that is not a Profile, is logged and shown as a warning in the window until the next Profile action, and changes nothing. All of its text is in `text.rs`.

## Managing Profiles in the window

Under the Profile list a row of actions works on the Profile the selected Controller uses: Duplicate, Rename, Export, Import and Delete. The Default Profile offers Duplicate and Import only. Rename edits the name in place and is saved with Enter or the check; Delete asks in place, and the question says how many Controllers go back to the Default Profile. The count comes from the settings, so it includes Controllers that are not connected. What a rename or delete is about is the Profile that was chosen when it began, so choosing another Profile meanwhile cannot redirect it.

## Stick tuning

Each stick of a Profile has a dead zone, an anti-dead zone, a max zone, a sensitivity, an output curve and inversion of each axis, and the Profile can swap the sticks. All start neutral and a Profile with neutral sticks writes nothing for them, so older files load unchanged.

- The tuning is radial. The stick's travel is the distance from the centre, from 0 at rest to 1 at the edge of the gate. Inside the dead zone the stick rests at the centre; from there to the max zone the rest of the travel is stretched to full; the anti-dead zone lifts the smallest movement to its own value; the sensitivity multiplies the result, which is capped at full travel; and the curve shapes it (linear, soft, softer, sharp and smooth, all of which start at nothing and end at everything and never fall). The direction of the stick is kept, so a diagonal keeps its angle but is held to the circle. Inversion follows. A sensitivity is 50 to 200%, a dead zone and an anti-dead zone are at most 50%, and the max zone is at least 50% and always above the dead zone; settings outside are brought into range when read.
- A Profile's tuning belongs to the physical stick named. Swapping then gives the Virtual Controller's left stick what the right stick did after its own tuning.
- Sticks are 8 bits in a Controller state with the centre at 128, and the Xbox report is 16 bits. Curves would lose that precision in 8 bits, so a tuned state also carries `FineSticks`, the same four axes in 16 bits with the centre at 0 and the Controller's direction of Y. The Xbox translator uses them when there, and the PS4 translator, which has bytes too, keeps using the bytes. A stick that is not tuned gets the 16-bit value the Xbox translator has always given its byte, so the report of an untuned stick is the same whether or not the other one is tuned. A Controller state read from a report never carries them.
- With neutral sticks and no swap the Mapper passes the sticks on untouched, so the reports are identical to before. With only a swap it exchanges the bytes. The tuning is a fixed number of float operations on the input thread and takes no lock.

## The readout

`LiveInput` holds the Controller's own state beside what the Virtual Controller receives, and how many reports a second the Controller sends, all behind the one lock the input thread already takes for each report. The rate is measured by `RateMeter` over whole seconds: it needs a second of reports before it shows anything, follows a change within a second, and is forgotten when the Link ends. The window reads all three with one `sample`.

The Profile editor's readout draws the Controller's own stick position and the tuned one, from the same sample, so the two always belong together.

## Sections of the Profile page

The Profile page has three sections, chosen by a switch at its top: Profiles (the list, the actions and the Profile's Output Type), Buttons (the Bindings) and Sticks. Only the part of a section that has many rows scrolls, so the Sticks section keeps its readout in view while its rows scroll beneath it. Which section and which stick are shown belong to the window and are kept while the Controller or Profile changes.

- The Sticks section tunes the left or the right stick as chosen by a second switch. Its readout draws the Controller's own stick position and the tuned one as two dots in a circle, with rings at the dead zone and at the max zone, and says how many reports a second the Controller sends. With the sticks swapped, the tuned dot of a stick is the one the Virtual Controller shows on the other side, because that is where its tuning ends up.
- Dead zone, anti-dead zone, max zone and sensitivity are chosen from steps (0, 5, 10, 15, 20 and 30% of travel for the dead zone and so on) rather than a slider, and a value that a file holds outside the steps is offered as one more step, so that it is shown as chosen and can be left. The curve is chosen from its five shapes, and each axis can be inverted.
- The Default Profile has no tuning; its Sticks and Buttons sections say it cannot be changed.

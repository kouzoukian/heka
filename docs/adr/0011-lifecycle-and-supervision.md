# A pure lifecycle, a Supervisor and one supervisor thread

## Lifecycle

`Lifecycle` is a pure state machine: Link and timer events go in, each carrying its time, and `Change`s come out. It owns no threads, Links or Virtual Controllers. A Controller is listed once however many Links it has, in arrival order, while connected or Held.

- At most 4 Controllers are Bridged or Held at once. Waiting ones take free slots oldest first.
- A Controller that loses every Link is Held for 30 seconds. Only losing the last Link matters.
- After a failed Bridge another is tried 2 seconds later.
- Output Type, Stopped and the rest are remembered in the settings. Changing the Output Type of a Bridged Controller removes its Virtual Controller and Bridges a new one at once; of a Held one, removes it and forgets the Controller until it returns; otherwise it applies at the next Bridge.
- Until Virtual Controllers can be made, the lifecycle lists Controllers and Bridges none.

## Changes carry the side effects

Per-Controller side effects attach to the `Change`s. Hiding acts on `Bridged` and `Returned` (a returning Controller is a new device to the system), keeps hiding while Held, and restores on `Removed`, except when the removal is for a new Output Type and a `Bridged` follows at once.

## Supervisor

`Supervisor` carries the lifecycle out and owns every Link, Bridge and Virtual Controller. Its caller finds Links, passes on the player's choices and calls `poll` with the time; calls that move the lifecycle return the `Change`s carried out, in order.

- A Link to a Virtual Controller, heka's own or another ViGEmBus client's, is closed and never listed.
- Only a Controller's Active Link is read. When it changes, the Bridge moves to the new Link and keeps its Virtual Controller; when the read Link is lost the Controller moves to its next one, or is Held.
- A Controller's model is remembered while it has no Link. Preferences are shared with its Bridge, so that changes apply at once; carrying out Hiding is the caller's job.
- Without a bus, every Controller's Active Link is watched, so input and battery still show. Doing the same for unbridged Controllers when there is a bus is #19.
- A bus that is already connected is not opened again.

## The supervisor thread

In the app, one thread owns the Supervisor. It looks for drivers, finds Controllers, carries out Stop and Start, and publishes a snapshot; the window never waits for it. It wakes every 250 ms to look for Controllers, ended Bridges and timers, and to retry what is pending.

Names and preferences arrive in bursts, as the player types or drags a colour. Those are only applied: looking for Controllers and saving keep their usual pace. Quitting removes every Virtual Controller, restores every Hidden Controller and returns once both are done. Dropping the Supervisor drops its bus first, which removes any Virtual Controller still there.

A Controller that cannot be opened, for example while another program holds it, is retried at the next scan (surfacing it is #52). Paths found to be Virtual Controllers are remembered while present and not opened again. A HID enumeration failure is shown and retried.

## Drivers

Detection goes through the `Drivers` trait, so any combination of results can be injected in tests. It runs at start and when the player asks, and must see a driver installed since the last time if the driver allows it without a restart. Nothing downloads, installs or runs anything: the window only links to the publishers' official installers. Without ViGEmBus nothing is Bridged; without HidHide Bridges work but games may see a Controller twice. A HidHide that another program is using is installed, so it is not listed as missing.

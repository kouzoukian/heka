# heka

Lets games treat a PlayStation controller as an Xbox or PS4 controller.

## Language

**Controller**:
A real DualShock 4 or DualSense held by a player. It is one Controller however many Links it has.
_Avoid_: pad, gamepad, device, physical controller

**Link**:
One live connection from a Controller to the PC, over USB or Bluetooth.
_Avoid_: connection, transport, cable

**Active Link**:
The Link heka reads from when a Controller has more than one. USB wins over Bluetooth.

**Virtual Controller**:
The controller heka presents to games in place of a Controller.
_Avoid_: emulated device, output device

**Output Type**:
The kind of Virtual Controller a Controller drives: Xbox or PS4. Remembered per Controller. A Profile may override it.
_Avoid_: mode

**Profile**:
A named set of choices for how a Controller's input becomes what games receive: its Bindings and, optionally, an Output Type. A Controller uses the Default Profile, which changes nothing and cannot be edited or deleted, until the player chooses another. Remembered per Controller.
_Avoid_: mapping, configuration, preset

**Binding**:
What one input of a Profile drives: another button, a d-pad direction or a trigger pressed all the way on the Virtual Controller, a key or key combination, a mouse button or a wheel step on the PC, or nothing.
_Avoid_: mapping, assignment

**Mapper**:
The step that turns a Controller's state into the state its Virtual Controller shows and the keys and mouse buttons it holds, by the Controller's Profile.

**Bridge**:
The live pairing of one Controller with one Virtual Controller, carrying input to the game and Feedback back.
_Avoid_: transcription, mapping, session

**Feedback**:
Rumble and lightbar colour that a game sends to a Virtual Controller and that reaches the Controller.
_Avoid_: output, notification

**Preferences**:
What the player chose for one Controller: how strongly it rumbles, whether its motors are swapped, the lightbar colour while no game sets one, its player lights and whether it is Hidden. They shape the Feedback on its way to the Controller.
_Avoid_: configuration

**Held**:
A Bridge whose Controller has lost every Link. The Virtual Controller stays, with everything released, for a limited time.
_Avoid_: grace period, suspended

**Stopped**:
A Controller the player has told heka not to Bridge.
_Avoid_: disabled, paused

**Turn off**:
To end a Controller's Bluetooth Link from the PC, by hand or after its idle time, so that it saves its battery.
_Avoid_: disconnect, power off, Stop

**Idle time**:
How long a Controller on Bluetooth may go unused before heka turns it off. Chosen per Controller.
_Avoid_: timeout, sleep timer

**Hidden**:
A Controller that heka can see and games cannot.
_Avoid_: cloaked, blocked

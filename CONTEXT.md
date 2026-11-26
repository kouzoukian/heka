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
The kind of Virtual Controller a Controller drives: Xbox or PS4. Remembered per Controller.
_Avoid_: mode, profile

**Bridge**:
The live pairing of one Controller with one Virtual Controller, carrying input to the game and Feedback back.
_Avoid_: transcription, mapping, session

**Feedback**:
Rumble and lightbar colour that a game sends to a Virtual Controller and that reaches the Controller.
_Avoid_: output, notification

**Preferences**:
What the player chose for one Controller: how strongly it rumbles, the lightbar colour while no game sets one, its player lights and whether it is Hidden. They shape the Feedback on its way to the Controller.
_Avoid_: profile, configuration

**Held**:
A Bridge whose Controller has lost every Link. The Virtual Controller stays, with everything released, for a limited time.
_Avoid_: grace period, suspended

**Stopped**:
A Controller the player has told heka not to Bridge.
_Avoid_: disabled, paused

**Hidden**:
A Controller that heka can see and games cannot.
_Avoid_: cloaked, blocked

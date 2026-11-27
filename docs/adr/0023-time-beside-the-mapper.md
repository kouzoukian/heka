# What takes time runs beside the Mapper, on the input thread

## The running mapper

The Mapper of [ADR 0021](0021-profiles-and-the-mapper.md) stays a pure step: what a Controller's present state shows and holds. Anything that depends on time or on what came before belongs to a `RunningMapper`, one for each Bridge, which asks the Mapper and remembers the rest. It is given the time and has no clock of its own, so it is tested with made-up instants.

- The input thread hands it each report, writes what it returns to the Virtual Controller, and only then lets it act on the keyboard and mouse. It never sleeps or waits, so it adds no delay to input.
- When the Mapper changes it starts over: what it held is released and what it remembered is dropped. It ends with its Bridge.

## Sticks as mouse, scroll and keys

Each stick of a Profile is a Virtual Controller stick, or moves the pointer, turns the wheel, or holds keys: WASD, the arrows, or four chosen ones. It belongs to the physical stick, like its tuning ([ADR 0021](0021-profiles-and-the-mapper.md)). A stick used otherwise rests centred on the Virtual Controller, on the side the swap would have given it.

- A Profile file writes it as `"left_stick_use": {"use": "mouse", "speed": 100}`, with `"keys": {"up": [17]}` for chosen keys, and writes nothing for a stick that is a stick. A field that cannot be read keeps its default.
- The stick's dead zone applies, and for the pointer and the wheel its curve. The other tuning shapes only a Virtual Controller stick. A pointing stick ignores less than 6% of travel and a key stick less than 25%, whatever the dead zone, because a stick at rest is not exactly centred and a key is all or nothing.
- Keys go by eighths of a turn: within 67.5 degrees of a direction its key is held, so a diagonal holds two.
- At full travel and 100% speed the pointer moves 1000 pixels a second and the wheel turns 20 notches a second. Speed is 10 to 300%.

## Movement

Movement is a speed times the time since the previous report, so it is the same however often reports arrive, on USB or Bluetooth. What is left under a whole pixel or notch is carried to the next report, so slow movement is not rounded away, and dropped once the stick rests. A gap longer than 100 ms counts as 100 ms, so a stalled Link does not make the pointer jump. Movement happens only when a report arrives.

Pointer and wheel movement go through `DesktopOutput` with the keys ([ADR 0022](0022-keyboard-and-mouse-output.md)). Windows applies the player's own pointer speed and acceleration to it, as to a mouse.

## The touchpad as a mouse

A Profile sends the touchpad to the Virtual Controller or lets it move the mouse. As a mouse its fingers are left out of what the Virtual Controller shows, while its click stays an input with Bindings like any other, so an unbound click still reaches the game and a click bound to a mouse button drags. The file writes `"touchpad": {"use": "mouse", ...}` and nothing for a touchpad left alone.

`Gestures` in `heka-core` follows one touch, from the first finger down to the last one up:

- One finger moves the pointer by its own travel, a pixel for each touchpad unit at 100%. Sensitivity is 25 to 300%, and vertical movement can be inverted. Fingers are told apart by their contact numbers, so a new one never makes the pointer jump.
- Once two fingers have been down, only two fingers do anything until all are lifted: dragged, they turn the wheel a notch for every 60 units, up and down and sideways, content following the fingers.
- A tap is a touch of at most 250 ms in which no finger moved more than 40 units and the touchpad was not pressed. With one finger it clicks the left button, with two the right, each only if the Profile says so; one is on and the other off to begin with.
- A tap's click is held for 30 ms, so that a program that looks only now and then sees it. The input thread ends it on a report or on a read that times out.
- A touch under way when the Mapper changes is ignored until the fingers are lifted.

## The gyro

A Profile leaves the gyro off, or has it move the pointer or add to the right stick, always, while a chosen button is held, or after that button toggles it. The button keeps its Binding. A change of Mapper turns a toggled gyro off.

- Turning (yaw, or roll if chosen) to the right moves right and tilting the far edge up (pitch) moves up. The signs follow the axes of [ADR 0008](0008-controller-state-and-calibration.md) as other tools read them and have not been tried on a Controller; the inversions are there if they are wrong.
- Below the dead zone, in degrees a second of both axes together, the gyro reads nothing, and above it the speed eases in from nothing. Sensitivity is set for each axis, 10 to 300%.
- As a mouse it moves 20 pixels for each degree at 100%, by the time on the sensor's own clock between reports, which does not depend on when a Link delivers them. `SensorClock` takes the difference of two timestamps modulo 2^20 ticks, which suits the DualShock 4's short clock and the DualSense's long one alike, and counts nothing over a gap of more than 200 ms, which the short clock could not measure.
- As a stick it pushes the right stick by the speed of turning, fully at 180 degrees a second at 100%, added to what the Virtual Controller's right stick shows after tuning and swapping, each axis held within its range. A still gyro leaves the report untouched.

### Drift

A gyro at rest rarely reads zero. The player asks for a measurement, per Controller, and `settings.json` keeps it as `gyro_drift` in the Controller's entry; it is subtracted before anything else. `DriftMeter` averages a second of reports on the input thread, of a Bridge or a Watch, and refuses a Controller that moved: a spread of more than 4 degrees a second on an axis, or an average of more than 20. It gives up after 5 seconds without enough reports, and when the Link or the Bridge ends. The motion the Virtual Controller reports to games is left as the Controller sends it.

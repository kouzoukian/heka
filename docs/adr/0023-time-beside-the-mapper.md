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

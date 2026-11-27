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

# Virtual output through ViGEmBus

ViGEmBus was retired in 2023 but is proven in games, needs one driver install, and is driven in-process (`vigem-rust`). We use it for v1 to create every Virtual Controller, behind a backend seam so another driver can replace it. VIIPER was rejected for now: it is maintained but young, needs usbip-win2, and runs a separate GPL-3.0 server.

The seam is the `VirtualBus` and `VirtualController` traits in `heka-core`: it decides what a Virtual Controller shows, and a backend in `heka-windows` only makes it exist and moves reports. Another driver replaces ViGEmBus by implementing both. A Virtual Controller is Xbox (Xbox 360) or PS4 (DualShock 4 with touchpad, motion and lightbar), and its input thread owns it.

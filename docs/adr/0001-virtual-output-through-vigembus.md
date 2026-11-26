# Virtual output through ViGEmBus

ViGEmBus was retired in 2023 but is proven in games, needs one driver install, and is driven in-process (`vigem-rust`). We use it for v1 to create every Virtual Controller, behind a backend seam so another driver can replace it. VIIPER was rejected for now: it is maintained but young, needs usbip-win2, and runs a separate GPL-3.0 server.

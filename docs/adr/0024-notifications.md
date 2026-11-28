# Notifications are WinRT toasts for an identity heka registers itself

heka tells the player about a Controller through Windows toasts, for low battery, a failed Bridge, and a Controller connecting or being removed. `heka-core`'s `Alerts` decides what is worth saying from the Controllers it is shown, with a fake clock in tests; `heka-windows` shows it and takes no decisions.

- A toast needs an identity. heka registers the AUMID `Kouzoukian.Heka` under `HKCU\Software\Classes\AppUserModelId`, with its name and an icon written to `%LOCALAPPDATA%\heka\notification.png`. It is a portable zip, so there is no installer to do it. Removing heka leaves that registry key behind ([README](../../README.md#files-updating-and-removing)).
- A balloon from the tray was rejected: `tray-icon` exposes the window but not the icon id the balloon needs.
- Windows can turn notifications off for the player. `Show` then returns no error and nothing appears, so heka reads the setting and logs that the toast was not shown.
- Low battery is said once at 20% or less while not charging, and again only after the Controller charges or rises above 30%. A Bridge failure is said once, and again after the Bridge has stayed up for 10 seconds, or after Stop or removal. A Controller present when heka starts is not announced as connected.
- Each toast has a tag from the Controller and the kind of alert, so a repeat replaces the earlier one.
- `examples/notification_spike.rs` ([ADR 0017](0017-test-fixtures-and-development-tools.md)) shows one toast. The delivery has not been seen on a PC with notifications turned on.

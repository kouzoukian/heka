# The log stays on the PC, and writing it never waits

Release builds open no console, so a log is the only record a player can attach to a report. It holds Controller addresses and device paths; heka sends it nowhere.

## Files

- `%LOCALAPPDATA%\heka\heka.log` is the current run and `heka.previous.log` the one before it. At start the current file is renamed over the previous one, unless it is empty. Only the copy that goes on to run does this: a second launch ends before touching the log.
- Each file stays within 1 MiB. A full one is rewritten with its first line, which names the version, a note that entries were left out, and its newest half. Trimming within one file means a long run never pushes out the run before it.
- Entries carry the PC's local time (`GetLocalTime`), because a player reports when something went wrong by their own clock.
- A log that cannot be opened or written leaves heka running, with the entries still listed in the window.

## Writing

- A caller takes the time and sends the message down a channel; the `heka-log` thread writes it. Nothing that logs waits for the disk, the input thread included.
- A message identical to one written less than a minute ago is written at most 3 times in that minute. The next one after it says how many were left out. A Bridge that fails every 2 seconds therefore cannot flood the log.
- A panic is written without that limit, by a panic hook that waits up to a second for the writer, then runs the earlier hook. Quitting waits the same way.
- Debug builds also print each entry on the console.

## What is logged

Start and version, the drivers found at each check, Links found and gone, every Bridge change, each change made to HidHide, a newer release at each check that finds one, and errors. Errors that last, such as a failed Bridge or a hiding warning, are logged when they appear or change rather than at every retry. HidHide changes are recorded by `Hiding` as it makes them, so a retry that changes nothing records nothing.

## Wording

`heka-core` returns typed errors and changes; `heka-app`'s `text.rs` words them for both the window and the log. The words inside an error from Windows, `hidapi` or ViGEmBus arrive as they are. `heka-windows` hands back what went wrong while opening a Link (no Feedback, no calibration) instead of printing it.

## In the window

The log view lists the latest 500 entries of this run, newest first. Copy puts them on the clipboard oldest first, as the file has them. Open folder starts `explorer.exe` from `%SystemRoot%`.

`GetLocalTime` writes into a `SYSTEMTIME` that heka owns for the length of the call, which is what makes that `unsafe` block sound.

# Windows backend: integration and manual tests

Written for: maintainers running the Windows backend on a real desktop before a release.

Automated tests run with `cargo test --workspace`. Tests that need a live desktop, touch
the user's registry, or launch programs are marked `#[ignore]`. Run them explicitly:

```powershell
cargo test -p zones-win -- --ignored --test-threads=1
```

Run ignored tests one at a time with `--test-threads=1`. Some of them share the process-wide
event subscription.

| Test                                   | What it checks                                              | Side effects                          |
| -------------------------------------- | ----------------------------------------------------------- | ------------------------------------- |
| `hotkey::register_and_unregister`      | `RegisterHotKey` accepts and releases `Ctrl+Alt+Shift+Win+F24` | None                              |
| `events::reports_moves_of_a_real_window` | A `Moved` event arrives for a window moved via `place`    | Starts and kills `notepad.exe`        |
| `autostart::round_trip_autostart`      | Writes, reads and removes the `Run` entry                   | Temporarily edits `HKCU\...\Run`     |

## Manual checklist

Use two monitors with different scale factors (for example 100% and 150%). Run the editor or
CLI with a config that has rules for at least two apps.

### Monitors and DPI

- [ ] `tpt-zones-cli monitors` lists every attached display, with the primary flagged.
- [ ] Work areas exclude the taskbar on each monitor (set the taskbar to auto-hide and back).
- [ ] Scale factors match **Settings → System → Display** for each monitor.
- [ ] Unplugging or adding a monitor is reflected in a fresh `monitors` call.
  *(Live `MonitorsChanged` events are not implemented yet; see `todo.md`.)*

### Window enumeration

- [ ] Visible application windows appear in `tpt-zones-cli windows`.
- [ ] Taskbar, desktop, tooltips, and owned dialogs do **not** appear.
- [ ] Minimised windows appear with the `(minimised)` marker.
- [ ] Windows on virtual desktops other than the current one are not listed as visible, and do not break enumeration.
- [ ] UWP apps (Settings, Calculator) appear with a sensible process name.

### Placement

- [ ] A window moved to a zone's rectangle lines up with the zone edges, with no gap and no overlap, on both monitors.
- [ ] Windows with invisible resize borders (most Win10/11 apps) fill the zone exactly. The visible frame matches the zone, not the outer frame.
- [ ] A minimised window is restored and then placed.
- [ ] Placing a window on a monitor with a different scale factor leaves no 1 px seams.
- [ ] `apply` is idempotent: a second run reports zero placements.

### Events

- [ ] Opening and closing an application produces `Created` and `Destroyed` events.
- [ ] Switching focus produces `Focused` events.
- [ ] Dragging a window produces `Moved` events that track the drag.
- [ ] Minimising and restoring produces `MinimizedChanged` events in both directions.
- [ ] Only one subscription is accepted at a time, and dropping it allows a new one.

### Hotkeys

- [ ] A registered combination fires its handler from any application, including elevated ones.
- [ ] Holding the key does not fire repeatedly (`MOD_NOREPEAT`).
- [ ] Registering a combination that another app already owns returns a clear error.
- [ ] Dropping the subscription releases the combination for other apps.
- [ ] Exiting the process releases every combination.

### Autostart

- [ ] Enabling autostart and signing out and back in starts the app.
- [ ] Disabling autostart removes the `Run` entry and nothing else.

### Error handling

- [ ] Placing a window that closed a moment earlier returns an error, and the other windows in the batch still move.
- [ ] Killing a window's process while it is being tracked does not crash the event thread.

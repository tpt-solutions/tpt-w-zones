# Architecture

This document records the design of the platform-independent core and its contracts with
the backends, the editor and external clients. The code is the source of truth for
details; this file explains the intent and the decisions behind them.

Written for: contributors who will implement a backend, change a shared contract, or
write a client.

## 1. Components

```
                ┌──────────────────────────┐
  editor (Tauri)│                          │  CLI / scripts
  ─────────────▶│   core service           │◀───────────── (planned)
   IPC (JSON)   │   zones-core + backend   │
                │                          │◀── zones-archon-bridge
                └────────────┬─────────────┘    (Archon IPC, same messages)
                             │ traits
              ┌──────────────┼───────────────┐
              ▼              ▼               ▼
          zones-win     zones-linux     (test fakes)
          (Win32)       (Wayland)
```

- **`zones-core`** holds all logic that does not depend on an OS: geometry, the data model,
  hotkey parsing, config validation, the IPC protocol and the backend traits. It has no
  OS dependencies and no `unsafe`.
- **Backends** (`zones-win`, `zones-linux`) implement the traits in `zones-core::backend`.
- **`zones-archon-bridge`** adapts the same IPC messages to Archon's service IPC.
- **The editor** is a Tauri app. Its Rust side calls `zones-core` directly for config work
  (validation, defaults). Live window operations go through the core service over IPC.

## 2. Platform boundary (`zones_core::backend`)

Three traits cover everything a platform must supply:

| Trait            | Responsibility                                                       |
| ---------------- | -------------------------------------------------------------------- |
| `WindowBackend`  | Enumerate monitors and windows, move/resize a window, emit events.   |
| `HotkeyBackend`  | Register a global `Hotkey` with a handler.                           |
| `Subscription`   | RAII handle. Dropping it cancels an event or hotkey registration.    |

Rules for implementers:

1. **Coordinates are physical pixels.** Backends convert from DPI-scaled logical coordinates
   and report `Monitor::scale_factor` so the core can reason about DPI.
2. **`WindowId` is opaque.** Only the backend that produced it may interpret it.
3. **Events are advisory.** A `Moved` event may arrive after the window has moved again. The
   core re-queries state before acting on it.
4. **Handlers must not block.** Backends may call sinks on their own thread. Heavy work
   belongs on a queue.
5. **Errors are values.** Return `Err`; do not panic on OS failures such as a window
   closing between enumeration and placement.
6. **Subscriptions are cancelled on drop.** Backends must tolerate cancel being called
   from any thread.

### Event model

`WindowEvent` is a closed enum: `Created`, `Destroyed`, `Focused`, `Moved`,
`MinimizedChanged` and `MonitorsChanged`. Backends translate native events into these
and drop anything they cannot map. Adding a variant is a breaking change for IPC
subscribers, so treat it as a protocol change (section 6).

The core subscribes once and fans events out to in-process listeners and IPC subscribers.
Platform hooks therefore exist in one place per backend.

## 3. Hotkeys

- **Text form:** `Modifier+...+Key`, for example `Ctrl+Alt+Left`. Modifiers are
  `Ctrl`, `Alt`, `Shift` and `Super` (aliases `Win`, `Windows`, `Meta`). Named keys are
  `Left`, `Right`, `Up`, `Down`, `Home`, `End`, `PageUp`, `PageDown`, `Space`, `Enter`,
  `Tab`, `Escape`, `Backspace`, `Delete` and `Insert`. Single characters are upper-cased.
  Function keys (`F1`–`F24`) are accepted as-is.
- **Canonical output** is `Super+Ctrl+Alt+Shift+Key`. Parse then format round-trips.
- **Platform mapping** happens in the backend. Win32 maps to `RegisterHotKey` virtual
  keys. Wayland maps to compositor bindings, and may refuse global grabs (see Phase 3 in
  `todo.md`). Hotkey registration can therefore fail at runtime, and callers must handle
  that.

## 4. Data model

```
Config
 ├─ general { autostart, log_level }
 ├─ hotkeys { action → Hotkey }
 ├─ layouts [ Layout { id, name, zones [ Zone { id, name, rect: NormRect } ] } ]
 └─ profiles [ Profile { name, monitors { stable_monitor_name → layout_id } } ]
```

- **Normalised zones.** `NormRect` stores fractions of the work area, not pixels.
  One layout works on every resolution and DPI. `Zone::to_rect` converts to pixels
  at placement time. It rounds edges, not sizes, so adjacent zones meet without gaps
  at any width.
- **Work area, not bounds.** Zones tile the work area, which excludes the taskbar and
  panels. This is intentional.
- **Stable monitor names.** Profiles key monitors by a name that survives reboots
  (Windows device name, Linux connector name). `MonitorId` is session-scoped and must
  not be stored.
- **Validation** runs at load time and rejects: duplicate ids, empty or out-of-range
  zones, profiles that reference unknown layouts, and a missing `active_profile`.

## 5. Configuration file

- Format: TOML, file `config.toml`, in the per-user config directory
  (`%APPDATA%\tpt-zones\` on Windows, `$XDG_CONFIG_HOME/tpt-zones/` on Linux).
- `schema_version` is mandatory and must equal `zones_core::config::SCHEMA_VERSION`
  (currently `1`). Newer or older values are rejected with a clear message. Migration
  code will be added when there is a second version.
- Unknown keys are **errors**, not warnings. Typos should fail loudly.
- Writes go through `Config::to_toml`, so the file always round-trips through the
  validator.

A complete example is in `README.md`.

## 6. IPC protocol (`zones_core::ipc`)

- **Transport:** named pipe `\\.\pipe\tpt-zones` (Windows), Unix socket
  `$XDG_RUNTIME_DIR/tpt-zones.sock` (Linux). Archon carries the same messages over its own IPC.
- **Framing:** newline-delimited JSON, UTF-8, at most 1 MiB per line.
- **Handshake:** the first message is `Hello { client, protocol }`. The server answers
  `Hello { server, protocol }`. Versions must match exactly. Any mismatch closes the
  connection.
- **Message tags:** `"type"` in `snake_case`. Requests are `hello`, `list_windows`,
  `list_monitors`, `snap_window`, `reload_config` and `subscribe`. Responses are `hello`,
  `ok`, `error`, `windows`, `monitors` and `event`.

### Versioning rules

| Change                                     | Action                         |
| ------------------------------------------ | ------------------------------ |
| Add a request or response variant          | Bump `PROTOCOL_VERSION`        |
| Add a field to an existing message         | Bump `PROTOCOL_VERSION`        |
| Add a `WindowEvent` variant                | Bump `PROTOCOL_VERSION`        |
| Fix a bug in message handling, no shape change | No bump                    |

Exact-match negotiation is deliberately strict while the protocol is young. Relaxing it to
"minimum supported version" is a later decision.

## 7. Logging

- `zones_core::logging::init(default_level)` installs a `tracing` subscriber on stderr.
- Filter precedence: `TPT_ZONES_LOG` env var, then `general.log_level`, then `info`.
- Backends log through `tracing` with a target matching their crate name
  (`zones_win`, `zones_linux`). File logging and rotation are planned with the service
  (Phase 2).

## 8. Testing strategy

- **Core:** pure unit tests in each module. Run with `cargo test -p zones-core`.
- **Backends:** a shared conformance suite written against the traits, run with a fake
  backend in core tests and against the real OS in `#[ignore]` integration tests. Those
  need a desktop session and are run manually (checklists in `todo.md`).
- **Editor:** type checking and lint in CI. Behaviour tests will be added with Phase 4.

## 9. Open questions

- Whether `Layout` should allow nested splits, or stay a flat list of rectangles
  (affects the Phase 4 editor and the zone-to-rect math).
- Whether rules (Phase 5) live in `config.toml` or in separate script files.
- Whether the core service runs as one process per user or one per session.
- Whether Wayland global hotkeys are possible on GNOME without an extension (Phase 3
  research item).

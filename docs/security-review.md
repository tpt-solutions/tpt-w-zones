# Security review: input hooking and the IPC surface

Written for: maintainers and reviewers. Scope is the code at `feat` tip on 2026-11-12:
the Windows service (`zones-win`, `tpt-zones-cli serve`), its named-pipe IPC, the tray, the
config file, and the editor host. Linux is not covered: `zones-linux` is still a placeholder.

Status legend: **fixed**, **mitigated**, **open**, **accepted** (with the reason).

## Attack surface

| Surface | Who can reach it | Notes |
| ------- | ---------------- | ----- |
| Named pipe `\\.\pipe\tpt-zones` | Local users, subject to the pipe's access list | Requests: list windows and monitors, snap a window, reload config, subscribe to events |
| Global hotkeys | Anything running in the user's session | Hotkeys run configured actions only |
| Window event hooks (`SetWinEventHook`, out-of-context) | Process-wide | Read-only: the hook reports events and never injects input |
| Config file `%APPDATA%\tpt-zones\config.toml` | The user, and anything running as them | Parsed with `deny_unknown_fields` and validated before use |
| Tray menu | The logged-in user | Reload, apply rules, quit |
| Editor host commands | The editor window | Load, validate, save config; list monitors; preview overlay |

## Findings

### 1. Pipe access list (fixed)

**Was:** the pipe was created with the default security descriptor. Windows documents that
default as granting read access to Everyone, which is broader than intended.

**Now:** `zones-win::pipe` builds an explicit descriptor, `D:(A;;GA;;;OW)(A;;GA;;;BA)(A;;GA;;;SY)`:
the creating user, Administrators and SYSTEM, with full access. Other local users are denied.
The descriptor is built once per process and freed with `LocalFree`.

Verify on Windows: connect as a second standard user and expect access denied. This has not
been run yet, because it needs a second account.

### 2. Input hooking (accepted, with limits)

The event hook is out-of-context and read-only. It does not inject or block input, and it
skips the service's own process (`WINEVENT_SKIPOWNPROCESS`). The keyboard path uses
`RegisterHotKey`, which the OS delivers to the registering thread only. Neither mechanism
sees keystrokes that are not registered hotkeys.

Limit: `RegisterHotKey` is per-user. A hotkey cannot fire while an elevated window has focus
unless the service itself is elevated, which it deliberately is not.

### 3. Requests that change state (mitigated)

`snap_window` and `reload_config` change state. Access is limited by finding 1: only the
owning user, Administrators and SYSTEM can send them. The server does not yet check the
protocol access levels in `zones-archon-bridge::access_for`. That check should be added when
the bridge is wired in, so that a future read-only client cannot send `snap_window`.

Mitigation until then: the pipe access list (finding 1).

### 4. Config parsing (fixed by design)

Config files are untrusted input in the sense that a user may paste one from elsewhere.

- Unknown keys are rejected (`deny_unknown_fields` on every struct that is read from the file).
- Hotkey strings are parsed and validated; unknown modifiers and keys are rejected.
- Layout zones are checked against the work area (0 to 1, positive size), so a zone cannot
  request an out-of-range rectangle.
- Rules and hotkeys must reference existing layouts, zones and profiles.
- The file is read once per reload. There is no code execution path from the config.

### 5. Placement and window geometry (accepted)

A placement computes a rectangle from validated fractions and passes it to `SetWindowPos`.
Arithmetic on pixel coordinates uses checked conversions where a value comes from the OS
(`try_from`), and saturates on overflow in `Rect::right/bottom`.

### 6. Panics across FFI (fixed)

The event hook and hotkey callbacks run under `catch_unwind`, so a panic in a sink cannot
unwind into the OS. The overlay and display window procedures do the same.

### 7. Unsafe code (accepted, bounded)

`unsafe` appears only in the Windows modules that call Win32, each with `#![allow(unsafe_code)]`
at the top of the module and a `// SAFETY:` comment on each block. `zones-core` and the rest of
the workspace are `deny(unsafe_code)`. Review each new `unsafe` block against its SAFETY comment.

### 8. Tray and overlay windows (accepted)

Both use hidden or message-only windows. The overlay is click-through (`WS_EX_TRANSPARENT`)
and never takes focus (`WS_EX_NOACTIVATE`), so it cannot capture clicks or keys.

### 9. Editor host (accepted)

The editor's commands can read and write only the user's config path, and can draw overlays.
They take the configuration from the webview, so the webview is trusted code. The CSP is
`default-src 'self'`, which blocks remote scripts. Tauri capabilities are limited to
`core:default`.

## Open items

- Add the protocol access check (`access_for`) to the pipe server (finding 3).
- Run the second-user test for finding 1 on a Windows machine with two accounts.
- Review the Linux backend when it exists: D-Bus and compositor access will be new surfaces.
- Telemetry and crash reporting are not implemented. If added, they must be opt-in and
  must not send window titles or process names.

# tpt-zones

**Advanced window tiling and workspace manager** — a keyboard-driven, scriptable
alternative to FancyZones and DisplayFusion.

tpt-zones is built by [TPT Solutions](https://github.com/tpt-solutions). A Rust core
handles window and monitor work with low latency; a TypeScript (Tauri) visual editor
lets you design zone layouts.

> **Status: early development.** The repository currently contains the workspace,
> shared core types, config format, IPC protocol and editor shell. Platform backends
> and the zone editor UI are still in progress. See [`todo.md`](todo.md).

## Concept

- **Zones and layouts** — define named layouts of normalised zones that work on any
  resolution or DPI.
- **Monitor-aware profiles** — bind a layout to each monitor, and switch profiles
  (docked, laptop, presenting) in one step.
- **Keyboard first** — global hotkeys snap the focused window into a zone.
- **Scriptable** — a CLI and a documented config file (planned) drive per-app and
  per-profile rules.
- **Lightweight** — no background bloat; the core is a small Rust service.

## Platforms

| Platform                 | Status                                         |
| ------------------------ | ---------------------------------------------- |
| Windows 10/11            | First target — backend in progress             |
| Linux (Wayland: GNOME, KDE) | Planned — research phase                    |
| Archon                   | Planned — user-space service via `tpt-archon-bridge` |

## Repository layout

```
crates/
  zones-core/            shared types, traits, config, IPC protocol (no OS calls)
  zones-win/             Windows backend (Win32)
  zones-linux/           Linux/Wayland backend
  zones-archon-bridge/   Archon integration
apps/
  editor/                Tauri + TypeScript visual zone editor
    src/                 frontend (Vite + TypeScript)
    src-tauri/           Tauri host, wires commands to zones-core
docs/
  architecture.md        design of the trait boundary, data model, IPC
```

## Configuration

Configuration is TOML. A minimal example:

```toml
schema_version = 1
active_profile = "docked"

[hotkeys]
"snap:halves:left" = "Ctrl+Alt+Left"

[[layouts]]
id = "halves"
name = "Halves"
zones = [
  { id = "left",  name = "Left",  rect = { x = 0.0, y = 0.0, w = 0.5, h = 1.0 } },
  { id = "right", name = "Right", rect = { x = 0.5, y = 0.0, w = 0.5, h = 1.0 } },
]

[[profiles]]
name = "docked"
monitors = { "DISPLAY1" = "halves" }
```

See [`docs/architecture.md`](docs/architecture.md) for the full schema, and [`docs/scripting.md`](docs/scripting.md) for rules, hotkey actions and the CLI.

## Running (Windows)

```powershell
tpt-zones-cli validate                  # check your config
tpt-zones-cli serve                     # run the service: hotkeys, rules, IPC
tpt-zones-cli ctl windows               # talk to the running service
```

## Building

Prerequisites:

- Rust 1.75 or newer (`rustup`)
- Node.js 20 or newer
- Windows: WebView2 runtime (ships with Windows 11)
- Linux: WebKitGTK 4.1 and the Tauri system dependencies listed in the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

```sh
# Rust workspace: core, backends and tests
cargo build --workspace
cargo test --workspace

# Editor frontend
cd apps/editor
npm install
npm run build

# Editor desktop app (development)
npm run tauri dev
```

Lint and format:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cd apps/editor && npm run lint && npm run format:check
```

## Contributing

Contributions are welcome. Read [`CONTRIBUTING.md`](CONTRIBUTING.md) first.

## Licence

Dual-licensed under either of

- [MIT licence](LICENSE-MIT)
- [Apache Licence, Version 2.0](LICENSE-APACHE)

at your option. Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work is dual-licensed as above, without additional terms.

---

Made by **TPT Solutions**.

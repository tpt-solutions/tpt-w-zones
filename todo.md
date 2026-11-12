# tpt-zones — Project Todo

Advanced Window Tiling & Workspace Manager (FancyZones/DisplayFusion alternative).
Rust core + TypeScript (Tauri) visual zone editor. Windows + Linux (Wayland) first,
Archon integration later. Dual-licensed MIT OR Apache-2.0. TPT Solutions.

## Phase 0 — Project & OSS Setup

- [x] Initialize git repo, `.gitignore` (Rust + Node/Tauri)
- [x] Set up Cargo workspace (`zones-core`, `zones-win`, `zones-linux`, `zones-archon-bridge`)
- [x] Scaffold `apps/editor` (Tauri + TypeScript)
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE`, dual-license notice in README
- [x] Write `README.md` (concept, features, build instructions, TPT Solutions attribution)
- [x] Write `CONTRIBUTING.md`
- [x] Set up GitHub issue templates and PR template
- [x] Set up CI (GitHub Actions): build/lint/test on Windows + Linux
- [x] Add code formatting/linting config (`rustfmt`, `clippy`, ESLint/Prettier)
- [ ] Register repo under TPT Solutions org, add branding/README badges

## Phase 1 — Core Architecture & Platform Abstraction

- [x] Design cross-platform trait boundary (window enumeration, move/resize, monitor info)
- [x] Design hotkey registration abstraction
- [x] Design core event loop / window event subscription model
- [x] Design workspace & zone data model (layouts, profiles, monitors)
- [x] Design config file format (e.g. TOML/JSON schema)
- [x] Design IPC protocol (core <-> editor, and future core <-> Archon bridge)
- [x] Set up `zones-core` crate with shared types and traits
- [x] Set up basic logging/tracing infrastructure

## Phase 2 — Windows Backend (`zones-win`)

- [x] Implement Win32 window event hooks (foreground/move/resize/create/destroy)
- [x] Implement global hotkey registration
- [x] Implement multi-monitor + DPI-aware geometry detection
- [x] Implement zone snapping/tiling logic (move/resize windows into zones)
- [x] Implement system tray icon + menu
- [x] Implement autostart (registry/startup entry)
- [x] Write integration tests / manual test checklist for Windows backend

## Phase 3 — Linux Backend (`zones-linux`, Wayland)

- [ ] Research/prototype Wayland protocol integration (research notes: docs/linux-research.md; prototype pending) (layer-shell, foreign-toplevel mgmt)
- [ ] Implement window enumeration & event subscription for GNOME (via extension/portal if needed)
- [ ] Implement window enumeration & event subscription for KDE (via KWin scripting/protocol)
- [ ] Implement global hotkey registration under Wayland constraints
- [ ] Implement multi-monitor geometry detection
- [ ] Implement zone snapping/tiling enforcement
- [ ] Implement autostart (systemd user service / desktop entry)
- [ ] Write integration tests / manual test checklist for GNOME and KDE

## Phase 4 — Visual Zone Editor (`apps/editor`, Tauri + TypeScript)

- [x] Build zone layout canvas (draw/resize/delete zones)
- [x] Build monitor-aware layout switching UI
- [x] Build save/load layout profile UI
- [x] Build live preview (highlight zones on real monitors)
- [x] Wire up Tauri commands/IPC to `zones-core`
- [x] Build settings screen (hotkeys, autostart, general preferences)
- [x] Add light/dark theme support
- [ ] Package editor as a Tauri desktop app (Windows + Linux)

## Phase 5 — Automation & Scripting

- [x] Design scriptable layout rules (per-app, per-monitor, per-profile)
- [x] Implement rule engine in `zones-core`
- [x] Build CLI for scripting/automation (`tpt-zones-cli`)
- [x] Build hotkey/keybinding customization UI (editor Settings: Record captures a key combination; text entry also works)
- [x] Implement config import/export
- [x] Document scripting API and examples

## Phase 6 — Archon Integration (`zones-archon-bridge`)

- [x] Study `tpt-archon` IPC interface (findings: docs/archon-integration.md) (https://github.com/tpt-solutions/tpt-archon)
- [ ] Implement `tpt-archon-bridge` IPC client crate
- [ ] Implement zero-latency layout enforcement as an Archon user-space service
- [ ] Package/deploy `zones-archon-bridge` for Archon
- [ ] Write integration tests / manual test checklist for Archon

## Phase 7 — Packaging, Distribution & Polish

- [ ] Build Windows installer (MSI) + winget manifest
- [ ] Build Linux packages (AppImage/Flatpak, or distro-specific packages)
- [ ] Implement auto-update mechanism
- [ ] Implement opt-in crash reporting/telemetry
- [ ] Build documentation site (usage, config reference, FAQ)
- [ ] Benchmark latency/performance vs. FancyZones and komorebi
- [x] Accessibility pass on editor UI (keyboard zone list, tab ARIA links, focus rings, WCAG AA contrast checked; screen-reader testing not done)
- [ ] Security review (hotkey/input hooking, IPC surface)

## Phase 8 — Launch

- [ ] Finalize versioned 1.0 release checklist
- [ ] Write release notes / changelog
- [ ] Prepare marketing/announcement materials under TPT Solutions
- [ ] Set up community channels (GitHub Discussions/Discord)
- [ ] Set up support/triage process for issues
- [ ] Publish 1.0 release

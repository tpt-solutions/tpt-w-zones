# Linux (Wayland) backend: research notes

Written for: the maintainer implementing `zones-linux`. Linux is verified through CI only
(`ubuntu-latest`), so every claim here that is marked **verify** must be checked on a real
session or in a CI job before the code depends on it.

## The core constraint

Wayland does not let ordinary clients move other clients' windows. A tiling manager needs
help from the compositor, and the help differs by desktop:

| Desktop | Window list | Move / resize other windows | Global hotkeys |
| ------- | ----------- | --------------------------- | -------------- |
| GNOME (Mutter) | No public protocol | Only from inside the shell: a GNOME Shell extension | Shell keybindings, or the GlobalShortcuts portal **verify** |
| KDE Plasma (KWin) | KWin scripting API over D-Bus | KWin script (JavaScript) | KGlobalAccel, or the GlobalShortcuts portal **verify** |
| wlroots (Sway, Hyprland, River) | `zwlr_foreign_toplevel_management_unstable_v1` | Partial: activate, close, maximise, output moves. No arbitrary geometry **verify** per compositor | Compositor config, or the portal |

Consequence: `zones-linux` is not one implementation. It is an adapter per compositor family,
behind the same `WindowBackend` and `HotkeyBackend` traits the Windows backend implements.

## Protocols worth knowing

- **`ext-foreign-toplevel-list-v1`** (standard): lists toplevels with title and `app_id`.
  Read-only. Good for `windows()`, not enough for placement.
- **`wlr-foreign-toplevel-management-unstable-v1`**: wlroots-only. Can control state, not
  arbitrary geometry.
- **`wlr-layer-shell-unstable-v1`**: for panels, overlays and the zone-preview overlay windows.
  Not a placement mechanism.
- **`xdg-desktop-portal` GlobalShortcuts**: the portable route to global hotkeys on Wayland.
  Availability per desktop and version **verify**.

## Recommended path

1. **Compositor adapters, one at a time.** Start with KDE: a KWin script that exports window
   list and geometry over D-Bus, plus a `place` method. KWin's scripting API is the most
   capable public surface. GNOME follows as a Shell extension that does the same. wlroots
   comes last, because its geometry control is the most limited.
2. **Hotkeys.** Try the GlobalShortcuts portal first. Where it is missing, document a manual
   setup: bind a desktop shortcut to `tpt-zones-cli ctl snap ...` (the same IPC the service
   exposes on Windows via a named pipe, here a Unix socket at `$XDG_RUNTIME_DIR/tpt-zones.sock`).
3. **Autostart.** An XDG autostart desktop entry, or a systemd user service. Pick one and
   document it. The service is the same binary as on Windows.

## Testing without a desktop

CI runners have no display, but a headless compositor can run in a job:

- `sway` with `WLR_BACKENDS=headless` exercises the wlroots adapter.
- `kwin_wayland --virtual` (from the KDE packages) exercises the KWin adapter. **verify** that it runs headless on the runner image.
- GNOME's `mutter --headless` with `--wayland` **verify**. The Shell extension may need a full session.

Unit tests cover the pure parts: geometry conversion, rule matching, protocol framing. The
compositor adapters get integration tests, marked `#[ignore]` where a compositor is needed,
and run in a dedicated CI job.

## Open questions

- Does the GNOME extension need to be shipped separately from the app package? (Affects Phase 7 packaging.)
- Which minimum desktop versions do we support? GlobalShortcuts availability sets this.
- Can a KWin script place windows on all virtual desktops, or only the current one?
- Does the portal's GlobalShortcuts API work in a sandboxed (Flatpak) build?

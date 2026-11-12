//! Linux (Wayland) backend for tpt-zones.
//!
//! **Status:** placeholder. Wayland protocol research, GNOME/KDE window
//! enumeration, hotkeys and zone enforcement are tracked in `todo.md` (Phase 3).

pub use zones_core as core;

/// Platform identifier used in logs and IPC handshakes.
pub const PLATFORM: &str = "linux";

/// Returns `true` when this build targets Linux.
#[must_use]
pub const fn is_supported_target() -> bool {
    cfg!(target_os = "linux")
}

#[cfg(test)]
mod tests {
    #[test]
    fn platform_matches_target() {
        assert_eq!(super::is_supported_target(), cfg!(target_os = "linux"));
    }
}

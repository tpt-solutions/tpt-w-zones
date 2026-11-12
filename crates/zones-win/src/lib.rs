//! Windows backend for tpt-zones.
//!
//! **Status:** placeholder. Win32 event hooks, hotkeys, DPI-aware monitor
//! detection and zone placement are tracked in `todo.md` (Phase 2). This crate
//! currently only identifies the platform so the workspace builds everywhere.

pub use zones_core as core;

/// Platform identifier used in logs and IPC handshakes.
pub const PLATFORM: &str = "windows";

/// Returns `true` when this build targets Windows.
#[must_use]
pub const fn is_supported_target() -> bool {
    cfg!(windows)
}

#[cfg(test)]
mod tests {
    #[test]
    fn platform_matches_target() {
        assert_eq!(super::is_supported_target(), cfg!(windows));
    }
}

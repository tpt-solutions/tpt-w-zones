//! Windows backend for tpt-zones.
//!
//! Implements the `zones_core::backend` traits on Win32:
//!
//! - [`display`]: monitor enumeration, work areas and effective DPI.
//! - [`window`]: top-level window enumeration, filtering, and DWM-aware placement.
//! - [`events`]: `SetWinEventHook` subscription on a dedicated message-loop thread.
//! - [`hotkey`]: `RegisterHotKey` on a dedicated message-loop thread.
//! - [`autostart`]: the per-user `Run` registry entry.
//!
//! [`WinBackend`] ties these together. Win32 is inherently `unsafe`, so the
//! `unsafe_code` lint is allowed only in the modules that call it, and every
//! `unsafe` block documents its invariants.

pub use zones_core as core;

#[cfg(windows)]
pub mod autostart;
#[cfg(windows)]
pub mod display;
#[cfg(windows)]
pub mod events;
#[cfg(windows)]
pub mod hotkey;
#[cfg(windows)]
pub mod pipe;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod window;

#[cfg(windows)]
mod backend;
#[cfg(windows)]
pub use backend::WinBackend;

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

//! Archon integration for tpt-zones.
//!
//! **Status:** placeholder. The Archon IPC client (`tpt-archon-bridge`), the
//! user-space layout-enforcement service and its packaging are tracked in
//! `todo.md` (Phase 6). The intended design is that Archon carries the same
//! [`zones_core::ipc`] messages used by the Windows and Linux services.

pub use zones_core as core;

/// Service identifier reported to Archon.
pub const SERVICE_NAME: &str = "tpt-zones";

/// IPC protocol version this bridge speaks.
pub const PROTOCOL_VERSION: u32 = zones_core::ipc::PROTOCOL_VERSION;

#[cfg(test)]
mod tests {
    #[test]
    fn bridge_uses_core_protocol_version() {
        assert_eq!(super::PROTOCOL_VERSION, zones_core::ipc::PROTOCOL_VERSION);
    }
}

//! Archon integration for tpt-zones.
//!
//! Archon's bridge crate is an in-process Rust API (capabilities and a unified page cache)
//! with no wire protocol yet; its TODO schedules the wire protocol for a later phase. So this
//! crate does not open a connection. It re-exports the access model in `zones_core::access`,
//! which the Windows pipe server already enforces. When Archon capabilities are wired in,
//! these levels become the capability rights a client must hold.
//!
//! See `docs/archon-integration.md` for the study notes and the planned mapping.

pub use zones_core as core;
pub use zones_core::access::{access_for, authorise, Access, Grant};

/// Service identifier reported to Archon.
pub const SERVICE_NAME: &str = "tpt-zones";

/// IPC protocol version this bridge speaks.
pub const PROTOCOL_VERSION_BRIDGE: u32 = zones_core::ipc::PROTOCOL_VERSION;

#[cfg(test)]
mod tests {
    #[test]
    fn bridge_uses_core_protocol_version() {
        assert_eq!(
            super::PROTOCOL_VERSION_BRIDGE,
            zones_core::ipc::PROTOCOL_VERSION
        );
    }
}

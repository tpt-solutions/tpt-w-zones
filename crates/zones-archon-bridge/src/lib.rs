//! Archon integration for tpt-zones.
//!
//! Archon's bridge crate is an in-process Rust API (capabilities and a unified page cache)
//! with no wire protocol yet; its TODO schedules the wire protocol for a later phase. So this
//! crate does not open a connection. What it provides now is the part that does not depend on
//! the transport:
//!
//! - [`access_for`]: the access level each `zones_core::ipc` request needs. When Archon
//!   capabilities are wired in, these levels become the capability rights a client must hold.
//! - [`Grant`]: a set of access levels, checked against requests.
//!
//! See `docs/archon-integration.md` for the study notes and the planned mapping.

pub use zones_core as core;

use zones_core::ipc::{Request, PROTOCOL_VERSION};

/// Service identifier reported to Archon.
pub const SERVICE_NAME: &str = "tpt-zones";

/// IPC protocol version this bridge speaks.
pub const PROTOCOL_VERSION_BRIDGE: u32 = PROTOCOL_VERSION;

/// What a client may do with the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Access {
    /// Read window, monitor and event state.
    Observe,
    /// Change window placement or configuration.
    Manage,
}

/// The access a request needs. `None` means the request is allowed without any grant
/// (the handshake, which carries no privilege).
#[must_use]
pub fn access_for(request: &Request) -> Option<Access> {
    match request {
        Request::Hello { .. } => None,
        Request::ListWindows | Request::ListMonitors | Request::Subscribe => Some(Access::Observe),
        Request::SnapWindow { .. } | Request::ReloadConfig => Some(Access::Manage),
    }
}

/// A client's granted access levels. `Manage` implies `Observe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    observe: bool,
    manage: bool,
}

impl Grant {
    /// No access beyond the handshake.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            observe: false,
            manage: false,
        }
    }

    /// Read-only access.
    #[must_use]
    pub const fn observer() -> Self {
        Self {
            observe: true,
            manage: false,
        }
    }

    /// Full access.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            observe: true,
            manage: true,
        }
    }

    /// Returns `true` if this grant covers `access`.
    #[must_use]
    pub const fn allows(self, access: Access) -> bool {
        match access {
            Access::Observe => self.observe || self.manage,
            Access::Manage => self.manage,
        }
    }

    /// Returns `true` if this grant may perform `request`.
    #[must_use]
    pub fn permits(self, request: &Request) -> bool {
        match access_for(request) {
            None => true,
            Some(access) => self.allows(access),
        }
    }
}

#[cfg(test)]
mod tests {
    use zones_core::model::ZoneId;

    use super::*;
    use zones_core::backend::WindowId;

    fn snap() -> Request {
        Request::SnapWindow {
            window: WindowId(1),
            layout: "halves".into(),
            zone: ZoneId("left".into()),
        }
    }

    #[test]
    fn bridge_uses_core_protocol_version() {
        assert_eq!(PROTOCOL_VERSION_BRIDGE, zones_core::ipc::PROTOCOL_VERSION);
    }

    #[test]
    fn handshake_needs_no_grant() {
        let hello = Request::Hello {
            client: "x".into(),
            protocol: PROTOCOL_VERSION,
        };
        assert_eq!(access_for(&hello), None);
        assert!(Grant::none().permits(&hello));
    }

    #[test]
    fn observers_cannot_manage() {
        assert!(Grant::observer().permits(&Request::ListWindows));
        assert!(Grant::observer().permits(&Request::Subscribe));
        assert!(!Grant::observer().permits(&snap()));
        assert!(!Grant::observer().permits(&Request::ReloadConfig));
    }

    #[test]
    fn full_grant_covers_everything() {
        assert!(Grant::full().permits(&snap()));
        assert!(Grant::full().permits(&Request::ListMonitors));
    }

    #[test]
    fn no_grant_blocks_observation() {
        assert!(!Grant::none().permits(&Request::ListWindows));
    }
}

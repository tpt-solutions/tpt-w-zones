//! Access levels for IPC requests.
//!
//! Every request needs an access level, and a client holds a [`Grant`] of levels. The
//! Windows pipe server checks requests against its grant before forwarding them, and the
//! Archon bridge will map these levels onto capability rights.

use crate::ipc::Request;

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

/// Checks `request` against `grant`, returning a message suitable for a client error.
///
/// # Errors
/// Returns a human-readable reason when the grant does not cover the request.
pub fn authorise(grant: Grant, request: &Request) -> Result<(), String> {
    if grant.permits(request) {
        Ok(())
    } else {
        let needed = match access_for(request) {
            Some(Access::Manage) => "manage",
            Some(Access::Observe) => "observe",
            None => "none",
        };
        Err(format!(
            "access denied: this request needs '{needed}' access"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::WindowId;
    use crate::ipc::PROTOCOL_VERSION;
    use crate::model::ZoneId;

    fn snap() -> Request {
        Request::SnapWindow {
            window: WindowId(1),
            layout: "halves".into(),
            zone: ZoneId("left".into()),
        }
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

    #[test]
    fn authorise_explains_the_missing_level() {
        let err = authorise(Grant::observer(), &snap()).unwrap_err();
        assert!(err.contains("'manage'"), "{err}");
        assert!(authorise(Grant::full(), &snap()).is_ok());
    }
}

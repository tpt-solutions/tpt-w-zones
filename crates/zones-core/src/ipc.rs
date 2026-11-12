//! IPC protocol between the core service and its clients (editor, CLI, Archon bridge).
//!
//! **Transport:** a local byte stream — a named pipe on Windows
//! (`\\.\pipe\tpt-zones`), a Unix domain socket on Linux
//! (`$XDG_RUNTIME_DIR/tpt-zones.sock`). Archon will use its own IPC, carrying these
//! same messages.
//!
//! **Framing:** newline-delimited JSON (one message per line, UTF-8). Lines longer
//! than [`MAX_FRAME_BYTES`] are rejected.
//!
//! **Versioning:** the client sends [`Request::Hello`] with its protocol version;
//! the server replies with [`Response::Hello`]. Versions must match exactly for now.

use serde::{Deserialize, Serialize};

use crate::backend::{WindowEvent, WindowId, WindowInfo};
use crate::geometry::Monitor;
use crate::model::ZoneId;
use crate::{Error, Result};

/// Protocol version spoken by this build.
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum size of one encoded message, excluding the newline.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Requests a client may send.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Must be the first message on a connection.
    Hello {
        /// Client name, e.g. `"tpt-zones-editor"`.
        client: String,
        /// Client protocol version.
        protocol: u32,
    },
    /// List manageable windows.
    ListWindows,
    /// List monitors.
    ListMonitors,
    /// Snap a window into a zone of a named layout on the window's current monitor.
    SnapWindow {
        /// Window to move.
        window: WindowId,
        /// Layout id from the configuration.
        layout: String,
        /// Zone within that layout.
        zone: ZoneId,
    },
    /// Re-read the configuration file from disk.
    ReloadConfig,
    /// Start streaming [`WindowEvent`]s on this connection.
    Subscribe,
}

/// Responses and pushed messages from the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// Reply to [`Request::Hello`].
    Hello {
        /// Server name.
        server: String,
        /// Server protocol version.
        protocol: u32,
    },
    /// Generic success.
    Ok,
    /// Request failed.
    Error {
        /// Human-readable reason.
        message: String,
    },
    /// Reply to [`Request::ListWindows`].
    Windows {
        /// Current windows.
        windows: Vec<WindowInfo>,
    },
    /// Reply to [`Request::ListMonitors`].
    Monitors {
        /// Current monitors.
        monitors: Vec<Monitor>,
    },
    /// Pushed to subscribers after [`Request::Subscribe`].
    Event {
        /// The event.
        event: WindowEvent,
    },
}

/// Encodes a message as one line (without the trailing newline).
///
/// # Errors
/// Returns [`Error::Ipc`] if serialisation fails or the result exceeds [`MAX_FRAME_BYTES`].
pub fn encode<T: Serialize>(msg: &T) -> Result<String> {
    let line = serde_json::to_string(msg).map_err(|e| Error::Ipc(e.to_string()))?;
    if line.len() > MAX_FRAME_BYTES {
        return Err(Error::Ipc(format!(
            "frame of {} bytes exceeds limit",
            line.len()
        )));
    }
    Ok(line)
}

/// Decodes one line (with or without its trailing newline).
///
/// # Errors
/// Returns [`Error::Ipc`] if the line is too long or is not a valid message of type `T`.
pub fn decode<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.len() > MAX_FRAME_BYTES {
        return Err(Error::Ipc(format!(
            "frame of {} bytes exceeds limit",
            line.len()
        )));
    }
    serde_json::from_str(line).map_err(|e| Error::Ipc(e.to_string()))
}

/// Checks that a peer's protocol version matches ours.
///
/// # Errors
/// Returns [`Error::Ipc`] describing both versions when they differ.
pub fn check_version(peer: u32) -> Result<()> {
    if peer == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(Error::Ipc(format!(
            "protocol mismatch: peer speaks v{peer}, this build speaks v{PROTOCOL_VERSION}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trip() {
        let req = Request::SnapWindow {
            window: WindowId(42),
            layout: "halves".into(),
            zone: ZoneId("left".into()),
        };
        let line = encode(&req).unwrap();
        assert!(line.contains(r#""type":"snap_window""#));
        assert_eq!(decode::<Request>(&line).unwrap(), req);
    }

    #[test]
    fn decode_accepts_trailing_newline() {
        let line = format!("{}\n", encode(&Request::ListWindows).unwrap());
        assert_eq!(decode::<Request>(&line).unwrap(), Request::ListWindows);
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode::<Request>("{not json").is_err());
        assert!(decode::<Request>(r#"{"type":"nope"}"#).is_err());
    }

    #[test]
    fn version_check() {
        assert!(check_version(PROTOCOL_VERSION).is_ok());
        assert!(check_version(PROTOCOL_VERSION + 1).is_err());
    }
}

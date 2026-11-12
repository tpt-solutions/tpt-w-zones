//! `zones-core`: platform-independent types, traits and configuration shared by
//! every tpt-zones backend (Windows, Linux/Wayland, Archon) and the editor.
//!
//! Nothing in this crate touches an OS API. Platform crates implement the
//! traits in [`backend`]; the editor and CLI consume the types here.

#![doc(html_root_url = "https://docs.rs/zones-core/0.1.0")]

pub mod actions;
pub mod backend;
pub mod config;
pub mod engine;
pub mod geometry;
pub mod hotkey;
pub mod ipc;
pub mod logging;
pub mod model;
pub mod rules;

pub use backend::{
    HotkeyBackend, HotkeyHandler, Subscription, WindowBackend, WindowEvent, WindowEventSink,
};
pub use config::Config;
pub use geometry::{Monitor, MonitorId, Rect};
pub use hotkey::{Hotkey, Key, Modifiers};
pub use model::{Layout, Profile, Zone, ZoneId};

/// Errors produced by `zones-core` and by backends that report through its traits.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A configuration file could not be parsed.
    #[error("invalid config: {0}")]
    Config(String),
    /// A hotkey string could not be parsed.
    #[error("invalid hotkey '{0}'")]
    Hotkey(String),
    /// A backend operation failed.
    #[error("backend error: {0}")]
    Backend(String),
    /// A referenced layout, zone or profile does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// An IPC message could not be encoded or decoded.
    #[error("ipc error: {0}")]
    Ipc(String),
}

/// Convenience alias used throughout the workspace.
pub type Result<T, E = Error> = std::result::Result<T, E>;

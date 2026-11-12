//! Linux (Wayland) backend for tpt-zones.
//!
//! **Status:** partial. Desktop detection and the KWin record parser are implemented and
//! tested. The live connection to a compositor is not: until an adapter is wired in, every
//! operation that needs the desktop returns a clear error instead of pretending to work. See
//! `docs/linux-research.md` for the plan and `todo.md` (Phase 3) for what remains.
//!
//! Design: [`LinuxBackend`] implements the same `zones_core` traits as the Windows backend. It
//! talks to the compositor through a [`Transport`], so the adapter logic is tested with a fake
//! transport on any platform. A real transport (KWin over D-Bus first) only has to send lines
//! and return replies.

pub use zones_core as core;

pub mod desktop;
pub mod kwin;

use std::collections::HashMap;
use std::sync::Mutex;

use zones_core::backend::{
    HotkeyBackend, HotkeyHandler, Subscription, WindowBackend, WindowEventSink, WindowId,
    WindowInfo,
};
use zones_core::geometry::{Monitor, Rect};
use zones_core::hotkey::Hotkey;
use zones_core::{Error, Result};

use desktop::Desktop;

/// Platform identifier used in logs and IPC handshakes.
pub const PLATFORM: &str = "linux";

/// Returns `true` when this build targets Linux.
#[must_use]
pub const fn is_supported_target() -> bool {
    cfg!(target_os = "linux")
}

/// A line-based channel to the compositor.
///
/// `request` sends one command line and returns the reply. Query commands return JSON;
/// action commands return an empty string on success.
pub trait Transport: Send + Sync {
    /// Sends `line` and returns the compositor's reply.
    ///
    /// # Errors
    /// Returns an error if the compositor cannot be reached or refuses the request.
    fn request(&self, line: &str) -> Result<String>;
}

/// The transport used until a real compositor connection exists. It explains why nothing works.
#[derive(Debug, Clone)]
pub struct Unconnected {
    reason: String,
}

impl Transport for Unconnected {
    fn request(&self, _line: &str) -> Result<String> {
        Err(Error::Backend(self.reason.clone()))
    }
}

/// The Linux backend: a desktop family plus a transport to its compositor.
pub struct LinuxBackend {
    desktop: Desktop,
    transport: Box<dyn Transport>,
    /// KWin UUIDs by window id, filled in by `windows()` and used by `place()`.
    uuids: Mutex<HashMap<u64, String>>,
}

impl LinuxBackend {
    /// Detects the desktop from the environment. The transport is not connected yet, so
    /// operations return an error that names the desktop.
    #[must_use]
    pub fn detect() -> Self {
        let desktop = desktop::detect(|key| std::env::var(key).ok());
        let reason = match &desktop {
            Desktop::Kde => "the KWin adapter is not connected yet",
            Desktop::Gnome => "the GNOME adapter is not implemented yet",
            Desktop::Wlroots => "the wlroots adapter is not implemented yet",
            Desktop::Other(_) | Desktop::Unknown => "no supported Wayland desktop was detected",
        };
        Self::with_transport(
            desktop,
            Box::new(Unconnected {
                reason: reason.to_owned(),
            }),
        )
    }

    /// Builds a backend with an explicit desktop and transport. Used by tests and by the
    /// adapters once they exist.
    #[must_use]
    pub fn with_transport(desktop: Desktop, transport: Box<dyn Transport>) -> Self {
        Self {
            desktop,
            transport,
            uuids: Mutex::new(HashMap::new()),
        }
    }

    /// The detected desktop family.
    #[must_use]
    pub fn desktop(&self) -> &Desktop {
        &self.desktop
    }

    fn remember(&self, id: WindowId, uuid: &str) {
        let mut map = self
            .uuids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.insert(id.0, uuid.to_owned());
    }

    fn uuid_for(&self, id: WindowId) -> Option<String> {
        let map = self
            .uuids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.get(&id.0).cloned()
    }
}

impl WindowBackend for LinuxBackend {
    fn monitors(&self) -> Result<Vec<Monitor>> {
        let json = self.transport.request("outputs")?;
        kwin::parse_outputs(&json)
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        let monitors = self.monitors()?;
        let json = self.transport.request("windows")?;
        let windows = kwin::parse_windows(&json, &monitors)?;
        Ok(windows
            .into_iter()
            .map(|w| {
                self.remember(w.info.id, &w.uuid);
                w.info
            })
            .collect())
    }

    fn place(&self, id: WindowId, rect: Rect) -> Result<()> {
        let uuid = self
            .uuid_for(id)
            .ok_or_else(|| Error::NotFound(format!("window {} (list windows first)", id.0)))?;
        self.transport
            .request(&kwin::place_command(&uuid, rect))
            .map(|_| ())
    }

    fn subscribe(&self, _sink: WindowEventSink) -> Result<Subscription> {
        Err(Error::Backend(
            "window events are not available on Linux yet".into(),
        ))
    }
}

impl HotkeyBackend for LinuxBackend {
    fn register(&self, _hotkey: &Hotkey, _handler: HotkeyHandler) -> Result<Subscription> {
        Err(Error::Backend(
            "global hotkeys on Linux need the GlobalShortcuts portal, which is not implemented yet"
                .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use zones_core::geometry::MonitorId;

    const OUTPUTS: &str = r#"[{"name":"DP-1","x":0,"y":0,"width":1920,"height":1080,
        "scale":1.0,"primary":true,"work":{"x":0,"y":0,"width":1920,"height":1040}}]"#;
    const WINDOWS: &str = r#"[{"uuid":"{abc}","title":"Notes","resourceClass":"kate",
        "x":0,"y":0,"width":800,"height":600,"minimized":false,"output":"DP-1"}]"#;

    /// Replies from fixed fixtures and records every command sent.
    struct Fake {
        sent: Arc<Mutex<Vec<String>>>,
    }

    impl Transport for Fake {
        fn request(&self, line: &str) -> Result<String> {
            self.sent.lock().unwrap().push(line.to_owned());
            Ok(match line {
                "outputs" => OUTPUTS.to_owned(),
                "windows" => WINDOWS.to_owned(),
                _ => String::new(),
            })
        }
    }

    fn backend() -> (LinuxBackend, Arc<Mutex<Vec<String>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let b = LinuxBackend::with_transport(
            Desktop::Kde,
            Box::new(Fake {
                sent: Arc::clone(&sent),
            }),
        );
        (b, sent)
    }

    #[test]
    fn lists_monitors_and_windows_through_the_transport() {
        let (b, _) = backend();
        let monitors = b.monitors().unwrap();
        assert_eq!(monitors[0].id, MonitorId(0));
        let windows = b.windows().unwrap();
        assert_eq!(windows[0].process, "kate");
        assert_eq!(windows[0].monitor, Some(MonitorId(0)));
    }

    #[test]
    fn placing_sends_the_kwin_uuid_for_a_listed_window() {
        let (b, sent) = backend();
        let id = b.windows().unwrap()[0].id;
        b.place(id, Rect::new(5, 6, 700, 500)).unwrap();
        let last = sent.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last, "place {abc} 5 6 700 500");
    }

    #[test]
    fn placing_an_unlisted_window_is_not_found() {
        let (b, _) = backend();
        let err = b.place(WindowId(42), Rect::new(0, 0, 1, 1)).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[test]
    fn unconnected_backend_explains_itself() {
        let b = LinuxBackend::with_transport(
            Desktop::Gnome,
            Box::new(Unconnected {
                reason: "the GNOME adapter is not implemented yet".into(),
            }),
        );
        let err = b.monitors().unwrap_err();
        assert!(err.to_string().contains("GNOME"), "{err}");
    }

    #[test]
    fn events_and_hotkeys_report_not_available() {
        let (b, _) = backend();
        assert!(b.subscribe(Box::new(|_| {})).is_err());
        let hk: Hotkey = "Ctrl+Alt+Left".parse().unwrap();
        assert!(b.register(&hk, Box::new(|| {})).is_err());
    }

    #[test]
    fn platform_matches_target() {
        assert_eq!(super::is_supported_target(), cfg!(target_os = "linux"));
    }
}

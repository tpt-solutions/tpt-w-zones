//! Trait boundary between the platform-independent core and OS backends.
//!
//! Each platform crate (`zones-win`, `zones-linux`, `zones-archon-bridge`)
//! implements these traits. The core never calls OS APIs directly.
//!
//! Threading: backends are `Send + Sync`. Event sinks and hotkey handlers may be
//! invoked from a backend-owned thread and must not block for long.

use serde::{Deserialize, Serialize};

use crate::geometry::{Monitor, MonitorId, Rect};
use crate::hotkey::Hotkey;
use crate::Result;

/// Opaque, backend-assigned identifier for a top-level window.
///
/// On Windows this wraps an `HWND`; on Wayland it is the foreign-toplevel handle
/// mapped to an integer. Treat values as opaque; they are only meaningful to the
/// backend that produced them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WindowId(pub u64);

/// Snapshot of a top-level window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Backend-assigned identifier.
    pub id: WindowId,
    /// Window title.
    pub title: String,
    /// Owning executable or application id (e.g. `code.exe`, `org.mozilla.firefox`).
    pub process: String,
    /// Window class (Win32) or app id / WM class (Linux), if available.
    pub class: Option<String>,
    /// Outer bounds in physical pixels.
    pub rect: Rect,
    /// Monitor the window is primarily on, if known.
    pub monitor: Option<MonitorId>,
    /// Whether the window is minimised.
    pub minimized: bool,
}

/// Events a [`WindowBackend`] reports to subscribers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WindowEvent {
    /// A new top-level window appeared.
    Created(WindowId),
    /// A top-level window was closed or destroyed.
    Destroyed(WindowId),
    /// A window gained keyboard focus.
    Focused(WindowId),
    /// A window was moved or resized by anyone (user, app, or this tool).
    Moved {
        /// Window that moved.
        id: WindowId,
        /// New outer bounds.
        rect: Rect,
    },
    /// A window was minimised or restored.
    MinimizedChanged {
        /// Affected window.
        id: WindowId,
        /// `true` when now minimised.
        minimized: bool,
    },
    /// Display configuration changed (monitor added, removed, moved or re-scaled).
    MonitorsChanged,
}

/// Callback receiving [`WindowEvent`]s.
pub type WindowEventSink = Box<dyn Fn(WindowEvent) + Send + Sync>;

/// Callback invoked when a registered hotkey fires.
pub type HotkeyHandler = Box<dyn Fn() + Send + Sync>;

/// RAII handle for an event or hotkey subscription. Dropping it cancels the subscription.
#[must_use = "dropping a Subscription immediately cancels it"]
pub struct Subscription {
    cancel: Option<Box<dyn FnOnce() + Send>>,
}

impl Subscription {
    /// Creates a subscription that runs `cancel` exactly once when dropped or cancelled.
    pub fn new(cancel: impl FnOnce() + Send + 'static) -> Self {
        Self {
            cancel: Some(Box::new(cancel)),
        }
    }

    /// Cancels the subscription now instead of waiting for drop.
    pub fn cancel(mut self) {
        if let Some(f) = self.cancel.take() {
            f();
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(f) = self.cancel.take() {
            f();
        }
    }
}

impl std::fmt::Debug for Subscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscription")
            .field("active", &self.cancel.is_some())
            .finish()
    }
}

/// Enumerates windows and monitors and moves windows into place.
pub trait WindowBackend: Send + Sync {
    /// Returns all currently attached monitors.
    ///
    /// # Errors
    /// Returns an error if the OS display query fails.
    fn monitors(&self) -> Result<Vec<Monitor>>;

    /// Returns all manageable top-level windows (excludes shell/tool windows).
    ///
    /// # Errors
    /// Returns an error if the OS window query fails.
    fn windows(&self) -> Result<Vec<WindowInfo>>;

    /// Moves and resizes `id` to `rect` in physical pixels.
    ///
    /// # Errors
    /// Returns an error if the window no longer exists or the OS rejects the request.
    fn place(&self, id: WindowId, rect: Rect) -> Result<()>;

    /// Subscribes to [`WindowEvent`]s until the returned [`Subscription`] is dropped.
    ///
    /// # Errors
    /// Returns an error if the event source cannot be created.
    fn subscribe(&self, sink: WindowEventSink) -> Result<Subscription>;
}

/// Registers global hotkeys with the OS or compositor.
pub trait HotkeyBackend: Send + Sync {
    /// Registers `hotkey`; `handler` runs each time it is pressed.
    ///
    /// # Errors
    /// Returns an error if the key is already taken or the platform forbids it
    /// (Wayland compositors may refuse global grabs).
    fn register(&self, hotkey: &Hotkey, handler: HotkeyHandler) -> Result<Subscription>;
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    #[test]
    fn subscription_cancels_on_drop() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&count);
        drop(Subscription::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn explicit_cancel_runs_once() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&count);
        Subscription::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .cancel();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}

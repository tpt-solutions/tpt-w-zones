//! `WinBackend`: the Windows implementation of the `zones_core` backend traits.

#![allow(unsafe_code)]

use std::sync::Mutex;

use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use zones_core::backend::{
    HotkeyBackend, HotkeyHandler, Subscription, WindowBackend, WindowEventSink, WindowId,
    WindowInfo,
};
use zones_core::geometry::{Monitor, Rect};
use zones_core::hotkey::Hotkey;
use zones_core::{Error, Result};

use crate::hotkey::HotkeyThread;
use crate::{display, events, window};

/// Windows backend. Create one per process.
pub struct WinBackend {
    hotkeys: Mutex<Option<HotkeyThread>>,
}

impl WinBackend {
    /// Opts the process into per-monitor DPI awareness (v2) so that coordinates are
    /// physical pixels on every monitor. Call before creating any windows.
    ///
    /// If the process was already made DPI-aware, for example by a manifest, the OS
    /// refuses the second request. That is harmless and is logged, not treated as an error.
    #[must_use]
    pub fn new() -> Self {
        // SAFETY: setting the DPI awareness context has no pointer arguments.
        if let Err(e) =
            unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
        {
            tracing::debug!(error = %e, "DPI awareness already set; keeping existing mode");
        }
        Self {
            hotkeys: Mutex::new(None),
        }
    }
}

impl Default for WinBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowBackend for WinBackend {
    fn monitors(&self) -> Result<Vec<Monitor>> {
        display::enumerate()
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        let monitors = display::enumerate()?;
        window::list(&monitors)
    }

    fn place(&self, id: WindowId, rect: Rect) -> Result<()> {
        window::place(id, rect)
    }

    fn subscribe(&self, sink: WindowEventSink) -> Result<Subscription> {
        events::subscribe(sink)
    }
}

impl HotkeyBackend for WinBackend {
    fn register(&self, hotkey: &Hotkey, handler: HotkeyHandler) -> Result<Subscription> {
        let mut slot = self
            .hotkeys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(HotkeyThread::start()?);
        }
        slot.as_ref()
            .ok_or_else(|| Error::Backend("hotkey thread unavailable".into()))?
            .register(hotkey, handler)
    }
}

//! Global hotkeys via `RegisterHotKey`.
//!
//! `RegisterHotKey` ties a registration to the calling thread's message queue, so
//! every registration and every `WM_HOTKEY` dispatch happens on one dedicated thread.
//! Other threads send commands through a channel and wake the thread with a
//! posted message.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread::JoinHandle;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage, MSG, WM_APP, WM_HOTKEY,
};
use zones_core::backend::{HotkeyHandler, Subscription};
use zones_core::hotkey::{Hotkey, Modifiers};
use zones_core::{Error, Result};

/// Posted to the hotkey thread to tell it that commands are waiting.
const WM_COMMAND_READY: u32 = WM_APP + 1;

enum Command {
    Register {
        id: i32,
        hotkey: Hotkey,
        handler: HotkeyHandler,
        reply: mpsc::Sender<Result<()>>,
    },
    Unregister(i32),
    Shutdown,
}

/// Owns the hotkey thread. Dropping it stops the thread and releases every hotkey.
pub struct HotkeyThread {
    commands: mpsc::Sender<Command>,
    thread_id: u32,
    next_id: std::sync::atomic::AtomicI32,
    join: Option<JoinHandle<()>>,
}

impl HotkeyThread {
    /// Starts the hotkey thread.
    ///
    /// # Errors
    /// Returns [`Error::Backend`] if the thread cannot be started.
    pub fn start() -> Result<Self> {
        let (commands, rx) = mpsc::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel::<u32>();
        let join = std::thread::Builder::new()
            .name("zones-win-hotkeys".into())
            .spawn(move || hotkey_loop(&rx, &ready_tx))
            .map_err(|e| Error::Backend(e.to_string()))?;
        let thread_id = ready_rx
            .recv()
            .map_err(|_| Error::Backend("hotkey thread exited before starting".into()))?;
        Ok(Self {
            commands,
            thread_id,
            next_id: std::sync::atomic::AtomicI32::new(1),
            join: Some(join),
        })
    }

    /// Registers `hotkey` and returns a subscription that unregisters it on drop.
    ///
    /// # Errors
    /// Returns [`Error::Backend`] if the combination is already taken by another
    /// application or the thread has stopped.
    pub fn register(&self, hotkey: &Hotkey, handler: HotkeyHandler) -> Result<Subscription> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (reply, reply_rx) = mpsc::channel();
        self.send(Command::Register {
            id,
            hotkey: hotkey.clone(),
            handler,
            reply,
        })?;
        reply_rx
            .recv()
            .map_err(|_| Error::Backend("hotkey thread stopped".into()))??;

        let commands = self.commands.clone();
        let thread_id = self.thread_id;
        Ok(Subscription::new(move || {
            if commands.send(Command::Unregister(id)).is_ok() {
                wake(thread_id);
            }
        }))
    }

    fn send(&self, command: Command) -> Result<()> {
        self.commands
            .send(command)
            .map_err(|_| Error::Backend("hotkey thread stopped".into()))?;
        wake(self.thread_id);
        Ok(())
    }
}

impl Drop for HotkeyThread {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        wake(self.thread_id);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn wake(thread_id: u32) {
    // SAFETY: posting to a thread id has no memory-safety preconditions. If the
    // thread has already exited, the call fails and the failure is harmless.
    let _ = unsafe { PostThreadMessageW(thread_id, WM_COMMAND_READY, WPARAM(0), LPARAM(0)) };
}

fn hotkey_loop(commands: &mpsc::Receiver<Command>, ready: &mpsc::Sender<u32>) {
    // SAFETY: GetCurrentThreadId has no preconditions.
    let thread_id = unsafe { GetCurrentThreadId() };
    let _ = ready.send(thread_id);

    let mut handlers: HashMap<i32, HotkeyHandler> = HashMap::new();
    let mut msg = MSG::default();
    // SAFETY: the standard message pump on the thread that owns the registrations.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        match msg.message {
            WM_COMMAND_READY => {
                while let Ok(command) = commands.try_recv() {
                    match command {
                        Command::Register {
                            id,
                            hotkey,
                            handler,
                            reply,
                        } => {
                            let result = register_native(id, &hotkey);
                            if result.is_ok() {
                                handlers.insert(id, handler);
                            }
                            let _ = reply.send(result);
                        }
                        Command::Unregister(id) => {
                            handlers.remove(&id);
                            // SAFETY: unregistering an id this thread registered.
                            let _ = unsafe { UnregisterHotKey(None, id) };
                        }
                        Command::Shutdown => {
                            release_all(&mut handlers);
                            return;
                        }
                    }
                }
            }
            WM_HOTKEY => {
                let id = i32::try_from(msg.wParam.0).unwrap_or(-1);
                if let Some(handler) = handlers.get(&id) {
                    // A panicking handler must not take down the message loop.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(handler));
                }
            }
            _ => {
                // SAFETY: standard dispatch of a message owned by this thread.
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
    release_all(&mut handlers);
}

fn release_all(handlers: &mut HashMap<i32, HotkeyHandler>) {
    for id in handlers.drain().map(|(id, _)| id) {
        // SAFETY: unregistering ids this thread registered.
        let _ = unsafe { UnregisterHotKey(None, id) };
    }
}

fn register_native(id: i32, hotkey: &Hotkey) -> Result<()> {
    let vk = virtual_key(hotkey.key.name())
        .ok_or_else(|| Error::Hotkey(format!("no virtual key for '{}'", hotkey.key.name())))?;
    let mods = native_modifiers(hotkey.modifiers) | MOD_NOREPEAT;
    // SAFETY: a null window associates the hotkey with this thread's queue, as required.
    unsafe { RegisterHotKey(None, id, mods, u32::from(vk)) }.map_err(|e| {
        Error::Backend(format!(
            "RegisterHotKey('{hotkey}') failed (already in use?): {e}"
        ))
    })
}

/// Translates portable modifiers to the Win32 `MOD_*` bit set.
#[must_use]
pub fn native_modifiers(m: Modifiers) -> HOT_KEY_MODIFIERS {
    let mut bits = HOT_KEY_MODIFIERS(0);
    if m.alt {
        bits |= MOD_ALT;
    }
    if m.ctrl {
        bits |= MOD_CONTROL;
    }
    if m.shift {
        bits |= MOD_SHIFT;
    }
    if m.super_key {
        bits |= MOD_WIN;
    }
    bits
}

/// Maps a normalised key name (see `zones_core::hotkey::Key`) to a Win32 virtual-key code.
#[must_use]
pub fn virtual_key(name: &str) -> Option<u8> {
    let named = match name {
        "Left" => 0x25,
        "Up" => 0x26,
        "Right" => 0x27,
        "Down" => 0x28,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "Space" => 0x20,
        "Enter" => 0x0D,
        "Tab" => 0x09,
        "Escape" => 0x1B,
        "Backspace" => 0x08,
        "Delete" => 0x2E,
        "Insert" => 0x2D,
        _ => 0,
    };
    if named != 0 {
        return Some(named);
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=24).contains(&n).then_some(0x70 + n - 1);
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        // Letters and digits share their ASCII upper-case code points with VK_* codes.
        (Some(c), None) if c.is_ascii_uppercase() || c.is_ascii_digit() => Some(c as u8),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_digits_and_named_keys() {
        assert_eq!(virtual_key("A"), Some(0x41));
        assert_eq!(virtual_key("1"), Some(0x31));
        assert_eq!(virtual_key("Left"), Some(0x25));
        assert_eq!(virtual_key("F1"), Some(0x70));
        assert_eq!(virtual_key("F24"), Some(0x87));
    }

    #[test]
    fn rejects_unknown_keys() {
        assert_eq!(virtual_key("F25"), None);
        assert_eq!(virtual_key("Weird"), None);
        assert_eq!(virtual_key("a"), None); // keys are normalised before this point
    }

    #[test]
    fn maps_modifiers() {
        let m = Modifiers {
            ctrl: true,
            alt: true,
            shift: false,
            super_key: true,
        };
        let bits = native_modifiers(m);
        assert_eq!(bits.0, MOD_CONTROL.0 | MOD_ALT.0 | MOD_WIN.0);
    }

    /// Requires a desktop session. Uses an unusual combination to avoid clashes.
    #[test]
    #[ignore = "needs an interactive Windows desktop session"]
    fn register_and_unregister() {
        let thread = HotkeyThread::start().expect("start");
        let hk: Hotkey = "Ctrl+Alt+Shift+Super+F24".parse().expect("parse");
        let sub = thread.register(&hk, Box::new(|| {})).expect("register");
        sub.cancel();
        thread
            .register(&hk, Box::new(|| {}))
            .expect("re-register after cancel")
            .cancel();
    }
}

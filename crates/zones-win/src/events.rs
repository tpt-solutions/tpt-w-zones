//! Window event subscription via `SetWinEventHook`.
//!
//! Out-of-context hooks are delivered to the thread that installed them, so the
//! hooks run on a dedicated thread with its own message loop. Only one subscription
//! may be active per process, because the hook procedure is a plain function
//! pointer that reaches its sink through a process-wide slot.

#![allow(unsafe_code)]

use std::sync::mpsc;
use std::sync::Mutex;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetAncestor, GetMessageW,
    PostThreadMessageW, RegisterClassW, TranslateMessage, EVENT_OBJECT_CREATE,
    EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, GA_ROOT, MSG, OBJID_WINDOW,
    WINDOW_EX_STYLE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_DISPLAYCHANGE, WM_QUIT,
    WNDCLASSW, WS_POPUP,
};
use zones_core::backend::{Subscription, WindowEvent, WindowEventSink};
use zones_core::{Error, Result};

use crate::window::{id_of, visible_rect};

/// The sink used by the hook procedure. `None` when no subscription is active.
static SINK: Mutex<Option<WindowEventSink>> = Mutex::new(None);

/// Starts delivering window events to `sink` until the returned [`Subscription`] is dropped.
///
/// # Errors
/// Returns [`Error::Backend`] if another subscription is active or the hooks cannot be installed.
pub fn subscribe(sink: WindowEventSink) -> Result<Subscription> {
    {
        let mut slot = lock_sink();
        if slot.is_some() {
            return Err(Error::Backend(
                "window event subscription already active".into(),
            ));
        }
        *slot = Some(sink);
    }

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32>>();
    let thread = std::thread::Builder::new()
        .name("zones-win-events".into())
        .spawn(move || event_thread(&ready_tx))
        .map_err(|e| Error::Backend(e.to_string()))?;

    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Ok(Subscription::new(move || {
            // SAFETY: posting WM_QUIT to a thread id is always safe; if the thread has
            // already exited the call fails and is ignored.
            let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            let _ = thread.join();
            *lock_sink() = None;
        })),
        Ok(Err(e)) => {
            *lock_sink() = None;
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            *lock_sink() = None;
            Err(Error::Backend("event thread exited before starting".into()))
        }
    }
}

fn lock_sink() -> std::sync::MutexGuard<'static, Option<WindowEventSink>> {
    // A poisoned lock only means a sink panicked; the slot itself is still usable.
    SINK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn event_thread(ready: &mpsc::Sender<Result<u32>>) {
    // SAFETY: installs process-wide-safe out-of-context hooks; the handles are kept
    // on this stack frame and unhooked before the thread returns.
    let hooks = unsafe {
        [
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_MINIMIZEEND,
                None,
                Some(hook_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            ),
            SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_LOCATIONCHANGE,
                None,
                Some(hook_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            ),
        ]
    };
    if hooks.iter().any(|h| h.is_invalid()) {
        let _ = ready.send(Err(Error::Backend("SetWinEventHook failed".into())));
        unhook_all(&hooks);
        return;
    }

    // SAFETY: GetCurrentThreadId has no preconditions.
    let thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let _ = ready.send(Ok(thread_id));

    // A hidden top-level window receives broadcast display messages; message-only windows do not.
    let display = create_display_window();
    if display.is_none() {
        tracing::warn!("display watcher unavailable; MonitorsChanged will not be reported");
    }

    let mut msg = MSG::default();
    // SAFETY: standard message pump on the thread that owns the hooks.
    // GetMessageW returns 0 on WM_QUIT and -1 on error; both end the loop.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    if let Some(hwnd) = display {
        // SAFETY: the window was created on this thread and is destroyed exactly once.
        let _ = unsafe { DestroyWindow(hwnd) };
    }
    unhook_all(&hooks);
}

fn create_display_window() -> Option<HWND> {
    // SAFETY: GetModuleHandleW(None) returns the running executable.
    let instance = unsafe { GetModuleHandleW(None) }.ok()?;
    let class = WNDCLASSW {
        lpfnWndProc: Some(display_proc),
        hInstance: instance.into(),
        lpszClassName: w!("TptZonesDisplay"),
        ..Default::default()
    };
    // SAFETY: the class is fully initialised with static strings; a duplicate registration returns 0 and is harmless.
    unsafe { RegisterClassW(&class) };
    // SAFETY: all pointer arguments are static or null. The window is never shown.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("TptZonesDisplay"),
            w!("tpt-zones display"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
    .ok()
}

unsafe extern "system" fn display_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_DISPLAYCHANGE {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(sink) = lock_sink().as_ref() {
                sink(WindowEvent::MonitorsChanged);
            }
        }));
        return LRESULT(0);
    }
    // SAFETY: unhandled messages go to the default procedure with the same arguments.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn unhook_all(hooks: &[HWINEVENTHOOK]) {
    for hook in hooks.iter().filter(|h| !h.is_invalid()) {
        // SAFETY: each handle came from SetWinEventHook and is unhooked exactly once.
        let _ = unsafe { UnhookWinEvent(*hook) };
    }
}

/// Maps a native event to a `WindowEvent`, or `None` for events we do not report.
fn translate(event: u32, hwnd: HWND, id_object: i32) -> Option<WindowEvent> {
    // Only window-level notifications; ignore menus, carets, scrollbars and so on.
    if id_object != OBJID_WINDOW.0 || hwnd.is_invalid() {
        return None;
    }
    // SAFETY: GetAncestor accepts any handle and returns null for invalid ones.
    if unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
        return None; // child windows are not managed
    }
    let id = id_of(hwnd);
    match event {
        EVENT_OBJECT_CREATE => Some(WindowEvent::Created(id)),
        EVENT_OBJECT_DESTROY => Some(WindowEvent::Destroyed(id)),
        EVENT_SYSTEM_FOREGROUND => Some(WindowEvent::Focused(id)),
        EVENT_SYSTEM_MINIMIZESTART => Some(WindowEvent::MinimizedChanged {
            id,
            minimized: true,
        }),
        EVENT_SYSTEM_MINIMIZEEND => Some(WindowEvent::MinimizedChanged {
            id,
            minimized: false,
        }),
        EVENT_OBJECT_LOCATIONCHANGE => {
            visible_rect(hwnd).map(|rect| WindowEvent::Moved { id, rect })
        }
        _ => None,
    }
}

unsafe extern "system" fn hook_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    let Some(ev) = translate(event, hwnd, id_object) else {
        return;
    };
    // A panic must not unwind across the FFI boundary into the OS.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Some(sink) = lock_sink().as_ref() {
            sink(ev);
        }
    }));
}

#[cfg(test)]
mod tests {
    use std::sync::{mpsc, Mutex};
    use std::time::{Duration, Instant};

    use zones_core::backend::WindowEvent;
    use zones_core::geometry::Rect;

    use crate::{display, window};

    /// The subscription slot is process-wide, so tests that use it must not overlap.
    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn second_subscription_is_rejected() {
        let _guard = LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let first = super::subscribe(Box::new(|_| {})).expect("first subscription");
        assert!(super::subscribe(Box::new(|_| {})).is_err());
        drop(first);
        let again = super::subscribe(Box::new(|_| {})).expect("resubscribe after drop");
        drop(again);
    }

    /// Sends the display watcher the same broadcast Windows sends on a display change, and
    /// checks that `MonitorsChanged` is reported. Does not change the real display settings.
    #[test]
    #[ignore = "needs an interactive desktop session"]
    fn reports_display_change_messages() {
        use windows::core::w;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            FindWindowW, SendMessageW, WM_DISPLAYCHANGE,
        };

        let _guard = LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (tx, rx) = mpsc::channel::<WindowEvent>();
        let sub = super::subscribe(Box::new(move |ev| {
            let _ = tx.send(ev);
        }))
        .expect("subscribe");

        // SAFETY: the class name is a static string; the handle is only used for SendMessageW
        // while the subscription (and therefore the watcher window) is alive.
        let found = (0..50).find_map(|_| {
            std::thread::sleep(Duration::from_millis(50));
            let hwnd = unsafe { FindWindowW(w!("TptZonesDisplay"), None) }.ok()?;
            (!hwnd.0.is_null()).then_some(hwnd)
        });
        let hwnd = found.expect("display watcher window exists");
        unsafe { SendMessageW(hwnd, WM_DISPLAYCHANGE, Some(WPARAM(0)), Some(LPARAM(0))) };

        let got = rx.recv_timeout(Duration::from_secs(2)).ok();
        sub.cancel();
        assert_eq!(got, Some(WindowEvent::MonitorsChanged));
    }

    /// Launches Notepad, moves it, and checks that a `Moved` event arrives for it.
    #[test]
    #[ignore = "launches notepad.exe and needs an interactive desktop session"]
    fn reports_moves_of_a_real_window() {
        let _guard = LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (tx, rx) = mpsc::channel::<WindowEvent>();
        let sub = super::subscribe(Box::new(move |ev| {
            let _ = tx.send(ev);
        }))
        .expect("subscribe");

        let mut child = std::process::Command::new("notepad.exe")
            .spawn()
            .expect("start notepad");

        let target = (0..100)
            .find_map(|_| {
                std::thread::sleep(Duration::from_millis(100));
                let monitors = display::enumerate().ok()?;
                window::list(&monitors)
                    .ok()?
                    .into_iter()
                    .find(|w| w.process.eq_ignore_ascii_case("notepad.exe"))
            })
            .expect("notepad window appears");

        let moved_to = Rect::new(
            target.rect.x + 40,
            target.rect.y + 40,
            target.rect.width,
            target.rect.height,
        );
        window::place(target.id, moved_to).expect("place");

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut saw_move = false;
        while Instant::now() < deadline && !saw_move {
            if let Ok(WindowEvent::Moved { id, .. }) = rx.recv_timeout(Duration::from_millis(200)) {
                saw_move |= id == target.id;
            }
        }

        let _ = child.kill();
        let _ = child.wait();
        sub.cancel();
        assert!(saw_move, "expected a Moved event for the placed window");
    }
}

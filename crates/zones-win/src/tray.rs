//! System tray icon with a small context menu.
//!
//! The icon lives on a dedicated thread with its own hidden message window. Menu choices
//! are sent as [`TrayCommand`]s; the service decides what they mean. The tray never
//! changes window state itself.

#![allow(unsafe_code)]

use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW, PostThreadMessageW, RegisterClassW,
    SetForegroundWindow, TrackPopupMenu, TranslateMessage, HWND_MESSAGE, IDI_APPLICATION,
    MF_STRING, MSG, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_QUIT, WM_RBUTTONUP, WNDCLASSW,
};
use zones_core::{Error, Result};

/// Callback message used by the shell to tell us about mouse activity on the icon.
const WM_TRAY: u32 = WM_APP + 2;
/// Menu command ids.
const CMD_RELOAD: u32 = 1;
const CMD_APPLY: u32 = 2;
const CMD_QUIT: u32 = 3;
/// Identifies our icon among any others the shell tracks for this window.
const ICON_ID: u32 = 1;

/// What the user asked for from the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    /// Re-read the configuration.
    Reload,
    /// Apply placement rules now.
    ApplyRules,
    /// Stop the service.
    Quit,
}

/// The sink used by the window procedure. One tray per process.
static SINK: Mutex<Option<Sender<TrayCommand>>> = Mutex::new(None);

/// A running tray icon. Dropping it removes the icon and stops its thread.
pub struct Tray {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

/// Shows the tray icon and starts forwarding menu choices to `commands`.
///
/// # Errors
/// Returns [`Error::Backend`] if the icon or its window cannot be created, or if a tray
/// is already running in this process.
pub fn start(commands: Sender<TrayCommand>) -> Result<Tray> {
    {
        let mut slot = SINK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_some() {
            return Err(Error::Backend("tray already running".into()));
        }
        *slot = Some(commands);
    }

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32>>();
    let join = std::thread::Builder::new()
        .name("zones-win-tray".into())
        .spawn(move || tray_thread(&ready_tx))
        .map_err(|e| Error::Backend(e.to_string()))?;

    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Ok(Tray {
            thread_id,
            join: Some(join),
        }),
        Ok(Err(e)) => {
            clear_sink();
            let _ = join.join();
            Err(e)
        }
        Err(_) => {
            clear_sink();
            Err(Error::Backend("tray thread exited before starting".into()))
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // SAFETY: posting WM_QUIT to a thread id has no memory-safety preconditions.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        clear_sink();
    }
}

fn clear_sink() {
    *SINK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

fn tray_thread(ready: &mpsc::Sender<Result<u32>>) {
    let hwnd = match create_window() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    if let Err(e) = add_icon(hwnd) {
        // SAFETY: `hwnd` was created above on this thread and is destroyed once.
        let _ = unsafe { DestroyWindow(hwnd) };
        let _ = ready.send(Err(e));
        return;
    }
    // SAFETY: GetCurrentThreadId has no preconditions.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));

    let mut msg = MSG::default();
    // SAFETY: standard message pump for the thread that owns the window.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        // SAFETY: standard dispatch of a message owned by this thread.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    remove_icon(hwnd);
    // SAFETY: the window was created on this thread and is destroyed exactly once here.
    let _ = unsafe { DestroyWindow(hwnd) };
}

fn create_window() -> Result<HWND> {
    // SAFETY: GetModuleHandleW(None) returns the handle of the running executable.
    let instance = unsafe { GetModuleHandleW(None) }
        .map_err(|e| Error::Backend(format!("GetModuleHandleW: {e}")))?;
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        lpszClassName: w!("TptZonesTray"),
        ..Default::default()
    };
    // SAFETY: `class` is fully initialised and its strings are static. A duplicate
    // registration (from an earlier tray in this process) returns 0 and is harmless.
    unsafe { RegisterClassW(&class) };
    // SAFETY: all pointer arguments are static strings or null; HWND_MESSAGE makes a
    // message-only window that is never shown.
    unsafe {
        CreateWindowExW(
            Default::default(),
            w!("TptZonesTray"),
            w!("tpt-zones"),
            Default::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
    }
    .map_err(|e| Error::Backend(format!("CreateWindowExW: {e}")))
}

fn add_icon(hwnd: HWND) -> Result<()> {
    // SAFETY: IDI_APPLICATION is a system icon id; no ownership is taken.
    let icon = unsafe { LoadIconW(None, IDI_APPLICATION) }
        .map_err(|e| Error::Backend(format!("LoadIconW: {e}")))?;
    let mut data = NOTIFYICONDATAW {
        cbSize: u32::try_from(std::mem::size_of::<NOTIFYICONDATAW>()).unwrap_or(0),
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    let tip: Vec<u16> = "tpt-zones".encode_utf16().collect();
    data.szTip[..tip.len()].copy_from_slice(&tip);
    // SAFETY: `data` is a fully initialised NOTIFYICONDATAW with the size set.
    if unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
        Ok(())
    } else {
        Err(Error::Backend("Shell_NotifyIconW(NIM_ADD) failed".into()))
    }
}

fn remove_icon(hwnd: HWND) {
    let data = NOTIFYICONDATAW {
        cbSize: u32::try_from(std::mem::size_of::<NOTIFYICONDATAW>()).unwrap_or(0),
        hWnd: hwnd,
        uID: ICON_ID,
        ..Default::default()
    };
    // SAFETY: the structure identifies the icon we added; removing it is idempotent.
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_TRAY && (lparam.0 as u32 & 0xFFFF) == WM_RBUTTONUP {
        show_menu(hwnd);
        return LRESULT(0);
    }
    // SAFETY: forwarding unhandled messages to the default procedure with the same arguments.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn show_menu(hwnd: HWND) {
    // SAFETY: each call either creates a menu we destroy below, or reads the cursor position.
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(menu, MF_STRING, CMD_RELOAD as usize, w!("Reload config"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_APPLY as usize, w!("Apply rules now"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_QUIT as usize, w!("Quit tpt-zones"));

        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        // Required so the menu dismisses when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            point.x,
            point.y,
            Some(0),
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);

        let command = match chosen.0 as u32 {
            CMD_RELOAD => TrayCommand::Reload,
            CMD_APPLY => TrayCommand::ApplyRules,
            CMD_QUIT => TrayCommand::Quit,
            _ => return,
        };
        if let Ok(slot) = SINK.lock() {
            if let Some(sink) = slot.as_ref() {
                let _ = sink.send(command);
            }
        }
    }
}

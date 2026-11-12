//! Top-level window enumeration, filtering, and placement.
//!
//! Placement accounts for DWM's invisible resize borders: `GetWindowRect` includes
//! them, but users see the extended frame bounds. Without this correction, zones
//! would leave gaps or overlap by about 7 px on each side of Windows 10/11 windows.

#![allow(unsafe_code)]

use std::path::Path;

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetClassNameW, GetWindowLongPtrW, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SetWindowPos, ShowWindow, GA_ROOTOWNER, GWL_EXSTYLE, SWP_NOACTIVATE, SWP_NOZORDER, SW_RESTORE,
    WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};
use zones_core::backend::{WindowId, WindowInfo};
use zones_core::geometry::{Monitor, Rect};
use zones_core::{Error, Result};

/// Window classes that belong to the shell or desktop rather than to applications.
const SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Button",
    "NotifyIconOverflowWindow",
];

/// Converts a window handle to the opaque id used by the core.
#[must_use]
pub fn id_of(hwnd: HWND) -> WindowId {
    WindowId(hwnd.0 as usize as u64)
}

/// Converts an opaque id back to a window handle.
fn hwnd_of(id: WindowId) -> HWND {
    HWND(id.0 as usize as *mut core::ffi::c_void)
}

/// Lists manageable top-level windows, with monitors resolved from `monitors`.
///
/// # Errors
/// Returns [`Error::Backend`] if the window enumeration fails.
pub fn list(monitors: &[Monitor]) -> Result<Vec<WindowInfo>> {
    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `&mut Vec<HWND>` passed to EnumWindows below.
        let handles = unsafe { &mut *(data.0 as *mut Vec<HWND>) };
        handles.push(hwnd);
        TRUE
    }

    let mut handles: Vec<HWND> = Vec::new();
    // SAFETY: the callback only uses `data` as described; the vector outlives the call.
    unsafe {
        EnumWindows(
            Some(collect),
            LPARAM(std::ptr::addr_of_mut!(handles) as isize),
        )
        .map_err(|e| Error::Backend(format!("EnumWindows failed: {e}")))?;
    }

    Ok(handles
        .into_iter()
        .filter(|&h| is_manageable(h))
        .filter_map(|h| info_of(h, monitors))
        .collect())
}

/// Returns `true` for windows a tiling manager should touch.
fn is_manageable(hwnd: HWND) -> bool {
    // SAFETY: all calls take a window handle from EnumWindows and tolerate it
    // becoming invalid (they return zero/false in that case).
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 && ex_style & WS_EX_APPWINDOW.0 == 0 {
            return false;
        }
        // Owned windows (dialogs, popups) follow their owner and are not tiled.
        if GetAncestor(hwnd, GA_ROOTOWNER) != hwnd {
            return false;
        }
        if is_cloaked(hwnd) {
            return false;
        }
    }
    let class = class_name(hwnd).unwrap_or_default();
    if SHELL_CLASSES.iter().any(|c| c.eq_ignore_ascii_case(&class)) {
        return false;
    }
    !window_title(hwnd).is_empty()
}

fn info_of(hwnd: HWND, monitors: &[Monitor]) -> Option<WindowInfo> {
    let rect = visible_rect(hwnd)?;
    let monitor = Monitor::best_for(monitors, &rect).map(|m| m.id);
    // SAFETY: IsIconic accepts any handle value and returns FALSE for invalid ones.
    let minimized = unsafe { IsIconic(hwnd).as_bool() };
    Some(WindowInfo {
        id: id_of(hwnd),
        title: window_title(hwnd),
        process: process_name(hwnd).unwrap_or_default(),
        class: class_name(hwnd),
        rect,
        monitor,
        minimized,
    })
}

/// Moves and resizes `id` so that its visible frame occupies `target`.
///
/// # Errors
/// Returns [`Error::Backend`] if the window no longer exists or `SetWindowPos` fails.
pub fn place(id: WindowId, target: Rect) -> Result<()> {
    let hwnd = hwnd_of(id);
    // SAFETY: a stale handle makes these calls fail, which is reported as an error below.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            // ShowWindow's return value only reports the previous visibility state.
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
    }

    let (outer, frame) = raw_and_frame_rects(hwnd)
        .ok_or_else(|| Error::Backend(format!("window {} is gone", id.0)))?;
    // Invisible border sizes on each side (outer minus visible frame).
    let left = frame.left - outer.left;
    let top = frame.top - outer.top;
    let right = outer.right - frame.right;
    let bottom = outer.bottom - frame.bottom;

    let width = i32::try_from(target.width).map_err(|e| Error::Backend(e.to_string()))?;
    let height = i32::try_from(target.height).map_err(|e| Error::Backend(e.to_string()))?;
    // SAFETY: `hwnd` is validated by the lookup above; SetWindowPos reports failure as Err.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            target.x - left,
            target.y - top,
            width + left + right,
            height + top + bottom,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .map_err(|e| Error::Backend(format!("SetWindowPos failed: {e}")))
    }
}

/// Returns the visible (DWM extended-frame) bounds of a window, if it exists.
#[must_use]
pub fn visible_rect(hwnd: HWND) -> Option<Rect> {
    raw_and_frame_rects(hwnd).map(|(_, frame)| to_rect(frame))
}

fn raw_and_frame_rects(hwnd: HWND) -> Option<(RECT, RECT)> {
    let mut outer = RECT::default();
    // SAFETY: `outer` is a valid out-parameter; failure is reported via the Result.
    unsafe { GetWindowRect(hwnd, &mut outer).ok()? };
    let frame = extended_frame(hwnd).unwrap_or(outer);
    Some((outer, frame))
}

fn extended_frame(hwnd: HWND) -> Option<RECT> {
    let mut frame = RECT::default();
    // SAFETY: the buffer is a RECT and its size is passed exactly.
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            std::ptr::addr_of_mut!(frame).cast(),
            u32::try_from(std::mem::size_of::<RECT>()).ok()?,
        )
    };
    result.ok().map(|()| frame)
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    // SAFETY: the buffer is a u32 and its size is passed exactly.
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            std::ptr::addr_of_mut!(cloaked).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    result.is_ok() && cloaked != 0
}

fn window_title(hwnd: HWND) -> String {
    // SAFETY: the length query and the buffer read are bounded by the returned length.
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let written = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..written.max(0) as usize])
    }
}

fn class_name(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 256];
    // SAFETY: the buffer length is passed as the capacity.
    let written = unsafe { GetClassNameW(hwnd, &mut buf) };
    (written > 0).then(|| String::from_utf16_lossy(&buf[..written as usize]))
}

/// Returns the executable file name (e.g. `code.exe`) of the window's process.
fn process_name(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out-parameter.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    // SAFETY: the handle is closed on every path below, and the buffer size is passed in.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        ok.ok()?;
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        Path::new(&full)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    }
}

fn to_rect(r: RECT) -> Rect {
    Rect::new(
        r.left,
        r.top,
        u32::try_from(r.right - r.left).unwrap_or(0),
        u32::try_from(r.bottom - r.top).unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_id_round_trips() {
        let id = WindowId(0x1234_5678);
        assert_eq!(id_of(hwnd_of(id)), id);
    }

    #[test]
    fn listing_windows_does_not_fail() {
        let monitors = crate::display::enumerate().expect("monitors");
        let windows = list(&monitors).expect("window enumeration");
        for w in &windows {
            assert!(!w.title.is_empty());
        }
    }
}

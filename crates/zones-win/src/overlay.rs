//! Preview overlays: translucent, click-through windows drawn over each monitor.
//!
//! Each monitor gets one layered window. Its background is a colour key, so only the zone
//! rectangles are visible. The windows never take focus or mouse input, and they close after
//! the requested time on their own thread.

#![allow(unsafe_code)]

use std::sync::mpsc;
use std::time::Duration;

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, FrameRect, SetBkMode,
    SetTextColor, TextOutW, HGDIOBJ, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, KillTimer, PostQuitMessage, RegisterClassW, SetLayeredWindowAttributes,
    SetTimer, SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW, GWLP_USERDATA,
    LAYERED_WINDOW_ATTRIBUTES_FLAGS, LWA_ALPHA, LWA_COLORKEY, MSG, SW_SHOWNOACTIVATE, WM_CREATE,
    WM_NCDESTROY, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use zones_core::engine::PreviewMonitor;
use zones_core::geometry::Rect;
use zones_core::{Error, Result};

/// Colour used as the transparent key. Magenta is never a zone colour.
const KEY_COLOR: u32 = 0x00FF_00FF;
/// Zone fill, `#3b6fd4` in COLORREF (BGR) order.
const ZONE_COLOR: u32 = 0x00D4_6F3B;
/// Zone outline, white.
const BORDER_COLOR: u32 = 0x00FF_FFFF;
/// Overall opacity of the overlay, 0 to 255.
const ALPHA: u8 = 200;

/// Name used by the timer that closes the overlays.
const CLOSE_TIMER: usize = 1;

/// Per-window data, owned by the overlay thread's stack and reached through `GWLP_USERDATA`.
struct Canvas {
    origin: (i32, i32),
    zones: Vec<(String, Rect)>,
}

/// Shows `previews` for `duration`. Returns immediately; the windows close on their own.
///
/// # Errors
/// Returns [`Error::Backend`] if the overlay thread cannot start or create its windows.
pub fn show(previews: Vec<PreviewMonitor>, duration: Duration) -> Result<()> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
    std::thread::Builder::new()
        .name("zones-win-overlay".into())
        .spawn(move || overlay_thread(&previews, duration, &ready_tx))
        .map_err(|e| Error::Backend(e.to_string()))?;
    ready_rx
        .recv()
        .map_err(|_| Error::Backend("overlay thread exited early".into()))?
}

fn overlay_thread(
    previews: &[PreviewMonitor],
    duration: Duration,
    ready: &mpsc::Sender<Result<()>>,
) {
    let canvases: Vec<Canvas> = previews
        .iter()
        .map(|p| Canvas {
            origin: (p.work_area.x, p.work_area.y),
            zones: p.zones.iter().map(|z| (z.name.clone(), z.rect)).collect(),
        })
        .collect();

    let mut windows: Vec<HWND> = Vec::new();
    for (preview, canvas) in previews.iter().zip(canvases.iter()) {
        match create_overlay(preview.work_area, canvas) {
            Ok(hwnd) => windows.push(hwnd),
            Err(e) => {
                for hwnd in windows.drain(..) {
                    // SAFETY: the window was created on this thread and is destroyed once.
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
                let _ = ready.send(Err(e));
                return;
            }
        }
    }
    let _ = ready.send(Ok(()));

    let millis = u32::try_from(duration.as_millis()).unwrap_or(u32::MAX);
    // SAFETY: a thread timer with no window; WM_TIMER arrives on this thread's queue.
    unsafe { SetTimer(None, CLOSE_TIMER, millis, None) };

    let mut msg = MSG::default();
    // SAFETY: standard message pump on the thread that owns the overlay windows.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        // SAFETY: standard dispatch of a message owned by this thread.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    // SAFETY: the timer and windows belong to this thread and are released exactly once.
    unsafe {
        let _ = KillTimer(None, CLOSE_TIMER);
    }
    for hwnd in windows {
        // SAFETY: as above.
        let _ = unsafe { DestroyWindow(hwnd) };
    }
}

fn create_overlay(area: Rect, canvas: &Canvas) -> Result<HWND> {
    // SAFETY: GetModuleHandleW(None) returns the running executable.
    let instance = unsafe { GetModuleHandleW(None) }
        .map_err(|e| Error::Backend(format!("GetModuleHandleW: {e}")))?;
    let class = WNDCLASSW {
        lpfnWndProc: Some(overlay_proc),
        hInstance: instance.into(),
        lpszClassName: w!("TptZonesOverlay"),
        ..Default::default()
    };
    // SAFETY: fully initialised class with static strings; a duplicate registration is harmless.
    unsafe { RegisterClassW(&class) };

    let width = i32::try_from(area.width).unwrap_or(i32::MAX);
    let height = i32::try_from(area.height).unwrap_or(i32::MAX);
    // SAFETY: `canvas` outlives the window (it lives on the overlay thread's stack until the
    // loop ends, and the windows are destroyed before it is dropped). The pointer is handed
    // to WM_CREATE through lpCreateParams and stored in GWLP_USERDATA.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("TptZonesOverlay"),
            w!("tpt-zones preview"),
            WS_POPUP,
            area.x,
            area.y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            Some((canvas as *const Canvas).cast()),
        )
    }
    .map_err(|e| Error::Backend(format!("CreateWindowExW (overlay): {e}")))?;

    // SAFETY: `hwnd` was just created; the attributes apply to it only.
    unsafe {
        SetLayeredWindowAttributes(
            hwnd,
            COLORREF(KEY_COLOR),
            ALPHA,
            LAYERED_WINDOW_ATTRIBUTES_FLAGS(LWA_COLORKEY.0 | LWA_ALPHA.0),
        )
        .map_err(|e| Error::Backend(format!("SetLayeredWindowAttributes: {e}")))?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    Ok(hwnd)
}

unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            // SAFETY: for WM_CREATE, lparam points to a CREATESTRUCTW whose lpCreateParams is
            // the Canvas pointer passed to CreateWindowExW.
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            // SAFETY: storing the canvas pointer for later WM_PAINT calls on this window.
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == CLOSE_TIMER => {
            // SAFETY: posting WM_QUIT to the current thread ends the overlay loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        WM_NCDESTROY => {
            // SAFETY: clears the stored pointer; the canvas itself is owned by the thread.
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            // SAFETY: default handling of the message with the same arguments.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        // SAFETY: unhandled messages go to the default procedure with the same arguments.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn paint(hwnd: HWND) {
    // SAFETY: GWLP_USERDATA holds the canvas pointer set in WM_CREATE. The canvas outlives
    // every window of this thread, so the reference is valid while painting.
    let canvas_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Canvas;
    if canvas_ptr.is_null() {
        return;
    }
    // SAFETY: see above.
    let canvas = unsafe { &*canvas_ptr };

    let mut ps = PAINTSTRUCT::default();
    // SAFETY: `ps` is a valid out-parameter; EndPaint below pairs with BeginPaint.
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    let mut client = RECT::default();
    // SAFETY: `client` is a valid out-parameter.
    let _ = unsafe { GetClientRect(hwnd, &mut client) };

    // SAFETY: the brushes are created here and deleted before returning.
    unsafe {
        let key = CreateSolidBrush(COLORREF(KEY_COLOR));
        FillRect(hdc, &client, key);
        let fill = CreateSolidBrush(COLORREF(ZONE_COLOR));
        let border = CreateSolidBrush(COLORREF(BORDER_COLOR));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(BORDER_COLOR));

        for (name, rect) in &canvas.zones {
            let r = RECT {
                left: rect.x - canvas.origin.0,
                top: rect.y - canvas.origin.1,
                right: rect.x - canvas.origin.0 + i32::try_from(rect.width).unwrap_or(0),
                bottom: rect.y - canvas.origin.1 + i32::try_from(rect.height).unwrap_or(0),
            };
            FillRect(hdc, &r, fill);
            FrameRect(hdc, &r, border);
            let wide: Vec<u16> = name.encode_utf16().collect();
            let _ = TextOutW(hdc, r.left + 8, r.top + 8, &wide);
        }

        let _ = DeleteObject(HGDIOBJ(key.0));
        let _ = DeleteObject(HGDIOBJ(fill.0));
        let _ = DeleteObject(HGDIOBJ(border.0));
        let _ = EndPaint(hwnd, &ps);
    }
}

#[cfg(test)]
mod tests {
    use zones_core::engine::{PreviewMonitor, PreviewZone};

    /// Shows left and right halves on every monitor for a few seconds. Run manually and look.
    #[test]
    #[ignore = "draws on the desktop for a few seconds"]
    fn shows_halves_on_every_monitor() {
        let monitors = crate::display::enumerate().expect("monitors");
        let plan: Vec<PreviewMonitor> = monitors
            .iter()
            .map(|m| {
                let half = m.work_area.width / 2;
                PreviewMonitor {
                    monitor: m.name.clone(),
                    work_area: m.work_area,
                    zones: vec![
                        PreviewZone {
                            name: "Left".into(),
                            rect: zones_core::geometry::Rect::new(
                                m.work_area.x,
                                m.work_area.y,
                                half,
                                m.work_area.height,
                            ),
                        },
                        PreviewZone {
                            name: "Right".into(),
                            rect: zones_core::geometry::Rect::new(
                                m.work_area.x + i32::try_from(half).unwrap_or(0),
                                m.work_area.y,
                                m.work_area.width - half,
                                m.work_area.height,
                            ),
                        },
                    ],
                }
            })
            .collect();
        super::show(plan, std::time::Duration::from_secs(4)).expect("show");
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

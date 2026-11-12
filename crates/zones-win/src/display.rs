//! Monitor enumeration: bounds, work areas, effective DPI and stable device names.

#![allow(unsafe_code)]

use std::mem::size_of;

use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use zones_core::geometry::{Monitor, MonitorId, Rect};
use zones_core::{Error, Result};

/// Default DPI that corresponds to a 100% scale factor.
const BASE_DPI: f64 = 96.0;

/// Enumerates attached monitors in OS order. `MonitorId` is the index in this list,
/// so it is stable only for the current session.
///
/// # Errors
/// Returns [`Error::Backend`] if the OS refuses the enumeration.
pub fn enumerate() -> Result<Vec<Monitor>> {
    let handles = monitor_handles()?;
    handles
        .into_iter()
        .enumerate()
        .map(|(index, hmon)| {
            let index = u32::try_from(index).map_err(|e| Error::Backend(e.to_string()))?;
            describe(hmon, MonitorId(index))
        })
        .collect()
}

fn monitor_handles() -> Result<Vec<HMONITOR>> {
    unsafe extern "system" fn collect(
        hmon: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        // SAFETY: `data` is the `&mut Vec<HMONITOR>` passed to `EnumDisplayMonitors`
        // below, which outlives the call and is not aliased during it.
        let handles = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
        handles.push(hmon);
        TRUE
    }

    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only dereferences `data` as described above. A null
    // device context and clip rectangle request all monitors.
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(std::ptr::addr_of_mut!(handles) as isize),
        )
    };
    if !ok.as_bool() {
        return Err(Error::Backend("EnumDisplayMonitors failed".into()));
    }
    Ok(handles)
}

fn describe(hmon: HMONITOR, id: MonitorId) -> Result<Monitor> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize =
        u32::try_from(size_of::<MONITORINFOEXW>()).map_err(|e| Error::Backend(e.to_string()))?;
    // SAFETY: `info` is a properly sized MONITORINFOEXW whose `cbSize` is set, which is
    // the documented contract for GetMonitorInfoW. The pointer cast to MONITORINFO
    // is the documented way to use the extended structure.
    let ok = unsafe { GetMonitorInfoW(hmon, std::ptr::addr_of_mut!(info).cast::<MONITORINFO>()) };
    if !ok.as_bool() {
        return Err(Error::Backend(format!(
            "GetMonitorInfoW failed for monitor {}",
            id.0
        )));
    }

    let (dpi_x, _dpi_y) = effective_dpi(hmon);
    let scale_factor = f64::from(dpi_x) / BASE_DPI;
    let device = wide_to_string(&info.szDevice);

    Ok(Monitor {
        id,
        name: stable_name(&device),
        bounds: to_rect(info.monitorInfo.rcMonitor),
        work_area: to_rect(info.monitorInfo.rcWork),
        scale_factor,
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

fn effective_dpi(hmon: HMONITOR) -> (u32, u32) {
    let (mut x, mut y) = (0u32, 0u32);
    // SAFETY: both out-pointers refer to live locals. On failure we keep the
    // 96 DPI default, so the return value is ignored deliberately.
    let result = unsafe { GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
    if result.is_err() || x == 0 {
        return (96, 96);
    }
    (x, y)
}

fn to_rect(r: RECT) -> Rect {
    Rect::new(
        r.left,
        r.top,
        u32::try_from(r.right - r.left).unwrap_or(0),
        u32::try_from(r.bottom - r.top).unwrap_or(0),
    )
}

/// Decodes a NUL-terminated UTF-16 buffer.
fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Turns the device path `\\.\DISPLAY1` into the stable name `DISPLAY1`, which is the
/// form used as profile keys in `config.toml`.
#[must_use]
pub fn stable_name(device: &str) -> String {
    device.strip_prefix(r"\\.\").unwrap_or(device).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_name_strips_device_prefix() {
        assert_eq!(stable_name(r"\\.\DISPLAY1"), "DISPLAY1");
        assert_eq!(stable_name("DISPLAY2"), "DISPLAY2");
    }

    #[test]
    fn wide_string_stops_at_nul() {
        let buf = [b'a' as u16, b'b' as u16, 0, b'c' as u16];
        assert_eq!(wide_to_string(&buf), "ab");
    }

    #[test]
    fn enumerates_at_least_one_monitor() {
        let monitors = enumerate().expect("monitor enumeration should succeed");
        assert!(!monitors.is_empty());
        assert!(monitors.iter().any(|m| m.primary));
        for m in &monitors {
            assert!(!m.work_area.is_empty());
            assert!(m.scale_factor >= 1.0);
        }
    }
}

//! Per-user autostart through `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//!
//! Per-user rather than machine-wide, so no elevation is needed and only the current
//! user's logon starts the app.

#![allow(unsafe_code)]

use std::path::Path;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};
use zones_core::{Error, Result};

/// Registry key under HKCU that holds per-user startup entries.
const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
/// Name of our value under the Run key.
const VALUE_NAME: PCWSTR = w!("tpt-zones");

/// Enables or disables autostart. When enabled, `exe` is launched at logon.
///
/// # Errors
/// Returns [`Error::Backend`] if the registry key cannot be opened or written.
pub fn set_enabled(enabled: bool, exe: &Path) -> Result<()> {
    let key = open(KEY_SET_VALUE)?;
    let result = if enabled {
        let command = format!("\"{}\"", exe.display());
        let bytes = to_reg_bytes(&command);
        // SAFETY: `bytes` is a valid UTF-16 buffer with its exact byte length.
        let status = unsafe { RegSetValueExW(key, VALUE_NAME, Some(0), REG_SZ, Some(&bytes)) };
        status
            .ok()
            .map_err(|e| Error::Backend(format!("writing autostart entry: {e}")))
    } else {
        // SAFETY: deleting our own named value; the key handle is valid.
        let status = unsafe { RegDeleteValueW(key, VALUE_NAME) };
        if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(Error::Backend(format!(
                "removing autostart entry: {status:?}"
            )))
        }
    };
    close(key);
    result
}

/// Returns `true` if the autostart entry exists.
///
/// # Errors
/// Returns [`Error::Backend`] for registry failures other than "not found".
pub fn is_enabled() -> Result<bool> {
    let key = match open(KEY_QUERY_VALUE) {
        Ok(key) => key,
        Err(_) => return Ok(false),
    };
    // SAFETY: querying existence only; no output buffer is requested.
    let status = unsafe { RegQueryValueExW(key, VALUE_NAME, None, None, None, None) };
    close(key);
    if status == ERROR_SUCCESS {
        Ok(true)
    } else if status == ERROR_FILE_NOT_FOUND {
        Ok(false)
    } else {
        Err(Error::Backend(format!(
            "querying autostart entry: {status:?}"
        )))
    }
}

fn open(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Result<HKEY> {
    let mut key = HKEY::default();
    // SAFETY: `key` is a valid out-parameter; the path is a static wide string.
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, Some(0), access, &mut key) };
    if status == ERROR_SUCCESS {
        Ok(key)
    } else {
        Err(Error::Backend(format!("opening Run key: {status:?}")))
    }
}

fn close(key: HKEY) {
    // SAFETY: `key` was returned by RegOpenKeyExW and is closed exactly once.
    let _ = unsafe { RegCloseKey(key) };
}

/// Encodes a string as NUL-terminated UTF-16 little-endian bytes for `REG_SZ`.
fn to_reg_bytes(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_bytes_are_nul_terminated_utf16() {
        let bytes = to_reg_bytes("a");
        assert_eq!(bytes, vec![b'a', 0, 0, 0]);
    }

    /// Modifies the real user registry, so it is opt-in.
    #[test]
    #[ignore = "writes to HKCU\\...\\Run; run manually"]
    fn round_trip_autostart() {
        let exe = Path::new(r"C:\tpt-zones-test\tpt-zones.exe");
        set_enabled(true, exe).unwrap();
        assert!(is_enabled().unwrap());
        set_enabled(false, exe).unwrap();
        assert!(!is_enabled().unwrap());
    }
}

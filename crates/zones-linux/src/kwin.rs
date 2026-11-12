//! KWin window and output records, parsed into zones-core types.
//!
//! A KWin script (not yet written) would emit these records as JSON. This module defines that
//! contract and parses it. Keeping the parser separate means the layout and window rules can
//! be tested on any platform, before a live KDE session is involved.
//!
//! Outputs:
//! ```json
//! [{"name":"DP-1","x":0,"y":0,"width":2560,"height":1440,"scale":1.25,"primary":true,
//!   "work":{"x":0,"y":0,"width":2560,"height":1400}}]
//! ```
//! Windows:
//! ```json
//! [{"uuid":"{3f...}","title":"Notes","resourceClass":"kate","x":0,"y":0,"width":800,
//!   "height":600,"minimized":false,"output":"DP-1"}]
//! ```

use serde::Deserialize;
use zones_core::backend::{WindowId, WindowInfo};
use zones_core::geometry::{Monitor, MonitorId, Rect};
use zones_core::{Error, Result};

#[derive(Debug, Deserialize)]
struct RawRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
struct RawOutput {
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
    primary: bool,
    work: RawRect,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWindow {
    uuid: String,
    title: String,
    resource_class: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    minimized: bool,
    output: Option<String>,
}

/// Parses the output list. The index of each output is its `MonitorId` for this session.
///
/// # Errors
/// Returns [`Error::Backend`] if the JSON does not match the contract.
pub fn parse_outputs(json: &str) -> Result<Vec<Monitor>> {
    let raw: Vec<RawOutput> = serde_json::from_str(json).map_err(bad("outputs"))?;
    raw.into_iter()
        .enumerate()
        .map(|(index, o)| {
            Ok(Monitor {
                id: MonitorId(u32::try_from(index).map_err(|e| Error::Backend(e.to_string()))?),
                name: o.name,
                bounds: Rect::new(o.x, o.y, o.width, o.height),
                work_area: Rect::new(o.work.x, o.work.y, o.work.width, o.work.height),
                scale_factor: o.scale,
                primary: o.primary,
            })
        })
        .collect()
}

/// A window with the KWin id it came from, so the backend can address it later.
#[derive(Debug, Clone, PartialEq)]
pub struct KWinWindow {
    /// KWin's own identifier, needed to move the window.
    pub uuid: String,
    /// The window as zones-core sees it.
    pub info: WindowInfo,
}

/// Parses the window list, resolving each window's output name to a monitor id.
///
/// # Errors
/// Returns [`Error::Backend`] if the JSON does not match the contract.
pub fn parse_windows(json: &str, monitors: &[Monitor]) -> Result<Vec<KWinWindow>> {
    let raw: Vec<RawWindow> = serde_json::from_str(json).map_err(bad("windows"))?;
    Ok(raw
        .into_iter()
        .map(|w| {
            let monitor = w
                .output
                .as_deref()
                .and_then(|name| monitors.iter().find(|m| m.name == name))
                .map(|m| m.id);
            KWinWindow {
                info: WindowInfo {
                    id: window_id(&w.uuid),
                    title: w.title,
                    process: w.resource_class.to_ascii_lowercase(),
                    class: None,
                    rect: Rect::new(w.x, w.y, w.width, w.height),
                    monitor,
                    minimized: w.minimized,
                },
                uuid: w.uuid,
            }
        })
        .collect())
}

/// Stable 64-bit id for a KWin UUID (FNV-1a). The same UUID always gives the same id, which
/// keeps client references valid across calls.
#[must_use]
pub fn window_id(uuid: &str) -> WindowId {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let hash = uuid
        .bytes()
        .fold(OFFSET, |h, b| (h ^ u64::from(b)).wrapping_mul(PRIME));
    WindowId(hash)
}

/// The command the backend sends to move a window. Kept here so the contract is in one place.
#[must_use]
pub fn place_command(uuid: &str, rect: Rect) -> String {
    format!(
        "place {uuid} {} {} {} {}",
        rect.x, rect.y, rect.width, rect.height
    )
}

fn bad(what: &'static str) -> impl Fn(serde_json::Error) -> Error {
    move |e| Error::Backend(format!("unexpected KWin {what} data: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUTS: &str = r#"[
      {"name":"DP-1","x":0,"y":0,"width":2560,"height":1440,"scale":1.25,"primary":true,
       "work":{"x":0,"y":0,"width":2560,"height":1400}},
      {"name":"HDMI-1","x":2560,"y":0,"width":1920,"height":1080,"scale":1.0,"primary":false,
       "work":{"x":2560,"y":0,"width":1920,"height":1040}}
    ]"#;

    const WINDOWS: &str = r#"[
      {"uuid":"{3f2a}","title":"Notes","resourceClass":"Kate","x":10,"y":20,"width":800,
       "height":600,"minimized":false,"output":"HDMI-1"},
      {"uuid":"{9b1c}","title":"Gone","resourceClass":"app","x":0,"y":0,"width":1,"height":1,
       "minimized":true,"output":null}
    ]"#;

    #[test]
    fn outputs_keep_order_and_work_areas() {
        let monitors = parse_outputs(OUTPUTS).unwrap();
        assert_eq!(monitors.len(), 2);
        assert_eq!(monitors[0].id, MonitorId(0));
        assert!(monitors[0].primary);
        assert_eq!(monitors[1].work_area, Rect::new(2560, 0, 1920, 1040));
        assert!((monitors[0].scale_factor - 1.25).abs() < f64::EPSILON);
    }

    #[test]
    fn windows_resolve_output_names_to_monitors() {
        let monitors = parse_outputs(OUTPUTS).unwrap();
        let windows = parse_windows(WINDOWS, &monitors).unwrap();
        assert_eq!(windows[0].info.monitor, Some(MonitorId(1)));
        assert_eq!(windows[0].info.process, "kate");
        assert_eq!(windows[1].info.monitor, None);
        assert!(windows[1].info.minimized);
    }

    #[test]
    fn window_ids_are_stable_and_distinct() {
        assert_eq!(window_id("{3f2a}"), window_id("{3f2a}"));
        assert_ne!(window_id("{3f2a}"), window_id("{9b1c}"));
    }

    #[test]
    fn place_command_has_the_agreed_shape() {
        assert_eq!(
            place_command("{3f2a}", Rect::new(1, 2, 3, 4)),
            "place {3f2a} 1 2 3 4"
        );
    }

    #[test]
    fn malformed_data_is_an_error_not_a_panic() {
        assert!(parse_outputs("{not json").is_err());
        assert!(parse_windows(r#"[{"uuid":1}]"#, &[]).is_err());
    }
}

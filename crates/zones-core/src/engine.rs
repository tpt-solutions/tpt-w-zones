//! Placement planning: turns windows, monitors and rules into concrete moves.
//!
//! Planning is pure. [`plan`] reads state and returns the moves to make, and [`apply`]
//! executes them through a [`WindowBackend`]. Keeping the two apart means the rule
//! logic is tested without a desktop session.

use crate::backend::{WindowBackend, WindowId, WindowInfo};
use crate::config::Config;
use crate::geometry::{Monitor, Rect};
use crate::rules::Context;
use crate::{Error, Result};

/// One planned window move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// Window to move.
    pub window: WindowId,
    /// Target rectangle in physical pixels.
    pub target: Rect,
    /// Name of the rule that produced this placement, if it has one.
    pub rule: Option<String>,
}

/// Computes the moves needed so that every window matching a rule sits in its zone.
///
/// Windows already at their target are skipped, so repeated runs are idempotent.
/// Windows whose monitor is unknown, or whose rule refers to a missing layout or zone,
/// are skipped as well. The config is validated, so the latter should not happen.
#[must_use]
pub fn plan(
    config: &Config,
    monitors: &[Monitor],
    windows: &[WindowInfo],
    active_profile: Option<&str>,
) -> Vec<Placement> {
    windows
        .iter()
        .filter_map(|window| {
            let monitor = monitors.iter().find(|m| Some(m.id) == window.monitor)?;
            let ctx = Context {
                monitor: Some(monitor.name.as_str()),
                profile: active_profile,
            };
            let rule = config.placement_for(window, ctx)?;
            let layout = config.layout(&rule.layout)?;
            let zone = layout.zone(&rule.zone)?;
            let target = zone.to_rect(monitor.work_area);
            if window.rect == target {
                return None;
            }
            Some(Placement {
                window: window.id,
                target,
                rule: rule.name.clone(),
            })
        })
        .collect()
}

/// Lists windows and monitors from `backend`, plans placements, and executes them.
///
/// Returns the placements that were applied. A failure on one window is logged and does
/// not stop the others, since windows close and change at any time.
///
/// # Errors
/// Returns an error only if monitors or windows cannot be listed at all.
pub fn apply(
    backend: &dyn WindowBackend,
    config: &Config,
    active_profile: Option<&str>,
) -> Result<Vec<Placement>> {
    let monitors = backend.monitors()?;
    let windows = backend.windows()?;
    let placements = plan(config, &monitors, &windows, active_profile);
    let mut applied = Vec::with_capacity(placements.len());
    for placement in placements {
        match backend.place(placement.window, placement.target) {
            Ok(()) => applied.push(placement),
            Err(e) => tracing::warn!(window = placement.window.0, error = %e, "placement failed"),
        }
    }
    Ok(applied)
}

/// Computes the rectangle for `zone` of `layout` on the monitor that holds `window`.
///
/// # Errors
/// Returns [`Error::NotFound`] if the window, its monitor, the layout or the zone is unknown.
pub fn snap_target(
    config: &Config,
    monitors: &[Monitor],
    windows: &[WindowInfo],
    window: WindowId,
    layout: &str,
    zone: &crate::model::ZoneId,
) -> Result<Rect> {
    let info = require_window(windows, window)?;
    let monitor = info
        .monitor
        .and_then(|id| monitors.iter().find(|m| m.id == id))
        .ok_or_else(|| Error::NotFound(format!("monitor for window {}", window.0)))?;
    let layout_def = config
        .layout(layout)
        .ok_or_else(|| Error::NotFound(format!("layout '{layout}'")))?;
    let zone_def = layout_def
        .zone(zone)
        .ok_or_else(|| Error::NotFound(format!("zone '{}' in layout '{layout}'", zone.0)))?;
    Ok(zone_def.to_rect(monitor.work_area))
}

/// Snaps `window` into a zone and returns the rectangle it was placed at.
///
/// # Errors
/// Returns an error if the window or zone cannot be resolved, or the move fails.
pub fn snap(
    backend: &dyn WindowBackend,
    config: &Config,
    window: WindowId,
    layout: &str,
    zone: &crate::model::ZoneId,
) -> Result<Rect> {
    let monitors = backend.monitors()?;
    let windows = backend.windows()?;
    let target = snap_target(config, &monitors, &windows, window, layout, zone)?;
    backend.place(window, target)?;
    Ok(target)
}

/// Verifies that a `Placement` refers to a window that is still known to the backend.
///
/// # Errors
/// Returns [`Error::NotFound`] if the window is not in `windows`.
pub fn require_window(windows: &[WindowInfo], id: WindowId) -> Result<&WindowInfo> {
    windows
        .iter()
        .find(|w| w.id == id)
        .ok_or_else(|| Error::NotFound(format!("window {}", id.0)))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::backend::{Subscription, WindowEventSink};
    use crate::geometry::{MonitorId, Rect};
    use crate::model::{Layout, NormRect, Zone, ZoneId};
    use crate::rules::Rule;

    /// In-memory backend that records placements.
    struct Fake {
        monitors: Vec<Monitor>,
        windows: Vec<WindowInfo>,
        moves: Mutex<Vec<(WindowId, Rect)>>,
    }

    impl WindowBackend for Fake {
        fn monitors(&self) -> Result<Vec<Monitor>> {
            Ok(self.monitors.clone())
        }
        fn windows(&self) -> Result<Vec<WindowInfo>> {
            Ok(self.windows.clone())
        }
        fn place(&self, id: WindowId, rect: Rect) -> Result<()> {
            if id.0 == 999 {
                return Err(Error::Backend("gone".into()));
            }
            self.moves.lock().unwrap().push((id, rect));
            Ok(())
        }
        fn subscribe(&self, _sink: WindowEventSink) -> Result<Subscription> {
            Ok(Subscription::new(|| {}))
        }
    }

    fn config() -> Config {
        Config {
            layouts: vec![Layout {
                id: "halves".into(),
                name: "Halves".into(),
                zones: vec![
                    Zone {
                        id: ZoneId("left".into()),
                        name: "Left".into(),
                        rect: NormRect {
                            x: 0.0,
                            y: 0.0,
                            w: 0.5,
                            h: 1.0,
                        },
                    },
                    Zone {
                        id: ZoneId("right".into()),
                        name: "Right".into(),
                        rect: NormRect {
                            x: 0.5,
                            y: 0.0,
                            w: 0.5,
                            h: 1.0,
                        },
                    },
                ],
            }],
            rules: vec![Rule {
                name: Some("editor left".into()),
                process: Some("code.exe".into()),
                class: None,
                title_contains: None,
                profile: None,
                monitor: Some("DISPLAY1".into()),
                layout: "halves".into(),
                zone: ZoneId("left".into()),
            }],
            ..Config::default()
        }
    }

    fn monitor() -> Monitor {
        Monitor {
            id: MonitorId(0),
            name: "DISPLAY1".into(),
            bounds: Rect::new(0, 0, 1000, 800),
            work_area: Rect::new(0, 0, 1000, 760),
            scale_factor: 1.0,
            primary: true,
        }
    }

    fn window(id: u64, process: &str, rect: Rect) -> WindowInfo {
        WindowInfo {
            id: WindowId(id),
            title: "t".into(),
            process: process.into(),
            class: None,
            rect,
            monitor: Some(MonitorId(0)),
            minimized: false,
        }
    }

    #[test]
    fn plans_only_matching_windows_that_need_moving() {
        let cfg = config();
        let mons = vec![monitor()];
        let windows = vec![
            window(1, "code.exe", Rect::new(500, 0, 400, 400)), // needs moving
            window(2, "code.exe", Rect::new(0, 0, 500, 760)),   // already in place
            window(3, "other.exe", Rect::new(0, 0, 100, 100)),  // no rule
        ];
        let plan = plan(&cfg, &mons, &windows, None);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].window, WindowId(1));
        assert_eq!(plan[0].target, Rect::new(0, 0, 500, 760));
        assert_eq!(plan[0].rule.as_deref(), Some("editor left"));
    }

    #[test]
    fn rule_monitor_condition_is_respected() {
        let mut cfg = config();
        cfg.rules[0].monitor = Some("DISPLAY2".into());
        let windows = vec![window(1, "code.exe", Rect::new(500, 0, 400, 400))];
        assert!(plan(&cfg, &[monitor()], &windows, None).is_empty());
    }

    #[test]
    fn apply_continues_after_one_failure() {
        let cfg = config();
        let backend = Fake {
            monitors: vec![monitor()],
            windows: vec![
                window(999, "code.exe", Rect::new(500, 0, 400, 400)),
                window(1, "code.exe", Rect::new(500, 0, 400, 400)),
            ],
            moves: Mutex::new(Vec::new()),
        };
        let applied = apply(&backend, &cfg, None).unwrap();
        assert_eq!(applied.len(), 1);
        assert_eq!(backend.moves.lock().unwrap().len(), 1);
        assert_eq!(backend.moves.lock().unwrap()[0].0, WindowId(1));
    }

    #[test]
    fn snap_target_uses_the_windows_monitor_work_area() {
        let cfg = config();
        let mons = vec![monitor()];
        let windows = vec![window(7, "notepad.exe", Rect::new(100, 100, 300, 300))];
        let zone = ZoneId("right".into());
        let target = snap_target(&cfg, &mons, &windows, WindowId(7), "halves", &zone).unwrap();
        assert_eq!(target, Rect::new(500, 0, 500, 760));
    }

    #[test]
    fn snap_target_reports_unknown_zone() {
        let cfg = config();
        let mons = vec![monitor()];
        let windows = vec![window(7, "notepad.exe", Rect::new(0, 0, 1, 1))];
        let zone = ZoneId("missing".into());
        assert!(matches!(
            snap_target(&cfg, &mons, &windows, WindowId(7), "halves", &zone),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn require_window_reports_missing() {
        let windows = vec![window(1, "a", Rect::default())];
        assert!(require_window(&windows, WindowId(1)).is_ok());
        assert!(matches!(
            require_window(&windows, WindowId(2)),
            Err(Error::NotFound(_))
        ));
    }
}

/// One zone to draw in a preview, in screen pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewZone {
    /// Zone name, shown as a label.
    pub name: String,
    /// Zone rectangle in screen pixels.
    pub rect: Rect,
}

/// The zones that a profile would show on one monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewMonitor {
    /// Stable monitor name, e.g. `DISPLAY1`.
    pub monitor: String,
    /// The monitor's work area in screen pixels.
    pub work_area: Rect,
    /// Zones to draw. Empty when the profile assigns no layout to this monitor.
    pub zones: Vec<PreviewZone>,
}

/// Works out what a profile looks like on each monitor, without touching any window.
///
/// Monitors the profile does not mention are still listed, with no zones, so the preview
/// shows the whole setup. An unknown profile name is treated as "no profile".
#[must_use]
pub fn preview_plan(
    config: &Config,
    monitors: &[Monitor],
    profile: Option<&str>,
) -> Vec<PreviewMonitor> {
    let profile = profile.and_then(|name| config.profile(name));
    monitors
        .iter()
        .map(|monitor| {
            let zones = profile
                .and_then(|p| p.layout_for(&monitor.name))
                .and_then(|layout_id| config.layout(layout_id))
                .map(|layout| {
                    layout
                        .zones
                        .iter()
                        .map(|zone| PreviewZone {
                            name: zone.name.clone(),
                            rect: zone.to_rect(monitor.work_area),
                        })
                        .collect()
                })
                .unwrap_or_default();
            PreviewMonitor {
                monitor: monitor.name.clone(),
                work_area: monitor.work_area,
                zones,
            }
        })
        .collect()
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    use crate::geometry::MonitorId;
    use crate::model::{Layout, NormRect, Profile, Zone, ZoneId};

    fn config_with_profile() -> Config {
        let halves = Layout {
            id: "halves".into(),
            name: "Halves".into(),
            zones: vec![
                Zone {
                    id: ZoneId("left".into()),
                    name: "Left".into(),
                    rect: NormRect {
                        x: 0.0,
                        y: 0.0,
                        w: 0.5,
                        h: 1.0,
                    },
                },
                Zone {
                    id: ZoneId("right".into()),
                    name: "Right".into(),
                    rect: NormRect {
                        x: 0.5,
                        y: 0.0,
                        w: 0.5,
                        h: 1.0,
                    },
                },
            ],
        };
        let mut monitors = std::collections::BTreeMap::new();
        monitors.insert("DISPLAY2".to_owned(), "halves".to_owned());
        Config {
            layouts: vec![halves],
            profiles: vec![Profile {
                name: "docked".into(),
                monitors,
            }],
            ..Config::default()
        }
    }

    fn monitor(id: u32, name: &str, x: i32) -> Monitor {
        Monitor {
            id: MonitorId(id),
            name: name.into(),
            bounds: Rect::new(x, 0, 1000, 800),
            work_area: Rect::new(x, 0, 1000, 760),
            scale_factor: 1.0,
            primary: id == 0,
        }
    }

    #[test]
    fn zones_only_on_monitors_the_profile_maps() {
        let cfg = config_with_profile();
        let mons = [monitor(0, "DISPLAY1", 0), monitor(1, "DISPLAY2", 1000)];
        let plan = preview_plan(&cfg, &mons, Some("docked"));
        assert_eq!(plan.len(), 2);
        assert!(plan[0].zones.is_empty());
        assert_eq!(plan[1].zones.len(), 2);
        assert_eq!(plan[1].zones[1].rect, Rect::new(1500, 0, 500, 760));
    }

    #[test]
    fn unknown_profile_shows_monitors_without_zones() {
        let cfg = config_with_profile();
        let mons = [monitor(0, "DISPLAY2", 0)];
        let plan = preview_plan(&cfg, &mons, Some("nope"));
        assert_eq!(plan.len(), 1);
        assert!(plan[0].zones.is_empty());
    }
}

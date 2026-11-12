//! Workspace data model: layouts made of zones, and profiles that bind layouts to monitors.
//!
//! Zones are stored in **normalised** coordinates (`0.0..=1.0` relative to a
//! monitor's work area) so one layout works on any resolution or DPI. They are
//! converted to physical pixels with [`Zone::to_rect`] at placement time.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::geometry::Rect;
use crate::{Error, Result};

/// Identifier for a zone, unique within its layout.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ZoneId(pub String);

/// A rectangle in normalised work-area coordinates. Each component is in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormRect {
    /// Left edge as a fraction of work-area width.
    pub x: f64,
    /// Top edge as a fraction of work-area height.
    pub y: f64,
    /// Width as a fraction of work-area width.
    pub w: f64,
    /// Height as a fraction of work-area height.
    pub h: f64,
}

impl NormRect {
    /// Returns `true` if the rectangle lies within the unit square and has positive area.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let in_unit = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
        in_unit(self.x)
            && in_unit(self.y)
            && in_unit(self.w)
            && in_unit(self.h)
            && self.w > 0.0
            && self.h > 0.0
            && self.x + self.w <= 1.0 + f64::EPSILON
            && self.y + self.h <= 1.0 + f64::EPSILON
    }
}

/// A named region of a layout that windows can be snapped into.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    /// Identifier, unique within the layout.
    pub id: ZoneId,
    /// Display name.
    pub name: String,
    /// Region in normalised work-area coordinates.
    pub rect: NormRect,
}

impl Zone {
    /// Converts the zone to physical pixels inside `work_area`, rounding to the nearest pixel.
    #[must_use]
    pub fn to_rect(&self, work_area: Rect) -> Rect {
        let wa_w = f64::from(work_area.width);
        let wa_h = f64::from(work_area.height);
        // Rounded fractions avoid off-by-one gaps between adjacent zones.
        let left = (self.rect.x * wa_w).round();
        let top = (self.rect.y * wa_h).round();
        let right = ((self.rect.x + self.rect.w) * wa_w).round();
        let bottom = ((self.rect.y + self.rect.h) * wa_h).round();
        // Truncation is safe: the values are within the work area, so they fit in i32/u32.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Rect::new(
            work_area.x + left as i32,
            work_area.y + top as i32,
            (right - left).max(0.0) as u32,
            (bottom - top).max(0.0) as u32,
        )
    }
}

/// A named set of zones that together tile a work area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// Identifier, unique within a configuration.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Zones in this layout.
    pub zones: Vec<Zone>,
}

impl Layout {
    /// Checks that the layout is usable: non-empty, unique zone ids, and every zone valid.
    ///
    /// # Errors
    /// Returns [`Error::Config`] describing the first problem found.
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(Error::Config("layout id must not be empty".into()));
        }
        if self.zones.is_empty() {
            return Err(Error::Config(format!("layout '{}' has no zones", self.id)));
        }
        let mut seen = BTreeSet::new();
        for zone in &self.zones {
            if !seen.insert(&zone.id) {
                return Err(Error::Config(format!(
                    "layout '{}' has duplicate zone id '{}'",
                    self.id, zone.id.0
                )));
            }
            if !zone.rect.is_valid() {
                return Err(Error::Config(format!(
                    "zone '{}' in layout '{}' is outside the work area or empty",
                    zone.id.0, self.id
                )));
            }
        }
        Ok(())
    }

    /// Looks up a zone by id.
    #[must_use]
    pub fn zone(&self, id: &ZoneId) -> Option<&Zone> {
        self.zones.iter().find(|z| &z.id == id)
    }
}

/// Binds layouts to monitors. Monitors are keyed by a **stable** name
/// (device/connector name), not by the session-scoped `MonitorId`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Profile name, e.g. `"docked"` or `"laptop"`.
    pub name: String,
    /// Stable monitor name → layout id.
    #[serde(default)]
    pub monitors: BTreeMap<String, String>,
}

impl Profile {
    /// Returns the layout id assigned to `monitor_name`, if any.
    #[must_use]
    pub fn layout_for(&self, monitor_name: &str) -> Option<&str> {
        self.monitors.get(monitor_name).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn half_left() -> Zone {
        Zone {
            id: ZoneId("left".into()),
            name: "Left".into(),
            rect: NormRect {
                x: 0.0,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            },
        }
    }

    #[test]
    fn zone_to_rect_offsets_by_work_area() {
        let wa = Rect::new(100, 40, 1920, 1000);
        assert_eq!(half_left().to_rect(wa), Rect::new(100, 40, 960, 1000));
    }

    #[test]
    fn adjacent_zones_meet_without_gap_at_odd_widths() {
        let wa = Rect::new(0, 0, 1001, 10);
        let left = Zone {
            id: ZoneId("l".into()),
            name: "l".into(),
            rect: NormRect {
                x: 0.0,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            },
        };
        let right = Zone {
            id: ZoneId("r".into()),
            name: "r".into(),
            rect: NormRect {
                x: 0.5,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            },
        };
        let l = left.to_rect(wa);
        let r = right.to_rect(wa);
        assert_eq!(l.right(), r.x);
        assert_eq!(l.width + r.width, 1001);
    }

    #[test]
    fn validate_rejects_duplicate_ids_and_out_of_bounds() {
        let mut layout = Layout {
            id: "grid".into(),
            name: "Grid".into(),
            zones: vec![half_left(), half_left()],
        };
        assert!(layout.validate().is_err());

        layout.zones = vec![Zone {
            id: ZoneId("x".into()),
            name: "x".into(),
            rect: NormRect {
                x: 0.8,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            },
        }];
        assert!(layout.validate().is_err());
    }

    #[test]
    fn validate_accepts_good_layout() {
        let layout = Layout {
            id: "halves".into(),
            name: "Halves".into(),
            zones: vec![half_left()],
        };
        assert!(layout.validate().is_ok());
    }
}

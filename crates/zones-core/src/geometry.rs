//! Screen-space geometry and monitor description.

use serde::{Deserialize, Serialize};

/// Stable identifier for a monitor within one session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MonitorId(pub u32);

/// An axis-aligned rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Rect {
    /// Creates a rectangle from its origin and size.
    #[must_use]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Right edge (exclusive).
    #[must_use]
    pub fn right(&self) -> i32 {
        self.x
            .saturating_add(i32::try_from(self.width).unwrap_or(i32::MAX))
    }

    /// Bottom edge (exclusive).
    #[must_use]
    pub fn bottom(&self) -> i32 {
        self.y
            .saturating_add(i32::try_from(self.height).unwrap_or(i32::MAX))
    }

    /// Returns `true` when the rectangle has no area.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns `true` when `point` lies inside the rectangle.
    #[must_use]
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// Returns the overlapping region of two rectangles, or `None` if they do not overlap.
    #[must_use]
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = self.right().min(other.right());
        let y1 = self.bottom().min(other.bottom());
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Rect::new(
            x0,
            y0,
            u32::try_from(x1 - x0).ok()?,
            u32::try_from(y1 - y0).ok()?,
        ))
    }

    /// Area in square pixels, as `u64` to avoid overflow on large displays.
    #[must_use]
    pub fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// A physical display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Monitor {
    /// Session-stable identifier.
    pub id: MonitorId,
    /// Human-readable name (e.g. `\\.\DISPLAY1` or the connector name on Linux).
    pub name: String,
    /// Full bounds of the display in physical pixels.
    pub bounds: Rect,
    /// Bounds minus taskbars/panels/docks.
    pub work_area: Rect,
    /// DPI scale factor (1.0 = 100%).
    pub scale_factor: f64,
    /// Whether this is the primary display.
    pub primary: bool,
}

impl Monitor {
    /// Returns the monitor that contains the most of `window`, if any overlaps.
    #[must_use]
    pub fn best_for<'a>(monitors: &'a [Monitor], window: &Rect) -> Option<&'a Monitor> {
        monitors
            .iter()
            .filter_map(|m| m.bounds.intersection(window).map(|r| (m, r.area())))
            .max_by_key(|(_, area)| *area)
            .map(|(m, _)| m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_overlapping() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(50, 50, 100, 100);
        assert_eq!(a.intersection(&b), Some(Rect::new(50, 50, 50, 50)));
    }

    #[test]
    fn intersection_touching_edges_is_none() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(100, 0, 100, 100);
        assert_eq!(a.intersection(&b), None);
    }

    #[test]
    fn contains_is_half_open() {
        let r = Rect::new(10, 10, 5, 5);
        assert!(r.contains(10, 10));
        assert!(r.contains(14, 14));
        assert!(!r.contains(15, 14));
    }

    #[test]
    fn best_monitor_prefers_largest_overlap() {
        let mk = |id, x| Monitor {
            id: MonitorId(id),
            name: format!("m{id}"),
            bounds: Rect::new(x, 0, 1000, 1000),
            work_area: Rect::new(x, 0, 1000, 1000),
            scale_factor: 1.0,
            primary: id == 0,
        };
        let monitors = [mk(0, 0), mk(1, 1000)];
        let window = Rect::new(900, 0, 400, 400); // 100px on m0, 300px on m1
        let best = Monitor::best_for(&monitors, &window).map(|m| m.id);
        assert_eq!(best, Some(MonitorId(1)));
    }
}

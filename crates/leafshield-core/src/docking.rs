//! Physical-pixel geometry. Collapsing changes the window bounds instead of moving
//! most of a window onto an adjacent monitor or behind a taskbar.
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl Rect {
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && y >= self.y as f64
            && x < self.x as f64 + self.width as f64
            && y < self.y as f64 + self.height as f64
    }
    pub fn padded(self, pixels: i32) -> Self {
        Self {
            x: self.x - pixels,
            y: self.y - pixels,
            width: self.width + (pixels * 2) as u32,
            height: self.height + (pixels * 2) as u32,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dock {
    pub edge: Edge,
    pub full: Rect,
    pub work: Rect,
    pub scale: f64,
}
impl Dock {
    pub fn snap(window: Rect, work: Rect, scale: f64) -> Option<Self> {
        // A native drag can put the window origin beyond an edge because the
        // pointer stays at its grab offset. Any remaining visible intersection
        // qualifies that crossed edge; a separate monitor must not attract it.
        if window.width == 0
            || window.height == 0
            || work.width == 0
            || work.height == 0
            || window.x + window.width as i32 <= work.x
            || window.x >= work.x + work.width as i32
            || window.y + window.height as i32 <= work.y
            || window.y >= work.y + work.height as i32
        {
            return None;
        }
        let threshold = (16.0 * scale).round() as i32;
        let edges = [
            (window.x - work.x, Edge::Left),
            (
                work.x + work.width as i32 - window.x - window.width as i32,
                Edge::Right,
            ),
            (window.y - work.y, Edge::Top),
            (
                work.y + work.height as i32 - window.y - window.height as i32,
                Edge::Bottom,
            ),
        ];
        let (_, edge) = edges
            .into_iter()
            .filter(|(distance, _)| *distance <= threshold)
            .min_by_key(|(distance, _)| (*distance).max(0))?;
        let mut full = window;
        full.x = full.x.clamp(
            work.x,
            (work.x + work.width as i32 - full.width as i32).max(work.x),
        );
        full.y = full.y.clamp(
            work.y,
            (work.y + work.height as i32 - full.height as i32).max(work.y),
        );
        match edge {
            Edge::Left => full.x = work.x,
            Edge::Right => full.x = work.x + work.width as i32 - full.width as i32,
            Edge::Top => full.y = work.y,
            Edge::Bottom => full.y = work.y + work.height as i32 - full.height as i32,
        }
        Some(Self {
            edge,
            full,
            work,
            scale,
        })
    }
    pub fn collapsed(self) -> Rect {
        let thin = (8.0 * self.scale).round().max(1.0) as u32;
        let vertical = (40.0 * self.scale).round().max(1.0) as u32;
        let horizontal = (72.0 * self.scale).round().max(1.0) as u32;
        match self.edge {
            Edge::Left | Edge::Right => Rect {
                x: if self.edge == Edge::Left {
                    self.work.x
                } else {
                    self.work.x + self.work.width as i32 - thin as i32
                },
                y: self.full.y + (self.full.height.saturating_sub(vertical) / 2) as i32,
                width: thin,
                height: vertical.min(self.full.height),
            },
            Edge::Top | Edge::Bottom => Rect {
                x: self.full.x + (self.full.width.saturating_sub(horizontal) / 2) as i32,
                y: if self.edge == Edge::Top {
                    self.work.y
                } else {
                    self.work.y + self.work.height as i32 - thin as i32
                },
                width: horizontal.min(self.full.width),
                height: thin,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const WORK: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };
    fn widget(x: i32, y: i32) -> Rect {
        Rect {
            x,
            y,
            width: 176,
            height: 54,
        }
    }
    #[test]
    fn snaps_four_edges_and_keeps_tabs_inside_work_area() {
        for (rect, edge) in [
            (widget(3, 200), Edge::Left),
            (widget(1740, 200), Edge::Right),
            (widget(500, 2), Edge::Top),
            (widget(500, 980), Edge::Bottom),
        ] {
            let dock = Dock::snap(rect, WORK, 1.0).unwrap();
            assert_eq!(dock.edge, edge);
            for bounds in [dock.full, dock.collapsed()] {
                assert!(bounds.x >= WORK.x && bounds.y >= WORK.y);
                assert!(bounds.x + bounds.width as i32 <= 1920);
                assert!(bounds.y + bounds.height as i32 <= 1040);
            }
        }
    }
    #[test]
    fn shared_monitor_boundary_never_leaks_onto_next_monitor() {
        let dock = Dock::snap(widget(1744, 300), WORK, 1.0).unwrap();
        let tab = dock.collapsed();
        assert_eq!(tab.x + tab.width as i32, 1920);
        assert_eq!(tab.width, 8);
        assert_eq!(dock.full.x + dock.full.width as i32, 1920);
    }
    #[test]
    fn negative_coordinates_and_high_dpi_keep_correct_physical_geometry() {
        let work = Rect {
            x: -2560,
            y: -180,
            width: 2560,
            height: 1400,
        };
        let full = Rect {
            x: -355,
            y: 300,
            width: 352,
            height: 108,
        };
        let dock = Dock::snap(full, work, 2.0).unwrap();
        assert_eq!(dock.edge, Edge::Right);
        assert_eq!(dock.collapsed().width, 16);
        assert_eq!(dock.collapsed().x, -16);
        assert_eq!(dock.full.x, -352);
    }
    #[test]
    fn middle_of_monitor_does_not_snap_and_taskbar_stays_clear() {
        assert!(Dock::snap(widget(500, 400), WORK, 1.0).is_none());
        let dock = Dock::snap(widget(1000, 985), WORK, 1.0).unwrap();
        assert_eq!(dock.collapsed().y, 1032);
        assert_eq!(dock.collapsed().height, 8);
        assert!(!dock.collapsed().contains(1000.0, 1060.0));
    }
    #[test]
    fn reported_left_drag_74_pixels_outside_still_docks() {
        let window = widget(-74, 801);
        let dock =
            Dock::snap(window, WORK, 1.0).expect("partially visible left-side window must dock");
        assert_eq!(dock.edge, Edge::Left);
        assert_eq!(dock.full, widget(0, 801));
        assert_eq!(
            dock.collapsed(),
            Rect {
                x: 0,
                y: 808,
                width: 8,
                height: 40
            }
        );
    }
    #[test]
    fn crossing_any_work_area_edge_still_snaps_inside() {
        for (window, edge) in [
            (widget(-74, 200), Edge::Left),
            (widget(1818, 200), Edge::Right),
            (widget(500, -20), Edge::Top),
            (widget(500, 1006), Edge::Bottom),
        ] {
            let dock = Dock::snap(window, WORK, 1.0).expect("partially visible window must snap");
            assert_eq!(dock.edge, edge);
            for rect in [dock.full, dock.collapsed()] {
                assert!(rect.x >= WORK.x && rect.y >= WORK.y);
                assert!(rect.x + rect.width as i32 <= WORK.x + WORK.width as i32);
                assert!(rect.y + rect.height as i32 <= WORK.y + WORK.height as i32);
            }
        }
    }
    #[test]
    fn overshoot_respects_negative_monitor_origin_dpi_and_left_taskbar() {
        let work = Rect {
            x: -2560,
            y: -180,
            width: 2560,
            height: 1400,
        };
        let dock = Dock::snap(
            Rect {
                x: -2708,
                y: 300,
                width: 352,
                height: 108,
            },
            work,
            2.0,
        )
        .unwrap();
        assert_eq!(dock.edge, Edge::Left);
        assert_eq!(dock.full.x, -2560);
        assert_eq!(dock.collapsed().width, 16);
        let taskbar_work = Rect {
            x: 48,
            y: 0,
            width: 1872,
            height: 1080,
        };
        let dock = Dock::snap(widget(0, 300), taskbar_work, 1.0).unwrap();
        assert_eq!(dock.edge, Edge::Left);
        assert_eq!(dock.full.x, 48);
        assert_eq!(dock.collapsed().x, 48);
    }
    #[test]
    fn completely_separate_windows_do_not_jump_to_a_monitor() {
        for window in [
            widget(-176, 200),
            widget(1920, 200),
            widget(500, -54),
            widget(500, 1040),
        ] {
            assert!(Dock::snap(window, WORK, 1.0).is_none());
        }
    }
}

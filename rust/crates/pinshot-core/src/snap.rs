//! Magnetic snap of a selection to window / control edges.
//!
//! Hover highlight (click-to-select) is a separate path (`hover_rect`). This
//! module snaps the moving corner or edges while the user drags, moves, or
//! resizes a custom selection — the missing “窗口/控件吸附”.

use crate::geom::{Handle, Point, Rect};

/// Distance, in overlay pixels, at which an edge sticks to a guide.
pub const SNAP_PX: f32 = 8.0;

#[derive(Clone, Debug, Default)]
pub struct SnapGuides {
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
}

impl SnapGuides {
    pub fn from_rects(rects: impl IntoIterator<Item = Rect>) -> Self {
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for r in rects {
            if r.w < 2.0 || r.h < 2.0 {
                continue;
            }
            xs.push(r.x);
            xs.push(r.x + r.w);
            ys.push(r.y);
            ys.push(r.y + r.h);
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        dedup_close(&mut xs);
        dedup_close(&mut ys);
        Self { xs, ys }
    }

    pub fn is_empty(&self) -> bool {
        self.xs.is_empty() && self.ys.is_empty()
    }

    pub fn snap_value(v: f32, guides: &[f32], thresh: f32) -> f32 {
        match nearest_delta(v, guides, thresh) {
            Some(d) => v + d,
            None => v,
        }
    }

    pub fn snap_point(&self, p: Point, thresh: f32) -> Point {
        Point::new(
            Self::snap_value(p.x, &self.xs, thresh),
            Self::snap_value(p.y, &self.ys, thresh),
        )
    }

    /// Translate `r` (keep size) so the closest pair of edges sticks to a guide.
    pub fn snap_rect_move(&self, r: Rect, thresh: f32) -> Rect {
        let (x0, x1) = snap_pair_keep_span(r.x, r.x + r.w, &self.xs, thresh);
        let (y0, y1) = snap_pair_keep_span(r.y, r.y + r.h, &self.ys, thresh);
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    /// Snap only the edges that `handle` is moving.
    pub fn snap_rect_resize(&self, r: Rect, handle: Handle, thresh: f32) -> Rect {
        let mut x0 = r.x;
        let mut y0 = r.y;
        let mut x1 = r.x + r.w;
        let mut y1 = r.y + r.h;
        if handle.has_left() {
            x0 = Self::snap_value(x0, &self.xs, thresh);
        }
        if handle.has_right() {
            x1 = Self::snap_value(x1, &self.xs, thresh);
        }
        if handle.has_top() {
            y0 = Self::snap_value(y0, &self.ys, thresh);
        }
        if handle.has_bottom() {
            y1 = Self::snap_value(y1, &self.ys, thresh);
        }
        Rect::new(x0, y0, x1 - x0, y1 - y0).normalized()
    }
}

fn nearest_delta(v: f32, guides: &[f32], thresh: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    for &g in guides {
        let d = g - v;
        if d.abs() <= thresh {
            best = Some(match best {
                Some(cur) if cur.abs() <= d.abs() => cur,
                _ => d,
            });
        }
    }
    best
}

fn snap_pair_keep_span(a: f32, b: f32, guides: &[f32], thresh: f32) -> (f32, f32) {
    let da = nearest_delta(a, guides, thresh);
    let db = nearest_delta(b, guides, thresh);
    let d = match (da, db) {
        (Some(x), Some(y)) if x.abs() <= y.abs() => x,
        (Some(_), Some(y)) => y,
        (Some(x), None) | (None, Some(x)) => x,
        (None, None) => 0.0,
    };
    (a + d, b + d)
}

fn dedup_close(v: &mut Vec<f32>) {
    let mut o = 0;
    for i in 0..v.len() {
        if o == 0 || (v[i] - v[o - 1]).abs() >= 0.5 {
            v[o] = v[i];
            o += 1;
        }
    }
    v.truncate(o);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_value_sticks_within_threshold() {
        let g = [100.0, 400.0];
        assert_eq!(SnapGuides::snap_value(105.0, &g, SNAP_PX), 100.0);
        assert_eq!(SnapGuides::snap_value(200.0, &g, SNAP_PX), 200.0);
        assert_eq!(SnapGuides::snap_value(396.0, &g, SNAP_PX), 400.0);
    }

    #[test]
    fn guides_from_window_and_control() {
        let win = Rect::new(10.0, 20.0, 200.0, 100.0);
        let btn = Rect::new(30.0, 40.0, 80.0, 24.0);
        let g = SnapGuides::from_rects([win, btn]);
        assert!(g.xs.contains(&10.0) && g.xs.contains(&210.0));
        assert!(g.xs.contains(&30.0) && g.xs.contains(&110.0));
        assert!(g.ys.contains(&40.0) && g.ys.contains(&64.0));
    }

    #[test]
    fn moving_rect_snaps_as_a_whole() {
        let g = SnapGuides::from_rects([Rect::new(0.0, 0.0, 200.0, 80.0)]);
        let r = Rect::new(6.0, 50.0, 40.0, 20.0);
        let out = g.snap_rect_move(r, SNAP_PX);
        assert_eq!(out, Rect::new(0.0, 50.0, 40.0, 20.0));
    }

    #[test]
    fn resize_snaps_only_moving_edge() {
        let g = SnapGuides::from_rects([Rect::new(0.0, 0.0, 200.0, 80.0)]);
        let r = Rect::new(10.0, 10.0, 186.0, 40.0);
        let out = g.snap_rect_resize(r, Handle::R, SNAP_PX);
        assert!((out.x + out.w - 200.0).abs() < 0.01);
        assert_eq!(out.x, 10.0);
    }

    #[test]
    fn drag_corner_snaps_to_control() {
        let g = SnapGuides::from_rects([Rect::new(40.0, 40.0, 80.0, 24.0)]);
        let p = g.snap_point(Point::new(118.0, 66.0), SNAP_PX);
        assert_eq!(p, Point::new(120.0, 64.0));
    }
}

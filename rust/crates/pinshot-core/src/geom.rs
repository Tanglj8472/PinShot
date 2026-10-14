//! Screen / image geometry. Internal truth is physical pixels.

use serde::{Deserialize, Serialize};

/// `f32::clamp` panics when `min > max` (or NaN). Toolbar/menu placement hits
/// that when a pin is narrower than the chrome.
pub fn clamp_f32(v: f32, a: f32, b: f32) -> f32 {
    let lo = if a.is_finite() && b.is_finite() {
        a.min(b)
    } else if a.is_finite() {
        a
    } else if b.is_finite() {
        b
    } else {
        return if v.is_finite() { v } else { 0.0 };
    };
    let hi = if a.is_finite() && b.is_finite() {
        a.max(b)
    } else {
        lo
    };
    if !v.is_finite() {
        return lo;
    }
    v.clamp(lo, hi)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn from_i32(x: i32, y: i32) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
        }
    }

    pub fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    pub fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }

    pub fn scale(self, s: f32) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
        }
    }

    pub fn manhattan(self, other: Self) -> f32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }

    pub fn distance(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    pub fn clamp_to(self, r: Rect) -> Self {
        Self {
            x: clamp_f32(self.x, r.x, r.right()),
            y: clamp_f32(self.y, r.y, r.bottom()),
        }
    }

    pub fn round_i32(self) -> (i32, i32) {
        (self.x.round() as i32, self.y.round() as i32)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const EMPTY: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn from_xyxy(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        let x = x0.min(x1);
        let y = y0.min(y1);
        Self {
            x,
            y,
            w: (x0 - x1).abs(),
            h: (y0 - y1).abs(),
        }
    }

    pub fn from_points(a: Point, b: Point) -> Self {
        Self::from_xyxy(a.x, a.y, b.x, b.y)
    }

    /// Drag-span rect: size equals the drag distance (not inclusive +1).
    pub fn span(a: Point, b: Point) -> Self {
        Self::from_points(a, b)
    }

    pub fn from_i32(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
            w: w as f32,
            h: h as f32,
        }
    }

    pub fn is_empty(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// Selection is "real" once it is larger than 2×2 (matches Python `has_sel`).
    pub fn is_valid_sel(self) -> bool {
        self.normalized().w > 2.0 && self.normalized().h > 2.0
    }

    pub fn right(self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    pub fn center(self) -> Point {
        Point::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    pub fn top_left(self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn bottom_right(self) -> Point {
        Point::new(self.right(), self.bottom())
    }

    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    pub fn contains_rect(self, other: Self) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }

    pub fn intersects(self, other: Self) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    pub fn intersect(self, other: Self) -> Self {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = self.right().min(other.right());
        let y1 = self.bottom().min(other.bottom());
        if x1 <= x0 || y1 <= y0 {
            Self::EMPTY
        } else {
            Self::from_xyxy(x0, y0, x1, y1)
        }
    }

    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = self.right().max(other.right());
        let y1 = self.bottom().max(other.bottom());
        Self::from_xyxy(x0, y0, x1, y1)
    }

    pub fn normalized(self) -> Self {
        let mut r = self;
        if r.w < 0.0 {
            r.x += r.w;
            r.w = -r.w;
        }
        if r.h < 0.0 {
            r.y += r.h;
            r.h = -r.h;
        }
        r
    }

    pub fn translated(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            w: self.w,
            h: self.h,
        }
    }

    pub fn inflated(self, pad: f32) -> Self {
        Self {
            x: self.x - pad,
            y: self.y - pad,
            w: self.w + pad * 2.0,
            h: self.h + pad * 2.0,
        }
    }

    /// Four-band subtraction: parts of `self` that do not overlap `other`.
    pub fn subtract(self, other: Self) -> [Self; 4] {
        let i = self.intersect(other);
        if self.is_empty() {
            return [Self::EMPTY; 4];
        }
        if i.is_empty() {
            return [self, Self::EMPTY, Self::EMPTY, Self::EMPTY];
        }
        [
            if self.y < i.y {
                Self::new(self.x, self.y, self.w, i.y - self.y)
            } else {
                Self::EMPTY
            },
            if self.bottom() > i.bottom() {
                Self::new(self.x, i.bottom(), self.w, self.bottom() - i.bottom())
            } else {
                Self::EMPTY
            },
            if self.x < i.x {
                Self::new(self.x, i.y, i.x - self.x, i.h)
            } else {
                Self::EMPTY
            },
            if self.right() > i.right() {
                Self::new(i.right(), i.y, self.right() - i.right(), i.h)
            } else {
                Self::EMPTY
            },
        ]
    }

    pub fn adjusted(self, l: f32, t: f32, r: f32, b: f32) -> Self {
        Self {
            x: self.x + l,
            y: self.y + t,
            w: self.w + r - l,
            h: self.h + b - t,
        }
    }

    pub fn area(self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }

    pub fn clamp_inside(self, bounds: Self) -> Self {
        let mut r = self.normalized();
        r.w = r.w.min(bounds.w.max(0.0));
        r.h = r.h.min(bounds.h.max(0.0));
        r.x = clamp_f32(r.x, bounds.x, bounds.right() - r.w);
        r.y = clamp_f32(r.y, bounds.y, bounds.bottom() - r.h);
        r
    }

    pub fn to_i32(self) -> (i32, i32, i32, i32) {
        let r = self.normalized();
        (
            r.x.round() as i32,
            r.y.round() as i32,
            r.w.round().max(0.0) as i32,
            r.h.round().max(0.0) as i32,
        )
    }

    pub fn size_label(self) -> String {
        let (_, _, w, h) = self.to_i32();
        format!("{w} × {h} px")
    }
}

/// Eight resize handles around a selection (Python overlay `_handles`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handle {
    Tl,
    T,
    Tr,
    L,
    R,
    Bl,
    B,
    Br,
}

impl Handle {
    pub const ALL: [Handle; 8] = [
        Handle::Tl,
        Handle::T,
        Handle::Tr,
        Handle::L,
        Handle::R,
        Handle::Bl,
        Handle::B,
        Handle::Br,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Handle::Tl => "tl",
            Handle::T => "t",
            Handle::Tr => "tr",
            Handle::L => "l",
            Handle::R => "r",
            Handle::Bl => "bl",
            Handle::B => "b",
            Handle::Br => "br",
        }
    }

    pub fn has_left(self) -> bool {
        matches!(self, Handle::Tl | Handle::L | Handle::Bl)
    }

    pub fn has_right(self) -> bool {
        matches!(self, Handle::Tr | Handle::R | Handle::Br)
    }

    pub fn has_top(self) -> bool {
        matches!(self, Handle::Tl | Handle::T | Handle::Tr)
    }

    pub fn has_bottom(self) -> bool {
        matches!(self, Handle::Bl | Handle::B | Handle::Br)
    }
}

pub const HANDLE_SIZE: f32 = 8.0;

/// Overlay selection handle slop. Matches Python `_hit_handle` (`HANDLE+2` = 8).
pub const HANDLE_HIT: f32 = 8.0;

/// Handle positions. Matches Python: xs = left / center / right+1, ys = top / center / bottom+1.
pub fn handle_points(r: Rect) -> [(Handle, Point); 8] {
    let r = r.normalized();
    let xs = [r.x, r.x + r.w * 0.5, r.right() + 1.0];
    let ys = [r.y, r.y + r.h * 0.5, r.bottom() + 1.0];
    [
        (Handle::Tl, Point::new(xs[0], ys[0])),
        (Handle::T, Point::new(xs[1], ys[0])),
        (Handle::Tr, Point::new(xs[2], ys[0])),
        (Handle::L, Point::new(xs[0], ys[1])),
        (Handle::R, Point::new(xs[2], ys[1])),
        (Handle::Bl, Point::new(xs[0], ys[2])),
        (Handle::B, Point::new(xs[1], ys[2])),
        (Handle::Br, Point::new(xs[2], ys[2])),
    ]
}

pub fn hit_handle(r: Rect, p: Point, slop: f32) -> Option<Handle> {
    if !r.is_valid_sel() {
        return None;
    }
    let slop = slop.max(HANDLE_SIZE / 2.0 + 2.0);
    for (h, c) in handle_points(r) {
        if (p.x - c.x).abs() <= slop && (p.y - c.y).abs() <= slop {
            return Some(h);
        }
    }
    None
}

/// Overlay selection handles. `handle_points` sit on `right()+1` / `bottom()+1`,
/// which a pointer clamped to the screen edge cannot reach. On a large rect,
/// a grab within [`HANDLE_HIT`] of the border still resizes.
pub fn hit_sel_handle(r: Rect, p: Point) -> Option<Handle> {
    if let Some(h) = hit_handle(r, p, HANDLE_HIT) {
        return Some(h);
    }
    let r = r.normalized();
    if r.w < 80.0 || r.h < 80.0 {
        return None;
    }
    if p.x < r.x - HANDLE_HIT
        || p.x > r.right() + HANDLE_HIT
        || p.y < r.y - HANDLE_HIT
        || p.y > r.bottom() + HANDLE_HIT
    {
        return None;
    }
    let left = (p.x - r.x).abs() <= HANDLE_HIT;
    let right = (p.x - r.right()).abs() <= HANDLE_HIT;
    let top = (p.y - r.y).abs() <= HANDLE_HIT;
    let bottom = (p.y - r.bottom()).abs() <= HANDLE_HIT;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(Handle::Tl),
        (_, true, true, _) => Some(Handle::Tr),
        (true, _, _, true) => Some(Handle::Bl),
        (_, true, _, true) => Some(Handle::Br),
        (true, false, false, false) => Some(Handle::L),
        (false, true, false, false) => Some(Handle::R),
        (false, false, true, false) => Some(Handle::T),
        (false, false, false, true) => Some(Handle::B),
        _ => None,
    }
}

/// Resize `r` by dragging `handle` to `p` (Python overlay `resizing` branch).
pub fn resize_rect(r: Rect, handle: Handle, p: Point) -> Rect {
    let mut r = r.normalized();
    if handle.has_left() {
        r.w += r.x - p.x;
        r.x = p.x;
    }
    if handle.has_right() {
        r.w = p.x - r.x;
    }
    if handle.has_top() {
        r.h += r.y - p.y;
        r.y = p.y;
    }
    if handle.has_bottom() {
        r.h = p.y - r.y;
    }
    r.normalized()
}

/// Panel / dock / taskbar: full-width short strip or full-height thin bar.
pub fn is_shell_strip(r: Rect, full: Rect) -> bool {
    if r.is_empty() || full.is_empty() {
        return false;
    }
    let full_w = r.w >= full.w * 0.85;
    let full_h = r.h >= full.h * 0.85;
    let short_h = r.h <= full.h.max(1.0) * 0.18 && r.h <= 220.0;
    let thin_w = r.w <= full.w.max(1.0) * 0.18 && r.w <= 220.0;
    (full_w && short_h) || (full_h && thin_w)
}

/// Overlay pixels → virtual-desktop (screen) pixels.
pub fn screen_from_overlay(p: Point, origin: (i32, i32)) -> (i32, i32) {
    (
        (p.x + origin.0 as f32).round() as i32,
        (p.y + origin.1 as f32).round() as i32,
    )
}

/// Screen physical rect → overlay-local rect (`-virtual_origin`).
pub fn overlay_from_screen(r: Rect, origin: (i32, i32)) -> Rect {
    r.translated(-(origin.0 as f32), -(origin.1 as f32))
}

/// Window / element hover: smallest containing element, else the topmost
/// non-dock window under the cursor (`windows` is topmost-first), else full screen.
pub fn hover_rect(cursor: Point, windows: &[Rect], elements: &[Rect], full: Rect) -> Rect {
    if let Some(r) = elements
        .iter()
        .copied()
        .filter(|r| r.contains(cursor))
        .min_by(|a, b| {
            a.area()
                .partial_cmp(&b.area())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    {
        return r;
    }
    let under: Vec<Rect> = windows
        .iter()
        .copied()
        .map(|r| r.intersect(full))
        .filter(|r| !r.is_empty() && r.contains(cursor))
        .collect();
    // Win11 maximized HWNDs include the taskbar band. Once the cursor is in
    // any shell strip, pick the smallest non-backdrop (notify chip < bar <
    // the app that extends under the bar).
    if under.iter().any(|r| is_shell_strip(*r, full)) {
        if let Some(r) = under
            .iter()
            .copied()
            .filter(|r| !is_desktop_backdrop(*r, full))
            .min_by(|a, b| {
                a.area()
                    .partial_cmp(&b.area())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            return r;
        }
    }
    // A full-desktop WorkerW/Progman contains the taskbar pixels too.
    // Prefer a real app, then the backdrop — otherwise leaving the tray
    // menu and coming back selects the desktop.
    under
        .iter()
        .copied()
        .find(|r| !is_shell_strip(*r, full) && !is_desktop_backdrop(*r, full))
        .or_else(|| under.first().copied())
        .unwrap_or(full)
}

/// True desktop / wallpaper host: both axes nearly the virtual desktop.
/// Maximized apps sit above the taskbar and fail this (~85–92% tall).
pub fn is_desktop_backdrop(r: Rect, full: Rect) -> bool {
    if r.is_empty() || full.is_empty() {
        return false;
    }
    r.w >= full.w * 0.95 && r.h >= full.h * 0.95
}

/// Map a physical-pixel rect onto a monitor's logical geometry (Python `phys_to_logical`).
pub fn phys_to_logical(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    screens: &[(f32, f32, f32, f32, f32)],
) -> (i32, i32, i32, i32) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let (sx, sy, dpr) = screens
        .iter()
        .copied()
        .find(|(sx, sy, sw, sh, dpr)| {
            cx >= *sx && cx < sx + sw * dpr && cy >= *sy && cy < sy + sh * dpr
        })
        .map(|(sx, sy, _, _, dpr)| (sx, sy, dpr))
        .unwrap_or_else(|| {
            screens
                .first()
                .map(|(sx, sy, _, _, dpr)| (*sx, *sy, *dpr))
                .unwrap_or((0.0, 0.0, 1.0))
        });
    (
        (sx + (x - sx) / dpr).round() as i32,
        (sy + (y - sy) / dpr).round() as i32,
        (w / dpr).round() as i32,
        (h / dpr).round() as i32,
    )
}

/// Distance from point to a finite segment.
pub fn dist_to_segment(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if len2 <= f32::EPSILON {
        return p.distance(a);
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

/// Distance from point to a polyline.
pub fn dist_to_polyline(p: Point, pts: &[Point]) -> f32 {
    if pts.is_empty() {
        return f32::MAX;
    }
    if pts.len() == 1 {
        return p.distance(pts[0]);
    }
    pts.windows(2)
        .map(|w| dist_to_segment(p, w[0], w[1]))
        .fold(f32::MAX, f32::min)
}

pub fn point_in_ellipse(p: Point, r: Rect) -> bool {
    if r.w <= 0.0 || r.h <= 0.0 {
        return false;
    }
    let c = r.center();
    let rx = r.w * 0.5;
    let ry = r.h * 0.5;
    let nx = (p.x - c.x) / rx;
    let ny = (p.y - c.y) / ry;
    nx * nx + ny * ny <= 1.0
}

/// Constrain a drag so the shape is a square / circle (Shift).
pub fn constrain_square(origin: Point, current: Point) -> Point {
    let d = (current.x - origin.x)
        .abs()
        .max((current.y - origin.y).abs());
    Point::new(
        origin.x + d * (current.x - origin.x).signum(),
        origin.y + d * (current.y - origin.y).signum(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_is_not_inclusive() {
        let r = Rect::span(Point::new(10.0, 20.0), Point::new(40.0, 50.0));
        assert_eq!(r, Rect::new(10.0, 20.0, 30.0, 30.0));
    }

    #[test]
    fn valid_sel_threshold() {
        assert!(!Rect::new(0.0, 0.0, 2.0, 2.0).is_valid_sel());
        assert!(Rect::new(0.0, 0.0, 3.0, 3.0).is_valid_sel());
    }

    #[test]
    fn resize_corners() {
        let r = Rect::new(10.0, 10.0, 40.0, 20.0);
        let out = resize_rect(r, Handle::Br, Point::new(80.0, 50.0));
        assert_eq!(out, Rect::new(10.0, 10.0, 70.0, 40.0));
        let out = resize_rect(r, Handle::Tl, Point::new(0.0, 0.0));
        assert_eq!(out, Rect::new(0.0, 0.0, 50.0, 30.0));
    }

    #[test]
    fn handle_hit() {
        let r = Rect::new(100.0, 100.0, 50.0, 40.0);
        assert_eq!(
            hit_handle(r, Point::new(100.0, 100.0), 4.0),
            Some(Handle::Tl)
        );
        assert_eq!(hit_handle(r, Point::new(125.0, 200.0), 4.0), None);
    }

    #[test]
    fn handle_hit_inside_screen_edge_corner() {
        let r = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(
            hit_sel_handle(r, Point::new(1916.0, 1076.0)),
            Some(Handle::Br)
        );
        assert_eq!(hit_sel_handle(r, Point::new(4.0, 4.0)), Some(Handle::Tl));
        assert_eq!(hit_sel_handle(r, Point::new(960.0, 540.0)), None);
        // Small rect: interior stays Move (not a border grab).
        let small = Rect::new(10.0, 10.0, 40.0, 20.0);
        assert_eq!(hit_sel_handle(small, Point::new(20.0, 15.0)), None);
    }

    #[test]
    fn overlay_screen_roundtrip_uses_virtual_origin() {
        // Dual-monitor desktop like DISPLAY1 at (-2240, -186).
        let origin = (-2240, -186);
        let overlay_pt = Point::new(100.0, 50.0);
        let (sx, sy) = screen_from_overlay(overlay_pt, origin);
        assert_eq!((sx, sy), (-2140, -136));
        let screen = Rect::from_i32(-2140, -136, 80, 24);
        let local = overlay_from_screen(screen, origin);
        assert_eq!(
            (
                local.x as i32,
                local.y as i32,
                local.w as i32,
                local.h as i32
            ),
            (100, 50, 80, 24)
        );
    }

    #[test]
    fn hover_picks_smallest_element() {
        let full = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let windows = [Rect::new(10.0, 10.0, 800.0, 600.0)];
        let elements = [
            Rect::new(10.0, 10.0, 800.0, 600.0),
            Rect::new(40.0, 40.0, 80.0, 24.0),
        ];
        let h = hover_rect(Point::new(50.0, 50.0), &windows, &elements, full);
        assert_eq!(h, elements[1]);
    }

    #[test]
    fn hover_falls_back_to_window_then_full() {
        let full = Rect::new(0.0, 0.0, 100.0, 100.0);
        let windows = [Rect::new(10.0, 10.0, 20.0, 20.0)];
        assert_eq!(
            hover_rect(Point::new(15.0, 15.0), &windows, &[], full),
            windows[0]
        );
        assert_eq!(
            hover_rect(Point::new(90.0, 90.0), &windows, &[], full),
            full
        );
    }

    #[test]
    fn hover_skips_dock_strip_for_window_under_cursor() {
        let full = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let dock = Rect::new(0.0, 1063.0, 1920.0, 137.0);
        let chrome = Rect::new(0.0, 0.0, 1920.0, 1063.0);
        // Topmost-first list: always-on-top panel, then Chrome.
        let windows = [dock, chrome];
        let h = hover_rect(Point::new(400.0, 720.0), &windows, &[], full);
        assert_eq!(h, chrome);
        let over_dock = hover_rect(Point::new(400.0, 1100.0), &windows, &[], full);
        assert_eq!(over_dock, dock);
        assert!(is_shell_strip(dock, full));
        assert!(!is_shell_strip(chrome, full));
    }

    #[test]
    fn hover_keeps_tray_over_fullscreen_desktop() {
        let full = Rect::new(0.0, 0.0, 4160.0, 1400.0);
        let desktop = full;
        let tray = Rect::new(3920.0, 1320.0, 240.0, 80.0);
        let taskbar = Rect::new(0.0, 1352.0, 4160.0, 48.0);
        let windows = [desktop, taskbar, tray];
        let hit = hover_rect(Point::new(4000.0, 1360.0), &windows, &[], full);
        assert_eq!(hit, tray);
        assert!(is_desktop_backdrop(desktop, full));
        assert!(is_shell_strip(taskbar, full));
        assert!(!is_desktop_backdrop(Rect::new(0.0, 0.0, 1920.0, 1063.0), full));
    }

    #[test]
    fn hover_prefers_taskbar_over_maximized_app_frame() {
        let full = Rect::new(0.0, 0.0, 4160.0, 1400.0);
        let devenv = Rect::new(0.0, 0.0, 4160.0, 1400.0);
        let taskbar = Rect::new(0.0, 1352.0, 4160.0, 48.0);
        let tray = Rect::new(3920.0, 1320.0, 240.0, 80.0);
        let windows = [devenv, taskbar, tray];
        assert_eq!(
            hover_rect(Point::new(2000.0, 1370.0), &windows, &[], full),
            taskbar
        );
        assert_eq!(
            hover_rect(Point::new(4000.0, 1360.0), &windows, &[], full),
            tray
        );
        assert_eq!(
            hover_rect(Point::new(800.0, 400.0), &windows, &[], full),
            devenv
        );
    }

    #[test]
    fn clamp_keeps_rect_inside() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 80.0);
        let r = Rect::new(90.0, 70.0, 30.0, 20.0).clamp_inside(bounds);
        assert_eq!(r, Rect::new(70.0, 60.0, 30.0, 20.0));
    }

    #[test]
    fn phys_to_logical_scales_by_dpr() {
        let screens = [(0.0, 0.0, 1280.0, 720.0, 1.5)];
        let (x, y, w, h) = phys_to_logical(0.0, 0.0, 1920.0, 1080.0, &screens);
        assert_eq!((x, y, w, h), (0, 0, 1280, 720));
    }

    #[test]
    fn constrain_square_is_isotropic() {
        let p = constrain_square(Point::new(10.0, 10.0), Point::new(40.0, 20.0));
        assert_eq!(p, Point::new(40.0, 40.0));
    }

    #[test]
    fn polyline_distance() {
        let pts = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        assert!((dist_to_polyline(Point::new(5.0, 2.0), &pts) - 2.0).abs() < 1e-5);
    }

    #[test]
    fn inflated_grows() {
        let r = Rect::new(10.0, 10.0, 20.0, 20.0).inflated(5.0);
        assert_eq!(r, Rect::new(5.0, 5.0, 30.0, 30.0));
    }

    #[test]
    fn subtract_center_hole() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(20.0, 20.0, 60.0, 60.0);
        let parts = a.subtract(b);
        let area: f32 = parts.iter().map(|r| r.area()).sum();
        assert!((area - 6400.0).abs() < 0.5);
        assert!(parts.iter().all(|r| !r.intersects(b) || r.is_empty() || {
            // bands share the hole edge but have zero interior overlap
            r.intersect(b).area() == 0.0
        }));
    }

    #[test]
    fn subtract_disjoint_keeps_self() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(50.0, 50.0, 10.0, 10.0);
        let parts = a.subtract(b);
        assert_eq!(parts[0], a);
        assert!(parts[1].is_empty() && parts[2].is_empty() && parts[3].is_empty());
    }

    #[test]
    fn clamp_f32_inverted_range_does_not_panic() {
        assert_eq!(clamp_f32(5.0, 0.0, -12.0), 0.0);
        assert_eq!(clamp_f32(-3.0, 0.0, -12.0), -3.0);
        assert_eq!(clamp_f32(8.0, 10.0, 20.0), 10.0);
        let r = Rect::new(0.0, 0.0, 40.0, 20.0).clamp_inside(Rect::new(0.0, 0.0, 30.0, 20.0));
        assert!(r.w <= 30.0);
    }
}

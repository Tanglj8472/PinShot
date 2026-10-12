//! Deduplicate one physical wheel notch that X11/winit can deliver twice.
//!
//! On Linux, a single click-wheel step often arrives as both a core Button4/5
//! event and an XI2 `MouseWheel` (or two `LineDelta`s a few milliseconds
//! apart). Python/Qt emits one `angleDelta = ±120` per notch. This gate keeps
//! the first same-sign event in a short window and normalizes it to ±120.

use std::time::{Duration, Instant};

/// Drop a second same-sign wheel event that follows within this window.
pub const WHEEL_DEDUPE: Duration = Duration::from_millis(40);

#[derive(Clone, Debug, Default)]
pub struct WheelGate {
    last_at: Option<Instant>,
    last_sign: i8,
}

impl WheelGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convert a raw winit-style angle (line×120 or pixel Δy) into one Qt
    /// notch (`±120`) or `None` if this is a duplicate of a recent notch.
    pub fn take_at(&mut self, angle: i32, now: Instant) -> Option<i32> {
        if angle == 0 {
            return None;
        }
        let sign = if angle > 0 { 1i8 } else { -1 };
        if let Some(prev) = self.last_at {
            if sign == self.last_sign && now.saturating_duration_since(prev) < WHEEL_DEDUPE {
                return None;
            }
        }
        self.last_at = Some(now);
        self.last_sign = sign;
        Some(sign as i32 * 120)
    }

    pub fn take(&mut self, angle: i32) -> Option<i32> {
        self.take_at(angle, Instant::now())
    }
}

/// `MouseScrollDelta::LineDelta(_, y) => y * 120`, `PixelDelta` uses `y` as-is.
pub fn angle_from_scroll(line_y: f32, pixel: bool) -> i32 {
    if pixel {
        line_y as i32
    } else {
        (line_y * 120.0) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::{Annotator, Tool};
    use crate::geom::Point;

    #[test]
    fn duplicate_within_10ms_is_dropped() {
        let mut g = WheelGate::new();
        let t0 = Instant::now();
        assert_eq!(g.take_at(120, t0), Some(120));
        assert_eq!(g.take_at(120, t0 + Duration::from_millis(10)), None);
        assert_eq!(g.take_at(240, t0 + Duration::from_millis(12)), None);
    }

    #[test]
    fn notch_after_50ms_is_kept() {
        let mut g = WheelGate::new();
        let t0 = Instant::now();
        assert_eq!(g.take_at(120, t0), Some(120));
        assert_eq!(g.take_at(120, t0 + Duration::from_millis(50)), Some(120));
    }

    #[test]
    fn sign_flip_is_kept_immediately() {
        let mut g = WheelGate::new();
        let t0 = Instant::now();
        assert_eq!(g.take_at(120, t0), Some(120));
        assert_eq!(g.take_at(-120, t0 + Duration::from_millis(5)), Some(-120));
    }

    #[test]
    fn zero_angle_is_ignored() {
        let mut g = WheelGate::new();
        assert_eq!(g.take_at(0, Instant::now()), None);
    }

    #[test]
    fn line_delta_of_two_normalizes_to_one_notch() {
        let mut g = WheelGate::new();
        let raw = angle_from_scroll(2.0, false);
        assert_eq!(raw, 240);
        assert_eq!(g.take_at(raw, Instant::now()), Some(120));
    }

    #[test]
    fn three_spaced_notches_are_three_width_steps() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        assert!(a.press(Point::new(10.0, 10.0), false));
        a.move_to(Point::new(40.0, 40.0), false);
        a.release(Point::new(40.0, 40.0));
        assert_eq!(a.shapes.len(), 1);
        assert_eq!(a.shapes[0].style.width, 3);

        let mut g = WheelGate::new();
        let t0 = Instant::now();
        for i in 0..3 {
            let t = t0 + Duration::from_millis(i * 80);
            // X11 double-delivery: two +120 events a few ms apart.
            if let Some(ang) = g.take_at(120, t) {
                assert!(a.wheel(ang));
            }
            assert!(g.take_at(120, t + Duration::from_millis(8)).is_none());
        }
        assert_eq!(
            a.shapes[0].style.width, 6,
            "3 notches must add 3, not 6 (3→6 not 3→9)"
        );
    }
}

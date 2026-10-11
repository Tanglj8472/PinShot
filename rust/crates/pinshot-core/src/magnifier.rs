//! Magnifier layout + colour sampling (15×15 @ 8×, RGB/HEX).

use crate::color::Rgba;
use crate::geom::{Point, Rect};
use crate::raster::Raster;

pub const MAG_N: i32 = 15;
pub const MAG_ZOOM: i32 = 8;
pub const MAG_INFO_H: f32 = 66.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Magnifier {
    pub grid: Rect,
    pub info: Rect,
    pub color: Rgba,
    pub phys: Point,
}

pub fn magnifier_visible(
    forced: bool,
    tool_active: bool,
    confirm_open: bool,
    over_toolbar: bool,
    creating_or_resizing: bool,
    idle_without_sel: bool,
) -> bool {
    if forced {
        return true;
    }
    if tool_active || confirm_open || over_toolbar {
        return false;
    }
    creating_or_resizing || idle_without_sel
}

/// Place the 15×15 ×8 grid next to the cursor, flipping when it would clip.
pub fn layout_magnifier(cursor: Point, overlay: Rect, color: Rgba, phys: Point) -> Magnifier {
    let size = (MAG_N * MAG_ZOOM) as f32;
    let mut x = cursor.x + 22.0;
    let mut y = cursor.y + 22.0;
    if x + size > overlay.right() {
        x = cursor.x - 22.0 - size;
    }
    if y + size + MAG_INFO_H > overlay.bottom() {
        y = cursor.y - 22.0 - size - MAG_INFO_H;
    }
    Magnifier {
        grid: Rect::new(x, y, size, size),
        info: Rect::new(x, y + size, size, MAG_INFO_H),
        color,
        phys,
    }
}

pub fn sample_color(img: &Raster, logical: Point, dpr: f32) -> (Rgba, Point) {
    let x = (logical.x * dpr).round() as i32;
    let y = (logical.y * dpr).round() as i32;
    let c = img.get_clamped(x, y);
    (c, Point::from_i32(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_rules() {
        assert!(magnifier_visible(true, true, false, false, false, false));
        assert!(!magnifier_visible(false, true, false, false, true, false));
        assert!(magnifier_visible(false, false, false, false, true, false));
        assert!(magnifier_visible(false, false, false, false, false, true));
        assert!(!magnifier_visible(false, false, false, true, true, false));
    }

    #[test]
    fn flips_near_edge() {
        let overlay = Rect::new(0.0, 0.0, 200.0, 200.0);
        let mag = layout_magnifier(Point::new(190.0, 190.0), overlay, Rgba::RED, Point::ZERO);
        assert!(mag.grid.right() <= 200.0 + 1.0);
        assert!(mag.info.bottom() <= 200.0 + 1.0);
    }

    #[test]
    fn samples_pixel() {
        let mut img = Raster::new(10, 10);
        img.set(3, 4, Rgba::BLUE);
        let (c, p) = sample_color(&img, Point::new(3.0, 4.0), 1.0);
        assert_eq!(c, Rgba::BLUE);
        assert_eq!(p, Point::new(3.0, 4.0));
    }
}

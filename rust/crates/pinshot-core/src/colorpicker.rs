//! In-process HSV + palette color picker (Python `QColorDialog` stand-in).
//!
//! Lives in the overlay / pin so Linux never needs zenity and the X11 grab
//! stays on the freeze.

use crate::color::{hsv_to_rgb, palette_colors, rgb_to_hsv, Rgba};
use crate::geom::{Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerHit {
    Sv,
    Hue,
    Palette(usize),
    Ok,
    Cancel,
    Frame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerDrag {
    Sv,
    Hue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerAction {
    Continue,
    Accept,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorPickerLayout {
    pub frame: Rect,
    pub sv: Rect,
    pub hue: Rect,
    pub preview: Rect,
    pub initial: Rect,
    pub palette: [Rect; 20],
    pub ok: Rect,
    pub cancel: Rect,
}

pub const PICKER_W: f32 = 280.0;
pub const PICKER_H: f32 = 248.0;

pub fn color_picker_layout(host_w: f32, host_h: f32) -> ColorPickerLayout {
    let w = PICKER_W.min(host_w.max(180.0));
    let h = PICKER_H.min(host_h.max(140.0));
    let x = ((host_w - w) / 2.0).max(0.0);
    let y = ((host_h - h) / 2.0).max(0.0);
    let frame = Rect::new(x, y, w, h);
    let sv_s = 128.0_f32.min(w - 148.0).max(48.0);
    let sv = Rect::new(x + 14.0, y + 32.0, sv_s, sv_s);
    let hue = Rect::new(sv.right() + 8.0, sv.y, 16.0, sv_s);
    let preview = Rect::new(hue.right() + 10.0, sv.y, 72.0, 28.0);
    let initial = Rect::new(preview.x, preview.bottom() + 6.0, 72.0, 20.0);
    let pal_y = (sv.bottom() + 12.0).min(y + h - 52.0);
    let cell = ((w - 28.0) / 10.0).min(24.0).max(12.0);
    let mut palette = [Rect::EMPTY; 20];
    for i in 0..20 {
        let col = (i % 10) as f32;
        let row = (i / 10) as f32;
        palette[i] = Rect::new(x + 14.0 + col * cell, pal_y + row * 18.0, cell - 2.0, 16.0);
    }
    ColorPickerLayout {
        frame,
        sv,
        hue,
        preview,
        initial,
        palette,
        ok: Rect::new(x + w - 168.0, y + h - 36.0, 70.0, 26.0),
        cancel: Rect::new(x + w - 88.0, y + h - 36.0, 70.0, 26.0),
    }
}

impl ColorPickerLayout {
    pub fn hit(&self, p: Point) -> Option<PickerHit> {
        if self.ok.contains(p) {
            Some(PickerHit::Ok)
        } else if self.cancel.contains(p) {
            Some(PickerHit::Cancel)
        } else if let Some(i) = self.palette.iter().position(|r| r.contains(p)) {
            Some(PickerHit::Palette(i))
        } else if self.sv.contains(p) {
            Some(PickerHit::Sv)
        } else if self.hue.contains(p) {
            Some(PickerHit::Hue)
        } else if self.frame.contains(p) {
            Some(PickerHit::Frame)
        } else {
            None
        }
    }

    pub fn contains(&self, p: Point) -> bool {
        self.frame.contains(p)
    }
}

#[derive(Clone, Debug)]
pub struct ColorPicker {
    pub current: Rgba,
    pub initial: Rgba,
    pub hsv: (f32, f32, f32),
    pub drag: Option<PickerDrag>,
}

impl ColorPicker {
    pub fn new(current: Rgba) -> Self {
        Self {
            current,
            initial: current,
            hsv: rgb_to_hsv(current),
            drag: None,
        }
    }

    pub fn set_rgb(&mut self, c: Rgba) {
        self.current = c;
        self.hsv = rgb_to_hsv(c);
    }

    pub fn press(&mut self, hit: PickerHit, p: Point, lay: &ColorPickerLayout) -> PickerAction {
        match hit {
            PickerHit::Ok => PickerAction::Accept,
            PickerHit::Cancel => PickerAction::Cancel,
            PickerHit::Palette(i) => {
                if let Some(&c) = palette_colors().get(i) {
                    self.set_rgb(c);
                }
                PickerAction::Continue
            }
            PickerHit::Sv => {
                self.drag = Some(PickerDrag::Sv);
                self.sample_sv(p, lay);
                PickerAction::Continue
            }
            PickerHit::Hue => {
                self.drag = Some(PickerDrag::Hue);
                self.sample_hue(p, lay);
                PickerAction::Continue
            }
            PickerHit::Frame => PickerAction::Continue,
        }
    }

    pub fn drag_to(&mut self, p: Point, lay: &ColorPickerLayout) {
        match self.drag {
            Some(PickerDrag::Sv) => self.sample_sv(p, lay),
            Some(PickerDrag::Hue) => self.sample_hue(p, lay),
            None => {}
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    fn sample_sv(&mut self, p: Point, lay: &ColorPickerLayout) {
        let s = ((p.x - lay.sv.x) / lay.sv.w.max(1.0)).clamp(0.0, 1.0);
        let v = 1.0 - ((p.y - lay.sv.y) / lay.sv.h.max(1.0)).clamp(0.0, 1.0);
        self.hsv.1 = s;
        self.hsv.2 = v;
        self.current = hsv_to_rgb(self.hsv.0, s, v);
    }

    fn sample_hue(&mut self, p: Point, lay: &ColorPickerLayout) {
        let t = ((p.y - lay.hue.y) / lay.hue.h.max(1.0)).clamp(0.0, 1.0);
        self.hsv.0 = t * 359.9;
        self.current = hsv_to_rgb(self.hsv.0, self.hsv.1, self.hsv.2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_hits_ok_cancel_sv_palette() {
        let lay = color_picker_layout(1920.0, 1200.0);
        assert!((lay.frame.x - (1920.0 - PICKER_W) / 2.0).abs() < 0.5);
        assert_eq!(
            lay.hit(Point::new(lay.ok.x + 4.0, lay.ok.y + 4.0)),
            Some(PickerHit::Ok)
        );
        assert_eq!(
            lay.hit(Point::new(lay.cancel.x + 4.0, lay.cancel.y + 4.0)),
            Some(PickerHit::Cancel)
        );
        assert_eq!(
            lay.hit(Point::new(lay.sv.x + 8.0, lay.sv.y + 8.0)),
            Some(PickerHit::Sv)
        );
        assert_eq!(
            lay.hit(Point::new(lay.palette[3].x + 2.0, lay.palette[3].y + 2.0)),
            Some(PickerHit::Palette(3))
        );
        assert_eq!(lay.hit(Point::new(0.0, 0.0)), None);
    }

    #[test]
    fn palette_and_sv_update_current() {
        let lay = color_picker_layout(800.0, 600.0);
        let mut p = ColorPicker::new(Rgba::RED);
        assert_eq!(
            p.press(PickerHit::Palette(6), Point::ZERO, &lay),
            PickerAction::Continue
        );
        assert_eq!(p.current, palette_colors()[6]);
        let sv = Point::new(lay.sv.x + lay.sv.w * 0.5, lay.sv.y + lay.sv.h * 0.25);
        assert_eq!(p.press(PickerHit::Sv, sv, &lay), PickerAction::Continue);
        assert!((p.hsv.1 - 0.5).abs() < 0.08);
        assert!((p.hsv.2 - 0.75).abs() < 0.08);
        assert_eq!(p.press(PickerHit::Ok, Point::ZERO, &lay), PickerAction::Accept);
        assert_eq!(p.press(PickerHit::Cancel, Point::ZERO, &lay), PickerAction::Cancel);
    }
}

//! In-overlay Esc confirm (Python `ConfirmAbort`), so a native dialog does
//! not steal the X11 grab or hide the freeze.

use crate::geom::{Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmAbortHit {
    Yes,
    No,
    Dont,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfirmAbortLayout {
    pub frame: Rect,
    pub yes: Rect,
    pub no: Rect,
    pub dont: Rect,
}

pub const CONFIRM_W: f32 = 300.0;
pub const CONFIRM_H: f32 = 128.0;

pub fn confirm_abort_layout(overlay_w: f32, overlay_h: f32) -> ConfirmAbortLayout {
    let w = CONFIRM_W.min(overlay_w.max(160.0));
    let h = CONFIRM_H.min(overlay_h.max(80.0));
    let x = ((overlay_w - w) / 2.0).max(0.0);
    let y = ((overlay_h - h) / 2.0).max(0.0);
    let frame = Rect::new(x, y, w, h);
    ConfirmAbortLayout {
        frame,
        dont: Rect::new(x + 16.0, y + 48.0, w - 32.0, 22.0),
        yes: Rect::new(x + w - 168.0, y + h - 40.0, 70.0, 26.0),
        no: Rect::new(x + w - 88.0, y + h - 40.0, 70.0, 26.0),
    }
}

impl ConfirmAbortLayout {
    pub fn hit(&self, p: Point) -> Option<ConfirmAbortHit> {
        // Inflate the painted chips so a click on the 是/否 label still counts.
        if self.yes.inflated(8.0).contains(p) {
            Some(ConfirmAbortHit::Yes)
        } else if self.no.inflated(8.0).contains(p) {
            Some(ConfirmAbortHit::No)
        } else if self.dont.inflated(4.0).contains(p) {
            Some(ConfirmAbortHit::Dont)
        } else {
            None
        }
    }

    pub fn contains(&self, p: Point) -> bool {
        self.frame.contains(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_and_hits_yes_no_dont() {
        let lay = confirm_abort_layout(1920.0, 1200.0);
        assert!((lay.frame.x - (1920.0 - CONFIRM_W) / 2.0).abs() < 0.5);
        assert!((lay.frame.y - (1200.0 - CONFIRM_H) / 2.0).abs() < 0.5);
        let yc = Point::new(lay.yes.x + 4.0, lay.yes.y + 4.0);
        let nc = Point::new(lay.no.x + 4.0, lay.no.y + 4.0);
        let dc = Point::new(lay.dont.x + 4.0, lay.dont.y + 4.0);
        assert_eq!(lay.hit(yc), Some(ConfirmAbortHit::Yes));
        assert_eq!(lay.hit(nc), Some(ConfirmAbortHit::No));
        assert_eq!(lay.hit(dc), Some(ConfirmAbortHit::Dont));
        // Click on the 是 glyph / just outside the chip still hits.
        let yes_label = Point::new(lay.yes.x + 24.0, lay.yes.y + 12.0);
        assert_eq!(lay.hit(yes_label), Some(ConfirmAbortHit::Yes));
        let yes_edge = Point::new(lay.yes.x - 4.0, lay.yes.y + 8.0);
        assert_eq!(lay.hit(yes_edge), Some(ConfirmAbortHit::Yes));
        assert_eq!(lay.hit(Point::new(0.0, 0.0)), None);
    }
}

//! Overlay selection state machine (idle → creating → selected + move/resize).

use crate::geom::{hit_handle, Handle, Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelState {
    Idle,
    Creating,
    Moving,
    Resizing,
    Selected,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    pub rect: Rect,
    pub state: SelState,
    press: Option<Point>,
    drag_off: Point,
    handle: Option<Handle>,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            rect: Rect::EMPTY,
            state: SelState::Idle,
            press: None,
            drag_off: Point::ZERO,
            handle: None,
        }
    }
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_sel(&self) -> bool {
        self.rect.is_valid_sel()
    }

    pub fn normalized(&self) -> Rect {
        self.rect.normalized()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Left press. `tool_active` means the annotator should own the click.
    pub fn press(
        &mut self,
        pt: Point,
        bounds: Rect,
        tool_active: bool,
    ) -> PressKind {
        if self.has_sel() && !tool_active {
            if let Some(h) = hit_handle(self.normalized(), pt, 8.0) {
                self.state = SelState::Resizing;
                self.handle = Some(h);
                self.rect = self.normalized();
                return PressKind::Resize(h);
            }
            if self.normalized().contains(pt) {
                self.state = SelState::Moving;
                let tl = self.normalized().top_left();
                self.drag_off = pt.sub(tl);
                self.rect = self.normalized();
                return PressKind::Move;
            }
            // Click outside an existing selection: keep it (Snipaste-like).
            return PressKind::Ignore;
        }
        if self.has_sel() {
            return PressKind::Ignore;
        }
        self.state = SelState::Creating;
        self.press = Some(pt.clamp_to(bounds));
        self.rect = Rect::EMPTY;
        PressKind::Create
    }

    pub fn motion(&mut self, pt: Point, bounds: Rect) {
        match self.state {
            SelState::Creating => {
                if let Some(origin) = self.press {
                    if pt.manhattan(origin) > 3.0 {
                        self.rect = Rect::span(origin, pt);
                    }
                }
            }
            SelState::Moving => {
                let mut r = self.rect;
                r = Rect::new(
                    pt.x - self.drag_off.x,
                    pt.y - self.drag_off.y,
                    r.w,
                    r.h,
                )
                .clamp_inside(bounds);
                self.rect = r;
            }
            SelState::Resizing => {
                if let Some(h) = self.handle {
                    self.rect = crate::geom::resize_rect(self.rect, h, pt);
                }
            }
            _ => {}
        }
    }

    /// Finish a drag. If the drag was a click, adopt `hover` (window auto-detect).
    pub fn release(&mut self, pt: Point, hover: Rect, bounds: Rect) {
        if self.state == SelState::Creating {
            if let Some(origin) = self.press {
                self.rect = Rect::span(origin, pt);
            }
            if !self.has_sel() {
                self.rect = if hover.is_empty() { bounds } else { hover };
            }
        }
        if matches!(
            self.state,
            SelState::Creating | SelState::Moving | SelState::Resizing
        ) {
            self.rect = self.rect.normalized().intersect(bounds);
            self.state = SelState::Selected;
            self.handle = None;
            self.press = None;
        }
    }

    pub fn set_rect(&mut self, r: Rect, bounds: Rect) {
        self.rect = r.intersect(bounds);
        self.state = SelState::Selected;
    }

    pub fn nudge(&mut self, dx: f32, dy: f32, mode: Nudge, bounds: Rect) {
        let mut r = self.rect.normalized();
        if r.is_empty() {
            return;
        }
        match mode {
            Nudge::Move => r = r.translated(dx, dy),
            Nudge::Grow => {
                if dx < 0.0 {
                    r.x -= 1.0;
                    r.w += 1.0;
                }
                if dx > 0.0 {
                    r.w += 1.0;
                }
                if dy < 0.0 {
                    r.y -= 1.0;
                    r.h += 1.0;
                }
                if dy > 0.0 {
                    r.h += 1.0;
                }
            }
            Nudge::Shrink => {
                if dx < 0.0 && r.w > 1.0 {
                    r.w -= 1.0;
                }
                if dx > 0.0 && r.w > 1.0 {
                    r.x += 1.0;
                    r.w -= 1.0;
                }
                if dy < 0.0 && r.h > 1.0 {
                    r.h -= 1.0;
                }
                if dy > 0.0 && r.h > 1.0 {
                    r.y += 1.0;
                    r.h -= 1.0;
                }
            }
        }
        self.rect = r.intersect(bounds);
        if self.state == SelState::Idle {
            self.state = SelState::Selected;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nudge {
    Move,
    Grow,
    Shrink,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressKind {
    Create,
    Move,
    Resize(Handle),
    Ignore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectMode {
    Window,
    Element,
}

impl DetectMode {
    pub fn toggle(self) -> Self {
        match self {
            DetectMode::Window => DetectMode::Element,
            DetectMode::Element => DetectMode::Window,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            DetectMode::Window => "window",
            DetectMode::Element => "element",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Rect {
        Rect::new(0.0, 0.0, 1920.0, 1080.0)
    }

    #[test]
    fn drag_creates_rect() {
        let mut s = Selection::new();
        assert_eq!(s.press(Point::new(10.0, 10.0), bounds(), false), PressKind::Create);
        s.motion(Point::new(80.0, 50.0), bounds());
        s.release(Point::new(80.0, 50.0), Rect::EMPTY, bounds());
        assert_eq!(s.state, SelState::Selected);
        assert_eq!(s.normalized(), Rect::new(10.0, 10.0, 70.0, 40.0));
    }

    #[test]
    fn click_adopts_hover_window() {
        let mut s = Selection::new();
        let hover = Rect::new(100.0, 80.0, 400.0, 300.0);
        s.press(Point::new(120.0, 100.0), bounds(), false);
        s.release(Point::new(121.0, 101.0), hover, bounds());
        assert_eq!(s.normalized(), hover);
    }

    #[test]
    fn move_and_resize() {
        let mut s = Selection::new();
        s.set_rect(Rect::new(10.0, 10.0, 40.0, 20.0), bounds());
        assert_eq!(
            s.press(Point::new(20.0, 15.0), bounds(), false),
            PressKind::Move
        );
        s.motion(Point::new(40.0, 25.0), bounds());
        s.release(Point::new(40.0, 25.0), Rect::EMPTY, bounds());
        assert_eq!(s.normalized().top_left(), Point::new(30.0, 20.0));

        let br = s.normalized().bottom_right();
        s.press(Point::new(br.x + 1.0, br.y + 1.0), bounds(), false);
        assert_eq!(s.state, SelState::Resizing);
        s.motion(Point::new(br.x + 20.0, br.y + 10.0), bounds());
        s.release(Point::new(br.x + 20.0, br.y + 10.0), Rect::EMPTY, bounds());
        assert!(s.normalized().w > 40.0);
    }

    #[test]
    fn outside_click_keeps_selection() {
        let mut s = Selection::new();
        s.set_rect(Rect::new(10.0, 10.0, 40.0, 20.0), bounds());
        assert_eq!(
            s.press(Point::new(400.0, 400.0), bounds(), false),
            PressKind::Ignore
        );
        assert!(s.has_sel());
    }

    #[test]
    fn nudge_grow_shrink() {
        let mut s = Selection::new();
        s.set_rect(Rect::new(10.0, 10.0, 20.0, 20.0), bounds());
        s.nudge(1.0, 0.0, Nudge::Grow, bounds());
        assert_eq!(s.normalized().w, 21.0);
        s.nudge(1.0, 0.0, Nudge::Shrink, bounds());
        assert_eq!(s.normalized().w, 20.0);
        // Shrink +x pulls the left edge in (Python overlay `nudge` shrink).
        assert_eq!(s.normalized().x, 11.0);
        s.nudge(5.0, 0.0, Nudge::Move, bounds());
        assert_eq!(s.normalized().x, 16.0);
    }
}

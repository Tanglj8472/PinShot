//! Self-drawn annotation toolbar + colour/width sub-panel (hit-testing only).

use pinshot_core::color::{palette_colors, Rgba};
use pinshot_core::geom::{Point, Rect};
use pinshot_core::icons::IconKind;
use pinshot_core::annotation::Tool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TbAction {
    Tool(Tool),
    Arrow,
    Undo,
    Redo,
    Close,
    Pin,
    Save,
    Copy,
    Width(i32),
    Color(u8),
}

#[derive(Clone, Copy, Debug)]
pub struct TbButton {
    pub rect: Rect,
    pub icon: IconKind,
    pub action: TbAction,
    pub tip: &'static str,
    pub checked: bool,
}

#[derive(Clone, Debug)]
pub struct Toolbar {
    pub bar: Rect,
    pub panel: Option<Rect>,
    pub buttons: Vec<TbButton>,
    pub swatches: Vec<(Rect, Rgba, u8)>,
    pub visible: bool,
}

const CELL: f32 = 26.0;
const PAD: f32 = 4.0;

impl Toolbar {
    pub fn layout(sel: Rect, overlay: Rect, active: Option<Tool>, arrow_on: bool) -> Self {
        let tools = [
            (IconKind::Rect, TbAction::Tool(Tool::Rect), "Rectangle"),
            (IconKind::Ellipse, TbAction::Tool(Tool::Ellipse), "Ellipse"),
            (IconKind::Line, TbAction::Tool(Tool::Line), "Line"),
            (IconKind::Arrow, TbAction::Arrow, "Arrow"),
            (IconKind::Pen, TbAction::Tool(Tool::Pen), "Pencil"),
            (IconKind::Marker, TbAction::Tool(Tool::Marker), "Marker"),
            (IconKind::Mosaic, TbAction::Tool(Tool::Mosaic), "Mosaic / blur"),
            (IconKind::Text, TbAction::Tool(Tool::Text), "Text"),
            (IconKind::Number, TbAction::Tool(Tool::Number), "Numbered marker (N)"),
        ];
        let acts = [
            (IconKind::Undo, TbAction::Undo, "Undo (Ctrl+Z)"),
            (IconKind::Redo, TbAction::Redo, "Redo (Ctrl+Y)"),
            (IconKind::Close, TbAction::Close, "Cancel (Esc)"),
            (IconKind::Pin, TbAction::Pin, "Pin to screen (Ctrl+T)"),
            (IconKind::Save, TbAction::Save, "Save as… (Ctrl+S)"),
            (IconKind::Copy, TbAction::Copy, "Copy (Ctrl+C / Enter)"),
        ];
        let n = (tools.len() + 1 + acts.len()) as f32;
        let w = PAD * 2.0 + n * CELL + 8.0;
        let h = CELL + PAD * 2.0;
        let mut x = sel.right() + 1.0 - w;
        let mut y = sel.bottom() + 8.0;
        if y + h > overlay.bottom() {
            y = sel.y - h - 8.0;
        }
        if y < overlay.y {
            y = (sel.bottom() - h - 8.0).max(overlay.y);
        }
        x = x.clamp(overlay.x, overlay.right() - w);
        let bar = Rect::new(x, y, w, h);
        let mut buttons = Vec::new();
        let mut cx = x + PAD;
        for (icon, action, tip) in tools {
            let checked = match action {
                TbAction::Tool(t) => Some(t) == active && !arrow_on,
                TbAction::Arrow => active == Some(Tool::Line) && arrow_on,
                _ => false,
            };
            buttons.push(TbButton {
                rect: Rect::new(cx, y + PAD, CELL, CELL),
                icon,
                action,
                tip,
                checked,
            });
            cx += CELL;
        }
        cx += 8.0;
        for (icon, action, tip) in acts {
            buttons.push(TbButton {
                rect: Rect::new(cx, y + PAD, CELL, CELL),
                icon,
                action,
                tip,
                checked: false,
            });
            cx += CELL;
        }

        let mut panel = None;
        let mut swatches = Vec::new();
        if active.is_some() {
            let pw = 20.0 * 16.0 + 80.0;
            let ph = 40.0;
            let mut py = bar.bottom() + 3.0;
            if py + ph > overlay.bottom() {
                py = bar.y - ph - 3.0;
            }
            let pr = Rect::new(bar.x, py, pw, ph);
            panel = Some(pr);
            buttons.push(TbButton {
                rect: Rect::new(pr.x + 6.0, pr.y + 8.0, 22.0, 22.0),
                icon: IconKind::Undo,
                action: TbAction::Width(-1),
                tip: "Width",
                checked: false,
            });
            buttons.push(TbButton {
                rect: Rect::new(pr.x + 30.0, pr.y + 8.0, 22.0, 22.0),
                icon: IconKind::Redo,
                action: TbAction::Width(1),
                tip: "Width",
                checked: false,
            });
            for (i, c) in palette_colors().iter().enumerate() {
                let col = i % 10;
                let row = i / 10;
                let sr = Rect::new(pr.x + 60.0 + col as f32 * 16.0, pr.y + 6.0 + row as f32 * 16.0, 14.0, 14.0);
                swatches.push((sr, *c, i as u8));
            }
        }
        Self {
            bar,
            panel,
            buttons,
            swatches,
            visible: true,
        }
    }

    pub fn contains(&self, p: Point) -> bool {
        if !self.visible {
            return false;
        }
        if self.bar.contains(p) {
            return true;
        }
        if let Some(pr) = self.panel {
            if pr.contains(p) {
                return true;
            }
        }
        false
    }

    pub fn hit(&self, p: Point) -> Option<TbAction> {
        if !self.visible {
            return None;
        }
        for (r, _, i) in &self.swatches {
            if r.contains(p) {
                return Some(TbAction::Color(*i));
            }
        }
        for b in &self.buttons {
            if b.rect.contains(p) {
                return Some(b.action);
            }
        }
        None
    }
}

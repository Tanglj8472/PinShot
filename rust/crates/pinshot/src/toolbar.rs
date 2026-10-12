//! Self-drawn annotation toolbar + colour/width/style sub-panel (hit-testing only).

use pinshot_core::annotation::{DrawMode, Effect, LineStyleKind, Style};
use pinshot_core::color::{palette_colors, Rgba};
use pinshot_core::geom::{clamp_f32, Point, Rect};
use pinshot_core::i18n::tr;
use pinshot_core::icons::IconKind;
use pinshot_core::annotation::{Annotator, Tool};

/// Clicking an active drawing tool switches back to selection/move mode.
pub fn toggle_tool(annot: &mut Annotator, arrow_on: &mut bool, action: TbAction) {
    let (tool, arrow) = match action {
        TbAction::Tool(tool) => (tool, false),
        TbAction::Arrow => (Tool::Line, true),
        _ => return,
    };
    let same = annot.tool == Some(tool) && (tool != Tool::Line || *arrow_on == arrow);
    annot.set_tool(if same { None } else { Some(tool) });
    if tool == Tool::Line && !same {
        annot.set_style_default(tool, |s| s.arrow = arrow);
    }
    *arrow_on = !same && arrow;
}

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
    CustomColor,
    Fill(bool),
    LineStyle(LineStyleKind),
    SetMode(DrawMode),
    SetEffect(Effect),
    Bold(bool),
    Italic(bool),
    Outline(bool),
    Ocr,
    Translate,
    Check,
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
pub struct TbExtra {
    pub rect: Rect,
    pub action: TbAction,
    pub label: String,
    pub checked: bool,
}

#[derive(Clone, Debug)]
pub struct Toolbar {
    pub bar: Rect,
    pub panel: Option<Rect>,
    pub buttons: Vec<TbButton>,
    pub extras: Vec<TbExtra>,
    pub swatches: Vec<(Rect, Rgba, u8)>,
    pub visible: bool,
    pub width_minus: Rect,
    pub width_plus: Rect,
    pub width_label: Rect,
    pub width_value: i32,
    pub custom_color: Rect,
    pub custom_value: Rgba,
}

const CELL: f32 = 26.0;
const PAD: f32 = 4.0;

impl Toolbar {
    pub fn layout(
        sel: Rect,
        overlay: Rect,
        active: Option<Tool>,
        arrow_on: bool,
        style: Option<&Style>,
    ) -> Self {
        Self::layout_ex(sel, overlay, active, arrow_on, false, style)
    }

    pub fn layout_pin(
        sel: Rect,
        overlay: Rect,
        active: Option<Tool>,
        arrow_on: bool,
        style: Option<&Style>,
    ) -> Self {
        Self::layout_ex(sel, overlay, active, arrow_on, true, style)
    }

    pub fn layout_ex(
        sel: Rect,
        overlay: Rect,
        active: Option<Tool>,
        arrow_on: bool,
        for_pin: bool,
        style: Option<&Style>,
    ) -> Self {
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
        let acts: Vec<(IconKind, TbAction, &'static str)> = if for_pin {
            vec![
                (IconKind::Undo, TbAction::Undo, "Undo (Ctrl+Z)"),
                (IconKind::Redo, TbAction::Redo, "Redo (Ctrl+Y)"),
                (IconKind::Check, TbAction::Check, "Show toolbar"),
                (IconKind::Save, TbAction::Save, "Save as… (Ctrl+S)"),
                (IconKind::Copy, TbAction::Copy, "Copy (Ctrl+C / Enter)"),
                (IconKind::Close, TbAction::Close, "Close"),
            ]
        } else {
            vec![
                (IconKind::Text, TbAction::Ocr, "Recognize text"),
                (IconKind::Copy, TbAction::Translate, "Recognize and translate"),
                (IconKind::Undo, TbAction::Undo, "Undo (Ctrl+Z)"),
                (IconKind::Redo, TbAction::Redo, "Redo (Ctrl+Y)"),
                (IconKind::Close, TbAction::Close, "Cancel (Esc)"),
                (IconKind::Pin, TbAction::Pin, "Pin to screen (Ctrl+T)"),
                (IconKind::Save, TbAction::Save, "Save as… (Ctrl+S)"),
                (IconKind::Copy, TbAction::Copy, "Copy (Ctrl+C / Enter)"),
            ]
        };
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
        x = clamp_f32(x, overlay.x, overlay.right() - w);
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
        let mut extras = Vec::new();
        let mut width_minus = Rect::EMPTY;
        let mut width_plus = Rect::EMPTY;
        let mut width_label = Rect::EMPTY;
        let mut custom_color = Rect::EMPTY;
        let custom_value = style.map(|s| s.color).unwrap_or(Rgba::RED);
        if active.is_some() {
            let has_style_row = matches!(
                active,
                Some(Tool::Rect | Tool::Ellipse | Tool::Line | Tool::Mosaic | Tool::Text | Tool::Number | Tool::Eraser)
            );
            let show_palette = !matches!(active, Some(Tool::Mosaic | Tool::Eraser | Tool::Select));
            let pw = (20.0_f32 * 16.0 + 128.0).min(overlay.w.max(128.0));
            let ph = if has_style_row { 66.0 } else { 40.0 };
            let mut py = bar.bottom() + 3.0;
            if py + ph > overlay.bottom() {
                py = bar.y - ph - 3.0;
            }
            let px = clamp_f32(bar.x, overlay.x, overlay.right() - pw);
            let pr = Rect::new(px, py, pw, ph);
            panel = Some(pr);

            let mut chip_x = pr.x + 6.0;
            let chip_y = pr.y + 6.0;
            if let Some(st) = style {
                match active {
                    Some(Tool::Rect) | Some(Tool::Ellipse) => {
                        push_chip(&mut extras, &mut chip_x, chip_y, 58.0, tr("Outline"), TbAction::Fill(false), !st.filled);
                        push_chip(&mut extras, &mut chip_x, chip_y, 52.0, tr("Filled"), TbAction::Fill(true), st.filled);
                    }
                    Some(Tool::Line) => {
                        push_chip(&mut extras, &mut chip_x, chip_y, 46.0, tr("Solid"), TbAction::LineStyle(LineStyleKind::Solid), st.line_style == LineStyleKind::Solid);
                        push_chip(&mut extras, &mut chip_x, chip_y, 46.0, tr("Dash"), TbAction::LineStyle(LineStyleKind::Dash), st.line_style == LineStyleKind::Dash);
                        push_chip(&mut extras, &mut chip_x, chip_y, 40.0, tr("Dot"), TbAction::LineStyle(LineStyleKind::Dot), st.line_style == LineStyleKind::Dot);
                    }
                    Some(Tool::Mosaic) => {
                        push_chip(&mut extras, &mut chip_x, chip_y, 50.0, tr("Brush"), TbAction::SetMode(DrawMode::Brush), st.mode == DrawMode::Brush);
                        push_chip(&mut extras, &mut chip_x, chip_y, 44.0, tr("Rect"), TbAction::SetMode(DrawMode::Rect), st.mode == DrawMode::Rect);
                        push_chip(&mut extras, &mut chip_x, chip_y, 56.0, tr("Mosaic"), TbAction::SetEffect(Effect::Mosaic), st.effect == Effect::Mosaic);
                        push_chip(&mut extras, &mut chip_x, chip_y, 44.0, tr("Blur"), TbAction::SetEffect(Effect::Blur), st.effect == Effect::Blur);
                    }
                    Some(Tool::Eraser) => {
                        push_chip(&mut extras, &mut chip_x, chip_y, 50.0, tr("Brush"), TbAction::SetMode(DrawMode::Brush), st.mode == DrawMode::Brush);
                        push_chip(&mut extras, &mut chip_x, chip_y, 44.0, tr("Rect"), TbAction::SetMode(DrawMode::Rect), st.mode == DrawMode::Rect);
                    }
                    Some(Tool::Text) => {
                        push_chip(&mut extras, &mut chip_x, chip_y, 22.0, "B".into(), TbAction::Bold(!st.bold), st.bold);
                        push_chip(&mut extras, &mut chip_x, chip_y, 22.0, "I".into(), TbAction::Italic(!st.italic), st.italic);
                        push_chip(&mut extras, &mut chip_x, chip_y, 58.0, tr("Outline"), TbAction::Outline(!st.outline), st.outline);
                    }
                    Some(Tool::Number) => {
                        push_chip(&mut extras, &mut chip_x, chip_y, 52.0, tr("Filled"), TbAction::Fill(true), st.filled);
                        push_chip(&mut extras, &mut chip_x, chip_y, 58.0, tr("Outline"), TbAction::Fill(false), !st.filled);
                    }
                    _ => {}
                }
            }

            let row_y = if has_style_row { pr.y + 36.0 } else { pr.y + 8.0 };
            width_minus = Rect::new(pr.x + 6.0, row_y, 22.0, 22.0);
            width_label = Rect::new(pr.x + 30.0, row_y, 28.0, 22.0);
            width_plus = Rect::new(pr.x + 60.0, row_y, 22.0, 22.0);
            let mut pal_x = pr.x + 88.0;
            if show_palette {
                custom_color = Rect::new(pal_x, row_y, 22.0, 22.0);
                pal_x += 26.0;
                for (i, c) in palette_colors().iter().enumerate() {
                    let col = i % 10;
                    let row = i / 10;
                    let sr = Rect::new(pal_x + col as f32 * 16.0, row_y - 2.0 + row as f32 * 16.0, 14.0, 14.0);
                    swatches.push((sr, *c, i as u8));
                }
            }
        }
        Self {
            bar,
            panel,
            buttons,
            extras,
            swatches,
            visible: true,
            width_minus,
            width_plus,
            width_label,
            width_value: 0,
            custom_color,
            custom_value,
        }
    }

    pub fn set_width_value(&mut self, w: i32) {
        self.width_value = w;
    }

    pub fn contains(&self, p: Point) -> bool {
        if !self.visible {
            return false;
        }
        if self.bar.contains(p)
            || self.width_minus.contains(p)
            || self.width_plus.contains(p)
            || self.width_label.contains(p)
            || self.custom_color.contains(p)
        {
            return true;
        }
        if self.extras.iter().any(|e| e.rect.contains(p)) {
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
        if self.width_minus.contains(p) {
            return Some(TbAction::Width(-1));
        }
        if self.width_plus.contains(p) {
            return Some(TbAction::Width(1));
        }
        if !self.custom_color.is_empty() && self.custom_color.contains(p) {
            return Some(TbAction::CustomColor);
        }
        for extra in &self.extras {
            if extra.rect.contains(p) {
                return Some(extra.action);
            }
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

fn push_chip(
    extras: &mut Vec<TbExtra>,
    x: &mut f32,
    y: f32,
    w: f32,
    label: String,
    action: TbAction,
    checked: bool,
) {
    extras.push(TbExtra {
        rect: Rect::new(*x, y, w, 22.0),
        action,
        label,
        checked,
    });
    *x += w + 4.0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinshot_core::geom::Rect;

    #[test]
    fn tool_click_toggles_and_line_arrow_switches_without_editing_shapes() {
        let mut ann = Annotator::new();
        let mut arrow = false;
        toggle_tool(&mut ann, &mut arrow, TbAction::Tool(Tool::Rect));
        assert_eq!(ann.tool, Some(Tool::Rect));
        toggle_tool(&mut ann, &mut arrow, TbAction::Tool(Tool::Rect));
        assert_eq!(ann.tool, None);
        toggle_tool(&mut ann, &mut arrow, TbAction::Tool(Tool::Line));
        toggle_tool(&mut ann, &mut arrow, TbAction::Arrow);
        assert_eq!(ann.tool, Some(Tool::Line));
        assert!(ann.style(Tool::Line).arrow);
        toggle_tool(&mut ann, &mut arrow, TbAction::Arrow);
        assert_eq!(ann.tool, None);
    }

    #[test]
    fn layout_on_tiny_pin_does_not_panic() {
        let sel = Rect::new(0.0, 0.0, 20.0, 12.0);
        let over = Rect::new(0.0, 0.0, 40.0, 20.0);
        let st = Style::for_tool(Tool::Pen);
        let tb = Toolbar::layout_pin(sel, over, Some(Tool::Pen), false, Some(&st));
        assert!(tb.bar.w > 0.0);
        assert_eq!(tb.hit(Point::new(tb.width_plus.x + 2.0, tb.width_plus.y + 2.0)), Some(TbAction::Width(1)));
    }

    #[test]
    fn rect_panel_has_fill_and_outline() {
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let st = Style::for_tool(Tool::Rect);
        let tb = Toolbar::layout(sel, over, Some(Tool::Rect), false, Some(&st));
        let fill = tb.extras.iter().find(|e| e.action == TbAction::Fill(true));
        let outline = tb.extras.iter().find(|e| e.action == TbAction::Fill(false));
        assert!(fill.is_some() && outline.is_some());
        assert!(outline.unwrap().checked);
        assert!(!fill.unwrap().checked);
        let c = fill.unwrap().rect.center();
        assert_eq!(tb.hit(c), Some(TbAction::Fill(true)));
        assert_eq!(
            tb.hit(Point::new(tb.custom_color.x + 2.0, tb.custom_color.y + 2.0)),
            Some(TbAction::CustomColor)
        );
    }

    #[test]
    fn line_panel_has_dash_dot() {
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let st = Style::for_tool(Tool::Line);
        let tb = Toolbar::layout(sel, over, Some(Tool::Line), false, Some(&st));
        assert!(tb.extras.iter().any(|e| e.action == TbAction::LineStyle(LineStyleKind::Dash)));
        assert!(tb.extras.iter().any(|e| e.action == TbAction::LineStyle(LineStyleKind::Dot)));
    }
}

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
    WidthSet(i32),
    Color(u8),
    CustomColor,
    Fill(bool),
    LineStyle(LineStyleKind),
    SetMode(DrawMode),
    SetEffect(Effect),
    Bold(bool),
    Italic(bool),
    Outline(bool),
    Family(&'static str),
    Ocr,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtraKind {
    Icon,
    Chip,
    Combo,
}

#[derive(Clone, Debug)]
pub struct TbExtra {
    pub rect: Rect,
    pub action: TbAction,
    pub label: String,
    pub checked: bool,
    pub icon: Option<IconKind>,
    pub kind: ExtraKind,
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
    pub slider: Rect,
    pub slider_min: i32,
    pub slider_max: i32,
}

/// Compact cells (Snipaste-like density). Hit-testing uses the same metrics.
const CELL: f32 = 24.0;
const PAD: f32 = 3.0;
const SEP: f32 = 8.0;

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
            (IconKind::Eraser, TbAction::Tool(Tool::Eraser), "Eraser"),
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
                (IconKind::Ocr, TbAction::Ocr, "Recognize text"),
                (IconKind::Undo, TbAction::Undo, "Undo (Ctrl+Z)"),
                (IconKind::Redo, TbAction::Redo, "Redo (Ctrl+Y)"),
                (IconKind::Close, TbAction::Close, "Cancel (Esc)"),
                (IconKind::Pin, TbAction::Pin, "Pin to screen (Ctrl+T)"),
                (IconKind::Save, TbAction::Save, "Save as… (Ctrl+S)"),
                (IconKind::Copy, TbAction::Copy, "Copy (Ctrl+C / Enter)"),
            ]
        };
        let n_tools = tools.len() as f32;
        let n_acts = acts.len() as f32;
        let w = PAD * 2.0 + n_tools * CELL + SEP + n_acts * CELL;
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
        cx += SEP;
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
        let width_minus = Rect::EMPTY;
        let width_plus = Rect::EMPTY;
        let mut width_label = Rect::EMPTY;
        let mut custom_color = Rect::EMPTY;
        let mut slider = Rect::EMPTY;
        let slider_min = 1;
        let mut slider_max = 30;
        let custom_value = style.map(|s| s.color).unwrap_or(Rgba::RED);
        if active.is_some() {
            // Measure a hugging row, then place it under (or above) the bar.
            let ph = 32.0;
            let mut py = bar.bottom() + 2.0;
            if py + ph > overlay.bottom() {
                py = bar.y - ph - 2.0;
            }
            let chip_y = py + 4.0;
            let mut chip_x = 6.0;
            let show_width = matches!(
                active,
                Some(Tool::Rect | Tool::Ellipse | Tool::Line | Tool::Pen | Tool::Marker | Tool::Mosaic | Tool::Eraser | Tool::Number)
            );
            let numeric_width = matches!(active, Some(Tool::Mosaic | Tool::Eraser));
            if show_width {
                let ww = if numeric_width { 28.0 } else { 26.0 };
                width_label = Rect::new(chip_x, chip_y, ww, 24.0);
                chip_x += ww + 4.0;
            }
            if let Some(st) = style {
                match active {
                    Some(Tool::Rect) | Some(Tool::Ellipse) => {
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Fill, TbAction::Fill(true), st.filled);
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Outline, TbAction::Fill(false), !st.filled);
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Line, TbAction::LineStyle(LineStyleKind::Solid), st.line_style == LineStyleKind::Solid);
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Dash, TbAction::LineStyle(LineStyleKind::Dash), st.line_style == LineStyleKind::Dash);
                    }
                    Some(Tool::Line) => {
                        let icon = match st.line_style {
                            LineStyleKind::Solid => IconKind::Line,
                            LineStyleKind::Dash => IconKind::Dash,
                            LineStyleKind::Dot => IconKind::Dot,
                        };
                        let next = match st.line_style {
                            LineStyleKind::Solid => LineStyleKind::Dash,
                            LineStyleKind::Dash => LineStyleKind::Dot,
                            LineStyleKind::Dot => LineStyleKind::Solid,
                        };
                        push_combo(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            56.0,
                            String::new(),
                            Some(icon),
                            TbAction::LineStyle(next),
                        );
                    }
                    Some(Tool::Mosaic) => {
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Mosaic, TbAction::SetEffect(Effect::Mosaic), st.effect == Effect::Mosaic);
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Blur, TbAction::SetEffect(Effect::Blur), st.effect == Effect::Blur);
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Pen, TbAction::SetMode(DrawMode::Brush), st.mode == DrawMode::Brush);
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Rect, TbAction::SetMode(DrawMode::Rect), st.mode == DrawMode::Rect);
                        slider = Rect::new(chip_x, chip_y + 6.0, 72.0, 12.0);
                        slider_max = 30;
                        chip_x += 80.0;
                    }
                    Some(Tool::Eraser) => {
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Pen, TbAction::SetMode(DrawMode::Brush), st.mode == DrawMode::Brush);
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Rect, TbAction::SetMode(DrawMode::Rect), st.mode == DrawMode::Rect);
                    }
                    Some(Tool::Text) => {
                        push_chip(&mut extras, &mut chip_x, chip_y, 22.0, "B".into(), TbAction::Bold(!st.bold), st.bold);
                        push_chip(&mut extras, &mut chip_x, chip_y, 22.0, "I".into(), TbAction::Italic(!st.italic), st.italic);
                        push_chip(&mut extras, &mut chip_x, chip_y, 22.0, "A".into(), TbAction::Outline(!st.outline), st.outline);
                        let presets = pinshot_core::font::FAMILY_PRESETS;
                        let idx = presets
                            .iter()
                            .position(|(id, _)| {
                                if id.is_empty() {
                                    st.family.is_empty()
                                } else {
                                    st.family.eq_ignore_ascii_case(id)
                                }
                            })
                            .unwrap_or(0);
                        let label = format!("{}  {}", tr(presets[idx].1), st.font_size.max(8));
                        let next = presets[(idx + 1) % presets.len()].0;
                        push_combo(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            118.0,
                            label,
                            None,
                            TbAction::Family(next),
                        );
                    }
                    Some(Tool::Number) => {
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Fill, TbAction::Fill(true), st.filled);
                        push_icon(&mut extras, &mut chip_x, chip_y, IconKind::Outline, TbAction::Fill(false), !st.filled);
                    }
                    _ => {}
                }
            }
            let show_palette = !matches!(active, Some(Tool::Mosaic | Tool::Eraser | Tool::Select));
            if show_palette {
                custom_color = Rect::new(chip_x, chip_y, 22.0, 24.0);
                chip_x += 26.0;
                let pal_x = chip_x;
                let pal_y = py + 3.0;
                for (i, c) in palette_colors().iter().enumerate() {
                    let col = i % 10;
                    let row = i / 10;
                    let sr = Rect::new(
                        pal_x + col as f32 * 14.0,
                        pal_y + row as f32 * 13.0,
                        12.0,
                        12.0,
                    );
                    swatches.push((sr, *c, i as u8));
                }
                chip_x += 10.0 * 14.0;
            }
            let pw = (chip_x + 6.0).clamp(32.0, overlay.w.max(32.0));
            let px = clamp_f32(bar.x, overlay.x, overlay.right() - pw);
            let dx = px - 0.0;
            let shift = |r: &mut Rect| {
                if !r.is_empty() {
                    r.x += dx;
                }
            };
            shift(&mut width_label);
            shift(&mut custom_color);
            shift(&mut slider);
            for extra in &mut extras {
                extra.rect.x += dx;
            }
            for (sr, _, _) in &mut swatches {
                sr.x += dx;
            }
            panel = Some(Rect::new(px, py, pw, ph));
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
            slider,
            slider_min,
            slider_max,
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
            || self.slider.contains(p)
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
        if !self.slider.is_empty() && self.slider.contains(p) {
            let t = ((p.x - self.slider.x) / self.slider.w.max(1.0)).clamp(0.0, 1.0);
            let span = (self.slider_max - self.slider_min) as f32;
            let v = self.slider_min + (t * span).round() as i32;
            return Some(TbAction::WidthSet(v));
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

    pub fn button_center(&self, want: TbAction) -> Option<Point> {
        self.buttons
            .iter()
            .find(|b| b.action == want)
            .map(|b| b.rect.center())
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
        rect: Rect::new(*x, y, w, 24.0),
        action,
        label,
        checked,
        icon: None,
        kind: ExtraKind::Chip,
    });
    *x += w + 2.0;
}

fn push_icon(
    extras: &mut Vec<TbExtra>,
    x: &mut f32,
    y: f32,
    icon: IconKind,
    action: TbAction,
    checked: bool,
) {
    extras.push(TbExtra {
        rect: Rect::new(*x, y, 24.0, 24.0),
        action,
        label: String::new(),
        checked,
        icon: Some(icon),
        kind: ExtraKind::Icon,
    });
    *x += 24.0;
}

fn push_combo(
    extras: &mut Vec<TbExtra>,
    x: &mut f32,
    y: f32,
    w: f32,
    label: String,
    icon: Option<IconKind>,
    action: TbAction,
) {
    extras.push(TbExtra {
        rect: Rect::new(*x, y, w, 24.0),
        action,
        label,
        checked: false,
        icon,
        kind: ExtraKind::Combo,
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
        assert!(tb.panel.is_some());
        assert!(
            !tb.custom_color.is_empty(),
            "pen sub-panel hugs a colour swatch"
        );
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
        assert!(tb.extras.iter().any(|e| e.action == TbAction::LineStyle(LineStyleKind::Dash)));
        let dash = tb.extras.iter().find(|e| e.action == TbAction::LineStyle(LineStyleKind::Dash)).unwrap();
        assert_eq!(tb.hit(dash.rect.center()), Some(TbAction::LineStyle(LineStyleKind::Dash)));
        let pw = tb.panel.unwrap().w;
        assert!(
            pw < 420.0,
            "Snipaste sub-panel hugs the chips, not a 500px strip, w={pw}"
        );
    }

    #[test]
    fn line_panel_has_style_combo() {
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let st = Style::for_tool(Tool::Line);
        let tb = Toolbar::layout(sel, over, Some(Tool::Line), false, Some(&st));
        let combo = tb
            .extras
            .iter()
            .find(|e| e.kind == ExtraKind::Combo)
            .expect("line-style combo");
        assert!(matches!(combo.action, TbAction::LineStyle(_)));
        assert_eq!(tb.hit(combo.rect.center()), Some(combo.action));
        let pen = Toolbar::layout(sel, over, Some(Tool::Pen), false, Some(&Style::for_tool(Tool::Pen)));
        assert!(
            pen.panel.unwrap().w < tb.panel.unwrap().w,
            "pen is colour-only; line adds a style combo"
        );
    }

    #[test]
    fn text_panel_has_font_combo() {
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let st = Style::for_tool(Tool::Text);
        let tb = Toolbar::layout(sel, over, Some(Tool::Text), false, Some(&st));
        let combo = tb
            .extras
            .iter()
            .find(|e| e.kind == ExtraKind::Combo)
            .expect("font combo");
        assert!(matches!(combo.action, TbAction::Family(_)));
        assert!(combo.label.contains(char::is_numeric), "combo shows size, got {}", combo.label);
        assert_eq!(tb.hit(combo.rect.center()), Some(combo.action));
    }

    #[test]
    fn compact_bar_and_button_centers() {
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let tb = Toolbar::layout(sel, over, None, false, None);
        assert!(
            tb.bar.h <= 32.0,
            "toolbar should be denser than the old 34px bar, h={}",
            tb.bar.h
        );
        let text = tb.button_center(TbAction::Tool(Tool::Text)).unwrap();
        assert_eq!(tb.hit(text), Some(TbAction::Tool(Tool::Text)));
        let pin = tb.button_center(TbAction::Pin).unwrap();
        assert_eq!(tb.hit(pin), Some(TbAction::Pin));
        let ocr = tb
            .buttons
            .iter()
            .find(|b| b.action == TbAction::Ocr)
            .expect("ocr");
        assert_eq!(ocr.icon, IconKind::Ocr);
        assert_eq!(ocr.tip, "Recognize text");
        assert!(
            tb.buttons
                .iter()
                .all(|b| b.tip != "Recognize and translate"),
            "translate is F5-only, not a toolbar button"
        );
        assert!(tb.buttons.iter().any(|b| b.action == TbAction::Tool(Tool::Eraser)));
    }

    #[test]
    fn text_panel_uses_outline_as_a() {
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let st = Style::for_tool(Tool::Text);
        let tb = Toolbar::layout(sel, over, Some(Tool::Text), false, Some(&st));
        let a = tb.extras.iter().find(|e| e.action == TbAction::Outline(true) || e.action == TbAction::Outline(false));
        assert_eq!(a.map(|e| e.label.as_str()), Some("A"));
        assert!(tb.panel.unwrap().h <= 36.0, "Snipaste sub-panel is one row");
        let mos = Toolbar::layout(sel, over, Some(Tool::Mosaic), false, Some(&Style::for_tool(Tool::Mosaic)));
        assert!(!mos.slider.is_empty(), "mosaic panel has a size slider");
        let at = Point::new(mos.slider.x + mos.slider.w * 0.5, mos.slider.y + 4.0);
        assert!(matches!(mos.hit(at), Some(TbAction::WidthSet(_))));
    }
}

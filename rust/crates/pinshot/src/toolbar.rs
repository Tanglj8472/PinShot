//! Self-drawn annotation toolbar + colour/width/style sub-panel (hit-testing only).

use pinshot_core::annotation::{Annotator, ArrowKind, Tool};
use pinshot_core::annotation::{DrawMode, Effect, LineStyleKind, Style};
use pinshot_core::color::{palette_colors, Rgba};
use pinshot_core::geom::{clamp_f32, Point, Rect};
use pinshot_core::i18n::tr;
use pinshot_core::icons::IconKind;

/// Clicking an active drawing tool (or grouped Shape / Line entry) deselects it.
pub fn toggle_tool(
    annot: &mut Annotator,
    arrow_kind: &mut ArrowKind,
    ellipse_on: &mut bool,
    action: TbAction,
) {
    match action {
        TbAction::Shape => {
            let in_group = matches!(annot.tool, Some(Tool::Rect | Tool::Ellipse));
            if in_group {
                annot.set_tool(None);
            } else {
                annot.set_tool(Some(if *ellipse_on {
                    Tool::Ellipse
                } else {
                    Tool::Rect
                }));
            }
        }
        TbAction::LineStroke => {
            let in_group = annot.tool == Some(Tool::Line);
            if in_group {
                annot.set_tool(None);
            } else {
                annot.set_tool(Some(Tool::Line));
                annot.set_style_default(Tool::Line, |s| s.set_arrow_kind(*arrow_kind));
            }
        }
        TbAction::Tool(tool) => {
            let same = annot.tool == Some(tool);
            annot.set_tool(if same { None } else { Some(tool) });
            if tool == Tool::Line && !same {
                annot.set_style_default(tool, |s| s.set_arrow_kind(*arrow_kind));
            }
        }
        _ => {}
    }
}

pub fn set_shape_kind(annot: &mut Annotator, ellipse_on: &mut bool, ellipse: bool) {
    *ellipse_on = ellipse;
    annot.convert_rect_ellipse(if ellipse { Tool::Ellipse } else { Tool::Rect });
}

pub fn set_line_preset(annot: &mut Annotator, arrow_kind: &mut ArrowKind, kind: ArrowKind) {
    *arrow_kind = kind;
    annot.convert_line_arrow(kind);
}

/// Tab while annotating: rectangle ↔ ellipse, or line ↔ arrow (Snipaste).
pub fn toggle_grouped_variant(
    annot: &mut Annotator,
    arrow_kind: &mut ArrowKind,
    ellipse_on: &mut bool,
) -> bool {
    match annot.option_tool().or(annot.tool) {
        Some(Tool::Rect) => {
            set_shape_kind(annot, ellipse_on, true);
            true
        }
        Some(Tool::Ellipse) => {
            set_shape_kind(annot, ellipse_on, false);
            true
        }
        Some(Tool::Line) => {
            let current = annot.current_line_arrow();
            let next = if current == ArrowKind::None {
                ArrowKind::Thin
            } else {
                ArrowKind::None
            };
            set_line_preset(annot, arrow_kind, next);
            true
        }
        _ => false,
    }
}

fn icon_for_arrow(kind: ArrowKind) -> IconKind {
    match kind {
        ArrowKind::None => IconKind::Line,
        ArrowKind::Thin => IconKind::Arrow,
        ArrowKind::Thick => IconKind::ArrowThick,
        ArrowKind::Hollow => IconKind::ArrowHollow,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TbAction {
    Tool(Tool),
    /// Combined rectangle / ellipse entry (Snipaste).
    Shape,
    /// Combined line / arrow entry (Snipaste).
    LineStroke,
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
    ToggleLineMenu,
    LinePreset(ArrowKind),
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
    pub menu: Option<TbMenu>,
    /// Vertical rules between extra-panel groups (fill, shape, colour…).
    pub seps: Vec<Rect>,
}

#[derive(Clone, Debug)]
pub struct TbMenu {
    pub rect: Rect,
    pub items: Vec<TbButton>,
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
        arrow: ArrowKind,
        style: Option<&Style>,
    ) -> Self {
        Self::layout_ex(sel, overlay, active, arrow, false, style)
    }

    pub fn layout_pin(
        sel: Rect,
        overlay: Rect,
        active: Option<Tool>,
        arrow: ArrowKind,
        style: Option<&Style>,
    ) -> Self {
        Self::layout_ex(sel, overlay, active, arrow, true, style)
    }

    pub fn layout_ex(
        sel: Rect,
        overlay: Rect,
        active: Option<Tool>,
        _arrow: ArrowKind,
        for_pin: bool,
        style: Option<&Style>,
    ) -> Self {
        let shape_icon = IconKind::RoundRect;
        let tools = [
            (shape_icon, TbAction::Shape, "Rectangle / Ellipse"),
            (IconKind::LineGroup, TbAction::LineStroke, "Line / Arrow"),
            (IconKind::Pen, TbAction::Tool(Tool::Pen), "Pencil"),
            (IconKind::Marker, TbAction::Tool(Tool::Marker), "Marker"),
            (
                IconKind::Mosaic,
                TbAction::Tool(Tool::Mosaic),
                "Mosaic / blur",
            ),
            (IconKind::Text, TbAction::Tool(Tool::Text), "Text"),
            (
                IconKind::Number,
                TbAction::Tool(Tool::Number),
                "Numbered marker (N)",
            ),
            (IconKind::Eraser, TbAction::Tool(Tool::Eraser), "Eraser"),
        ];
        let acts: Vec<(IconKind, TbAction, &'static str)> = if for_pin {
            vec![
                (IconKind::Ocr, TbAction::Ocr, "Recognize text"),
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
                TbAction::Shape => matches!(active, Some(Tool::Rect | Tool::Ellipse)),
                TbAction::LineStroke => active == Some(Tool::Line),
                TbAction::Tool(t) => Some(t) == active,
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
        let mut seps: Vec<Rect> = Vec::new();
        let width_minus = Rect::EMPTY;
        let width_plus = Rect::EMPTY;
        let mut width_label = Rect::EMPTY;
        let mut custom_color = Rect::EMPTY;
        let mut slider = Rect::EMPTY;
        let slider_min = 1;
        let mut slider_max = 30;
        let custom_value = style.map(|s| s.color).unwrap_or(Rgba::RED);
        if active.is_some() && active != Some(Tool::Select) {
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
                Some(
                    Tool::Rect
                        | Tool::Ellipse
                        | Tool::Line
                        | Tool::Pen
                        | Tool::Marker
                        | Tool::Mosaic
                        | Tool::Eraser
                        | Tool::Number
                )
            );
            let numeric_width = matches!(active, Some(Tool::Mosaic | Tool::Eraser));
            if show_width {
                let ww = if numeric_width { 28.0 } else { 26.0 };
                width_label = Rect::new(chip_x, chip_y, ww, 24.0);
                chip_x += ww + 4.0;
                push_sep(&mut seps, &mut chip_x, chip_y);
            }
            if let Some(st) = style {
                match active {
                    Some(Tool::Rect) | Some(Tool::Ellipse) => {
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Fill,
                            TbAction::Fill(true),
                            st.filled,
                        );
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Outline,
                            TbAction::Fill(false),
                            !st.filled,
                        );
                        push_sep(&mut seps, &mut chip_x, chip_y);
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Rect,
                            TbAction::Tool(Tool::Rect),
                            active == Some(Tool::Rect),
                        );
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Ellipse,
                            TbAction::Tool(Tool::Ellipse),
                            active == Some(Tool::Ellipse),
                        );
                    }
                    Some(Tool::Line) => {
                        let icon = icon_for_arrow(st.arrow_kind());
                        push_combo(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            56.0,
                            String::new(),
                            Some(icon),
                            TbAction::ToggleLineMenu,
                        );
                    }
                    Some(Tool::Mosaic) => {
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Mosaic,
                            TbAction::SetEffect(Effect::Mosaic),
                            st.effect == Effect::Mosaic,
                        );
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Blur,
                            TbAction::SetEffect(Effect::Blur),
                            st.effect == Effect::Blur,
                        );
                        push_sep(&mut seps, &mut chip_x, chip_y);
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Pen,
                            TbAction::SetMode(DrawMode::Brush),
                            st.mode == DrawMode::Brush,
                        );
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Rect,
                            TbAction::SetMode(DrawMode::Rect),
                            st.mode == DrawMode::Rect,
                        );
                        push_sep(&mut seps, &mut chip_x, chip_y);
                        slider = Rect::new(chip_x, chip_y + 6.0, 72.0, 12.0);
                        slider_max = 30;
                        chip_x += 80.0;
                    }
                    Some(Tool::Eraser) => {
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Pen,
                            TbAction::SetMode(DrawMode::Brush),
                            st.mode == DrawMode::Brush,
                        );
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Rect,
                            TbAction::SetMode(DrawMode::Rect),
                            st.mode == DrawMode::Rect,
                        );
                    }
                    Some(Tool::Text) => {
                        push_chip(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            22.0,
                            "B".into(),
                            TbAction::Bold(!st.bold),
                            st.bold,
                        );
                        push_chip(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            22.0,
                            "I".into(),
                            TbAction::Italic(!st.italic),
                            st.italic,
                        );
                        push_chip(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            22.0,
                            "A".into(),
                            TbAction::Outline(!st.outline),
                            st.outline,
                        );
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
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Fill,
                            TbAction::Fill(true),
                            st.filled,
                        );
                        push_icon(
                            &mut extras,
                            &mut chip_x,
                            chip_y,
                            IconKind::Outline,
                            TbAction::Fill(false),
                            !st.filled,
                        );
                    }
                    _ => {}
                }
            }
            let show_palette = !matches!(active, Some(Tool::Mosaic | Tool::Eraser | Tool::Select));
            if show_palette {
                if !extras.is_empty() || !width_label.is_empty() {
                    push_sep(&mut seps, &mut chip_x, chip_y);
                }
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
            for sep in &mut seps {
                sep.x += dx;
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
            menu: None,
            seps,
        }
    }

    /// Compact Snipaste-style hover strip on an idle pin (OCR / save / copy / close).
    pub fn layout_pin_hover(sel: Rect, overlay: Rect) -> Self {
        let acts = [
            (IconKind::Ocr, TbAction::Ocr, "Recognize text"),
            (IconKind::Save, TbAction::Save, "Save"),
            (IconKind::Copy, TbAction::Copy, "Copy"),
            (IconKind::Close, TbAction::Close, "Close"),
        ];
        let n_acts = acts.len() as f32;
        let w = PAD * 2.0 + n_acts * CELL;
        let h = CELL + PAD * 2.0;
        let mut x = sel.right() - w - 4.0;
        let mut y = sel.bottom() - h - 4.0;
        if y < sel.y {
            y = sel.y + 4.0;
        }
        x = clamp_f32(
            x,
            overlay.x.max(sel.x),
            overlay.right().min(sel.right()) - w,
        );
        let bar = Rect::new(x, y, w, h);
        let mut buttons = Vec::new();
        let mut cx = x + PAD;
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
        Self {
            bar,
            panel: None,
            buttons,
            extras: Vec::new(),
            swatches: Vec::new(),
            visible: true,
            width_minus: Rect::EMPTY,
            width_plus: Rect::EMPTY,
            width_label: Rect::EMPTY,
            width_value: 0,
            custom_color: Rect::EMPTY,
            custom_value: Rgba::RED,
            slider: Rect::EMPTY,
            slider_min: 1,
            slider_max: 30,
            menu: None,
            seps: Vec::new(),
        }
    }

    pub fn attach_line_menu(&mut self, overlay: Rect, style: &Style) {
        let Some(combo) = self
            .extras
            .iter()
            .find(|e| matches!(e.action, TbAction::ToggleLineMenu))
        else {
            self.menu = None;
            return;
        };
        let presets = [
            (ArrowKind::None, IconKind::Line, "Line"),
            (ArrowKind::Thin, IconKind::Arrow, "Thin arrow"),
            (ArrowKind::Thick, IconKind::ArrowThick, "Thick arrow"),
            (ArrowKind::Hollow, IconKind::ArrowHollow, "Hollow arrow"),
        ];
        let iw = combo.rect.w;
        let ih = 24.0;
        let h = presets.len() as f32 * ih + 6.0;
        let mut mx = combo.rect.x;
        let mut my = combo.rect.bottom() + 2.0;
        if my + h > overlay.bottom() {
            my = combo.rect.y - h - 2.0;
        }
        mx = clamp_f32(mx, overlay.x, overlay.right() - iw);
        my = my.max(overlay.y);
        let rect = Rect::new(mx, my, iw, h);
        let mut items = Vec::new();
        for (i, (kind, icon, tip)) in presets.iter().enumerate() {
            let checked = style.arrow_kind() == *kind;
            items.push(TbButton {
                rect: Rect::new(mx + 3.0, my + 3.0 + i as f32 * ih, iw - 6.0, ih - 2.0),
                icon: *icon,
                action: TbAction::LinePreset(*kind),
                tip,
                checked,
            });
        }
        self.menu = Some(TbMenu { rect, items });
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
        if let Some(menu) = &self.menu {
            if menu.rect.contains(p) {
                return true;
            }
        }
        false
    }

    pub fn hit(&self, p: Point) -> Option<TbAction> {
        if !self.visible {
            return None;
        }
        if let Some(menu) = &self.menu {
            for b in &menu.items {
                if b.rect.contains(p) {
                    return Some(b.action);
                }
            }
            if menu.rect.contains(p) {
                return None;
            }
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

fn push_sep(seps: &mut Vec<Rect>, x: &mut f32, y: f32) {
    let sx = (*x + (SEP - 1.0) * 0.5).round();
    seps.push(Rect::new(sx, y + 4.0, 1.0, 16.0));
    *x += SEP;
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
        let mut arrow = ArrowKind::None;
        let mut ellipse = false;
        toggle_tool(&mut ann, &mut arrow, &mut ellipse, TbAction::Shape);
        assert_eq!(ann.tool, Some(Tool::Rect));
        toggle_tool(&mut ann, &mut arrow, &mut ellipse, TbAction::Shape);
        assert_eq!(ann.tool, None);
        set_shape_kind(&mut ann, &mut ellipse, true);
        assert_eq!(ann.tool, Some(Tool::Ellipse));
        assert!(ellipse);
        toggle_tool(&mut ann, &mut arrow, &mut ellipse, TbAction::LineStroke);
        assert_eq!(ann.tool, Some(Tool::Line));
        assert!(!ann.style(Tool::Line).has_arrow());
        set_line_preset(&mut ann, &mut arrow, ArrowKind::Thin);
        assert_eq!(ann.tool, Some(Tool::Line));
        assert_eq!(ann.style(Tool::Line).arrow_kind(), ArrowKind::Thin);
        set_line_preset(&mut ann, &mut arrow, ArrowKind::Hollow);
        assert_eq!(ann.style(Tool::Line).arrow_kind(), ArrowKind::Hollow);
        toggle_tool(&mut ann, &mut arrow, &mut ellipse, TbAction::LineStroke);
        assert_eq!(ann.tool, None);
    }

    #[test]
    fn line_chip_converts_selected_line_to_arrow() {
        let mut ann = Annotator::new();
        let mut arrow = ArrowKind::None;
        let mut ellipse = false;
        ann.set_tool(Some(Tool::Line));
        ann.press(Point::new(8.0, 8.0), false);
        ann.move_to(Point::new(60.0, 8.0), false);
        ann.release(Point::new(60.0, 8.0));
        assert_eq!(ann.shapes[0].style.arrow_kind(), ArrowKind::None);
        set_line_preset(&mut ann, &mut arrow, ArrowKind::Thin);
        assert_eq!(arrow, ArrowKind::Thin);
        assert_eq!(ann.shapes[0].style.arrow_kind(), ArrowKind::Thin);
        assert_eq!(ann.selected, Some(0));
        assert!(toggle_grouped_variant(&mut ann, &mut arrow, &mut ellipse));
        assert_eq!(ann.shapes[0].style.arrow_kind(), ArrowKind::None);
        assert_eq!(arrow, ArrowKind::None);
    }

    #[test]
    fn shape_chip_converts_selected_rect_to_ellipse() {
        let mut ann = Annotator::new();
        let mut ellipse = false;
        ann.set_tool(Some(Tool::Rect));
        ann.press(Point::new(8.0, 8.0), false);
        ann.move_to(Point::new(60.0, 40.0), false);
        ann.release(Point::new(60.0, 40.0));
        assert_eq!(ann.shapes[0].kind, Tool::Rect);
        set_shape_kind(&mut ann, &mut ellipse, true);
        assert!(ellipse);
        assert_eq!(ann.shapes[0].kind, Tool::Ellipse);
        assert_eq!(ann.selected, Some(0));
        assert_eq!(ann.tool, Some(Tool::Ellipse));
        assert!(toggle_grouped_variant(
            &mut ann,
            &mut ArrowKind::None,
            &mut ellipse
        ));
        assert_eq!(ann.shapes[0].kind, Tool::Rect);
        assert!(!ellipse);
    }

    #[test]
    fn select_tool_has_no_draw_panel() {
        let sel = Rect::new(0.0, 0.0, 240.0, 120.0);
        let over = Rect::new(0.0, 0.0, 400.0, 300.0);
        let tb = Toolbar::layout_pin(sel, over, Some(Tool::Select), ArrowKind::None, None);
        assert!(
            tb.panel.is_none(),
            "Select must not open a rect style panel"
        );
        let rect = Toolbar::layout_pin(sel, over, Some(Tool::Rect), ArrowKind::None, None);
        assert!(rect.panel.is_some());
    }

    #[test]
    fn layout_on_tiny_pin_does_not_panic() {
        let sel = Rect::new(0.0, 0.0, 20.0, 12.0);
        let over = Rect::new(0.0, 0.0, 40.0, 20.0);
        let st = Style::for_tool(Tool::Pen);
        let tb = Toolbar::layout_pin(sel, over, Some(Tool::Pen), ArrowKind::None, Some(&st));
        assert!(tb.bar.w > 0.0);
        assert!(tb.panel.is_some());
        assert!(
            !tb.custom_color.is_empty(),
            "pen sub-panel hugs a colour swatch"
        );
        assert!(
            tb.buttons.iter().any(|b| b.action == TbAction::Ocr),
            "pin annotation toolbar must expose OCR"
        );
    }

    #[test]
    fn pin_hover_bar_has_ocr() {
        let sel = Rect::new(0.0, 0.0, 240.0, 120.0);
        let over = Rect::new(0.0, 0.0, 240.0, 120.0);
        let tb = Toolbar::layout_pin_hover(sel, over);
        let ocr = tb
            .buttons
            .iter()
            .find(|b| b.action == TbAction::Ocr)
            .expect("hover OCR");
        assert_eq!(ocr.icon, IconKind::Ocr);
        assert_eq!(tb.hit(ocr.rect.center()), Some(TbAction::Ocr));
        assert!(tb.buttons.iter().any(|b| b.action == TbAction::Copy));
        assert!(tb.buttons.iter().any(|b| b.action == TbAction::Close));
        assert!(
            tb.buttons
                .iter()
                .all(|b| !matches!(b.action, TbAction::Tool(_))),
            "idle hover bar is actions only"
        );
        let tips: Vec<&str> = tb.buttons.iter().map(|b| b.tip).collect();
        assert_eq!(tips, ["Recognize text", "Save", "Copy", "Close"]);
        assert!(tips.iter().all(|t| !t.is_empty()));
    }

    #[test]
    fn rect_panel_has_fill_and_outline() {
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let st = Style::for_tool(Tool::Rect);
        let tb = Toolbar::layout(sel, over, Some(Tool::Rect), ArrowKind::None, Some(&st));
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
        assert!(tb
            .extras
            .iter()
            .any(|e| e.action == TbAction::Tool(Tool::Rect)));
        assert!(tb
            .extras
            .iter()
            .any(|e| e.action == TbAction::Tool(Tool::Ellipse)));
        let ell = tb
            .extras
            .iter()
            .find(|e| e.action == TbAction::Tool(Tool::Ellipse))
            .unwrap();
        assert_eq!(
            tb.hit(ell.rect.center()),
            Some(TbAction::Tool(Tool::Ellipse))
        );
        let pw = tb.panel.unwrap().w;
        assert!(
            pw < 420.0,
            "Snipaste sub-panel hugs the chips, not a 500px strip, w={pw}"
        );
        assert_eq!(
            tb.seps.len(),
            3,
            "width | fill/stroke | rect/ellipse | colour"
        );
        let fill_r = fill.unwrap().rect;
        let outline_r = outline.unwrap().rect;
        let rect_r = tb
            .extras
            .iter()
            .find(|e| e.action == TbAction::Tool(Tool::Rect))
            .unwrap()
            .rect;
        let xs: Vec<f32> = tb.seps.iter().map(|s| s.x).collect();
        assert!(
            xs[0] > tb.width_label.right() && xs[0] < fill_r.x,
            "first rule sits after the width chip"
        );
        assert!(
            xs[1] > outline_r.right() && xs[1] < rect_r.x,
            "second rule splits fill/stroke from rect/ellipse"
        );
        assert!(
            xs[2] > ell.rect.right() && xs[2] < tb.custom_color.x,
            "third rule sits before the colour well"
        );
    }

    #[test]
    fn line_panel_has_style_combo() {
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let st = Style::for_tool(Tool::Line);
        let tb = Toolbar::layout(sel, over, Some(Tool::Line), ArrowKind::None, Some(&st));
        let combo = tb
            .extras
            .iter()
            .find(|e| e.kind == ExtraKind::Combo)
            .expect("line-style combo");
        assert_eq!(combo.action, TbAction::ToggleLineMenu);
        assert_eq!(tb.hit(combo.rect.center()), Some(TbAction::ToggleLineMenu));
        let mut open = tb.clone();
        open.attach_line_menu(over, &st);
        let menu = open.menu.as_ref().expect("line preset menu");
        assert_eq!(
            menu.rect.w, combo.rect.w,
            "open menu must match the closed combo width"
        );
        assert_eq!(menu.items[0].icon, IconKind::Line);
        assert_eq!(menu.items.len(), 4);
        assert_eq!(
            menu.items.iter().map(|b| b.action).collect::<Vec<_>>(),
            vec![
                TbAction::LinePreset(ArrowKind::None),
                TbAction::LinePreset(ArrowKind::Thin),
                TbAction::LinePreset(ArrowKind::Thick),
                TbAction::LinePreset(ArrowKind::Hollow),
            ]
        );
        assert_eq!(
            open.hit(menu.items[1].rect.center()),
            Some(TbAction::LinePreset(ArrowKind::Thin))
        );
        assert_eq!(
            open.hit(menu.items[3].rect.center()),
            Some(TbAction::LinePreset(ArrowKind::Hollow))
        );
        let pen = Toolbar::layout(
            sel,
            over,
            Some(Tool::Pen),
            ArrowKind::None,
            Some(&Style::for_tool(Tool::Pen)),
        );
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
        let tb = Toolbar::layout(sel, over, Some(Tool::Text), ArrowKind::None, Some(&st));
        let combo = tb
            .extras
            .iter()
            .find(|e| e.kind == ExtraKind::Combo)
            .expect("font combo");
        assert!(matches!(combo.action, TbAction::Family(_)));
        assert!(
            combo.label.contains(char::is_numeric),
            "combo shows size, got {}",
            combo.label
        );
        assert_eq!(tb.hit(combo.rect.center()), Some(combo.action));
    }

    #[test]
    fn compact_bar_and_button_centers() {
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let tb = Toolbar::layout(sel, over, None, ArrowKind::None, None);
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
        assert!(tb
            .buttons
            .iter()
            .any(|b| b.action == TbAction::Tool(Tool::Eraser)));
        assert!(tb.buttons.iter().any(|b| b.action == TbAction::Shape));
        assert!(tb
            .buttons
            .iter()
            .any(|b| b.action == TbAction::Shape && b.icon == IconKind::RoundRect));
        let ell = Toolbar::layout(sel, over, Some(Tool::Ellipse), ArrowKind::None, None);
        assert!(ell
            .buttons
            .iter()
            .any(|b| b.action == TbAction::Shape && b.icon == IconKind::RoundRect));
        assert!(tb.buttons.iter().any(|b| b.action == TbAction::LineStroke));
        assert!(tb
            .buttons
            .iter()
            .any(|b| b.action == TbAction::LineStroke && b.icon == IconKind::LineGroup));
        let lined = Toolbar::layout(sel, over, Some(Tool::Line), ArrowKind::Thin, None);
        assert!(lined
            .buttons
            .iter()
            .any(|b| b.action == TbAction::LineStroke && b.icon == IconKind::LineGroup));
        assert!(
            tb.buttons
                .iter()
                .all(|b| b.action != TbAction::Tool(Tool::Ellipse)
                    && b.action != TbAction::Tool(Tool::Rect)
                    && b.action != TbAction::Tool(Tool::Line)),
            "rect/ellipse and line/arrow are grouped main entries"
        );
    }

    #[test]
    fn text_panel_uses_outline_as_a() {
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let st = Style::for_tool(Tool::Text);
        let tb = Toolbar::layout(sel, over, Some(Tool::Text), ArrowKind::None, Some(&st));
        let a = tb
            .extras
            .iter()
            .find(|e| e.action == TbAction::Outline(true) || e.action == TbAction::Outline(false));
        assert_eq!(a.map(|e| e.label.as_str()), Some("A"));
        assert!(tb.panel.unwrap().h <= 36.0, "Snipaste sub-panel is one row");
        let mos = Toolbar::layout(
            sel,
            over,
            Some(Tool::Mosaic),
            ArrowKind::None,
            Some(&Style::for_tool(Tool::Mosaic)),
        );
        assert!(!mos.slider.is_empty(), "mosaic panel has a size slider");
        let at = Point::new(mos.slider.x + mos.slider.w * 0.5, mos.slider.y + 4.0);
        assert!(matches!(mos.hit(at), Some(TbAction::WidthSet(_))));
    }
}

//! Annotation document, tools, hit-testing, and undo/redo (snapshot stack).
//!
//! Mirrors `pinshot/annotate.py`: `option_target` is live | selected | edit_target |
//! width_target; `wheel()` accumulates Qt-style `angleDelta/120` and merges undo
//! for 0.5s on the same committed shape.

use crate::color::{palette_colors, Rgba};
use crate::geom::{
    constrain_square, dist_to_polyline, handle_points, hit_handle, point_in_ellipse, Handle, Point,
    Rect, HANDLE_SIZE,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Select,
    Rect,
    Ellipse,
    Line,
    Pen,
    Marker,
    Mosaic,
    Text,
    Number,
    Eraser,
}

impl Tool {
    pub const DRAW: [Tool; 9] = [
        Tool::Rect,
        Tool::Ellipse,
        Tool::Line,
        Tool::Pen,
        Tool::Marker,
        Tool::Mosaic,
        Tool::Text,
        Tool::Number,
        Tool::Eraser,
    ];

    /// Phase-1 toolbar (no select / eraser / OCR).
    pub const PHASE1: [Tool; 8] = [
        Tool::Rect,
        Tool::Ellipse,
        Tool::Line,
        Tool::Pen,
        Tool::Marker,
        Tool::Mosaic,
        Tool::Text,
        Tool::Number,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Rect => "rect",
            Tool::Ellipse => "ellipse",
            Tool::Line => "line",
            Tool::Pen => "pen",
            Tool::Marker => "marker",
            Tool::Mosaic => "mosaic",
            Tool::Text => "text",
            Tool::Number => "number",
            Tool::Eraser => "eraser",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        Some(match s {
            "select" => Tool::Select,
            "rect" => Tool::Rect,
            "ellipse" => Tool::Ellipse,
            "line" => Tool::Line,
            "arrow" => Tool::Line,
            "pen" | "pencil" => Tool::Pen,
            "marker" => Tool::Marker,
            "mosaic" => Tool::Mosaic,
            "text" => Tool::Text,
            "number" => Tool::Number,
            "eraser" => Tool::Eraser,
            _ => return None,
        })
    }

    pub fn tip(self) -> &'static str {
        match self {
            Tool::Select => "Select / move / resize (V)",
            Tool::Rect => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Line => "Line / arrow (Shift+drag continues polyline)",
            Tool::Pen => "Pencil",
            Tool::Marker => "Marker",
            Tool::Mosaic => "Mosaic / blur",
            Tool::Text => "Text",
            Tool::Number => "Numbered marker (N)",
            Tool::Eraser => "Eraser",
        }
    }

    pub fn is_brush(self, style: &Style) -> bool {
        matches!(self, Tool::Pen | Tool::Marker)
            || (matches!(self, Tool::Mosaic | Tool::Eraser) && style.mode == DrawMode::Brush)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineStyleKind {
    Solid,
    Dash,
    Dot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawMode {
    Brush,
    Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Mosaic,
    Blur,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub color: Rgba,
    pub width: i32,
    pub filled: bool,
    pub arrow: bool,
    pub line_style: LineStyleKind,
    pub mode: DrawMode,
    pub effect: Effect,
    pub size: i32,
    pub bold: bool,
    pub italic: bool,
    pub outline: bool,
    pub family: String,
    pub font_size: i32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            color: Rgba::RED,
            width: 3,
            filled: false,
            arrow: true,
            line_style: LineStyleKind::Solid,
            mode: DrawMode::Rect,
            effect: Effect::Mosaic,
            size: 6,
            bold: false,
            italic: false,
            outline: true,
            family: String::new(),
            font_size: 20,
        }
    }
}

impl Style {
    pub fn for_tool(tool: Tool) -> Self {
        let mut s = Self::default();
        match tool {
            Tool::Marker => {
                s.color = Rgba::MARKER_YELLOW;
                s.width = 14;
            }
            Tool::Pen => s.width = 3,
            Tool::Line => {
                s.width = 3;
                s.arrow = false;
            }
            Tool::Eraser => {
                s.mode = DrawMode::Brush;
                s.width = 16;
            }
            Tool::Mosaic => s.mode = DrawMode::Brush,
            Tool::Number => {
                s.width = 22;
                s.filled = true;
                s.font_size = 14;
            }
            _ => {}
        }
        s
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub kind: Tool,
    pub pts: Vec<Point>,
    pub style: Style,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub number: i32,
}

impl Shape {
    pub fn new(kind: Tool, pts: Vec<Point>, style: Style) -> Self {
        Self {
            kind,
            pts,
            style,
            text: String::new(),
            number: 0,
        }
    }

    pub fn bounds(&self) -> Rect {
        if self.kind == Tool::Number {
            if let Some(c) = self.pts.first() {
                let r = (self.style.width.max(8) as f32) / 2.0;
                return Rect::new(c.x - r, c.y - r, r * 2.0, r * 2.0);
            }
        }
        if self.kind == Tool::Text && !self.text.is_empty() {
            if let Some(p) = self.pts.first() {
                let w = approx_text_width(&self.text, self.style.font_size);
                let lines = self.text.lines().count().max(1) as f32;
                let h = self.style.font_size as f32 * 1.25 * lines;
                return Rect::new(p.x, p.y, w + 4.0, h + 2.0);
            }
        }
        if matches!(self.kind, Tool::Pen | Tool::Marker | Tool::Line) && self.pts.len() > 2 {
            let xs = self.pts.iter().map(|p| p.x);
            let ys = self.pts.iter().map(|p| p.y);
            let x0 = xs.clone().fold(f32::MAX, f32::min);
            let x1 = xs.fold(f32::MIN, f32::max);
            let y0 = ys.clone().fold(f32::MAX, f32::min);
            let y1 = ys.fold(f32::MIN, f32::max);
            return Rect::from_xyxy(x0, y0, x1, y1);
        }
        if self.pts.len() < 2 {
            let p = self.pts.first().copied().unwrap_or(Point::ZERO);
            return Rect::new(p.x - 2.0, p.y - 2.0, 4.0, 4.0);
        }
        Rect::from_points(self.pts[0], *self.pts.last().unwrap()).normalized()
    }
}

fn approx_text_width(text: &str, font_size: i32) -> f32 {
    let fs = font_size.max(8) as f32;
    text.lines()
        .map(|l| l.chars().count() as f32 * fs * 0.6)
        .fold(0.0, f32::max)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UndoStack {
    undo: Vec<Vec<Shape>>,
    redo: Vec<Vec<Shape>>,
}

impl UndoStack {
    pub const LIMIT: usize = 50;

    pub fn push(&mut self, before: Vec<Shape>) {
        self.undo.push(before);
        if self.undo.len() > Self::LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: Vec<Shape>) -> Option<Vec<Shape>> {
        let prev = self.undo.pop()?;
        self.redo.push(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: Vec<Shape>) -> Option<Vec<Shape>> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelMode {
    Move,
    Handle(Handle),
}

#[derive(Clone, Debug)]
pub struct TextEdit {
    pub img_pt: Point,
    pub text: String,
    pub existing: Option<usize>,
    started: Instant,
}

#[derive(Clone, Debug)]
pub struct Annotator {
    pub tool: Option<Tool>,
    pub styles: [Style; 9],
    pub shapes: Vec<Shape>,
    pub live: Option<Shape>,
    pub selected: Option<usize>,
    pub next_number: i32,
    pub text_edit: Option<TextEdit>,
    /// Last committed shape the user clicked (Python `edit_target`).
    pub edit_target: Option<usize>,
    /// Last added shape (Python `width_target`); wheel/style hit this when nothing is selected.
    pub width_target: Option<usize>,
    undo: UndoStack,
    sel_mode: Option<SelMode>,
    sel_origin: Point,
    sel_pts0: Vec<Point>,
    sel_dirty: bool,
    wheel_target: Option<usize>,
    wheel_time: Instant,
    wheel_delta: i32,
}

impl Default for Annotator {
    fn default() -> Self {
        Self {
            tool: None,
            styles: Tool::DRAW.map(Style::for_tool),
            shapes: Vec::new(),
            live: None,
            selected: None,
            next_number: 1,
            text_edit: None,
            edit_target: None,
            width_target: None,
            undo: UndoStack::default(),
            sel_mode: None,
            sel_origin: Point::ZERO,
            sel_pts0: Vec::new(),
            sel_dirty: false,
            wheel_target: None,
            wheel_time: Instant::now(),
            wheel_delta: 0,
        }
    }
}

impl Annotator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn style_index(tool: Tool) -> Option<usize> {
        Tool::DRAW.iter().position(|&t| t == tool)
    }

    pub fn style(&self, tool: Tool) -> &Style {
        let i = Self::style_index(tool).unwrap_or(0);
        &self.styles[i]
    }

    pub fn style_mut(&mut self, tool: Tool) -> &mut Style {
        let i = Self::style_index(tool).unwrap_or(0);
        &mut self.styles[i]
    }

    pub fn set_tool(&mut self, tool: Option<Tool>) {
        self.finish();
        self.width_target = None;
        self.edit_target = None;
        self.wheel_target = None;
        self.wheel_delta = 0;
        if tool != Some(Tool::Select) {
            self.selected = None;
        }
        self.tool = tool;
        self.sel_mode = None;
    }

    pub fn all_shapes(&self) -> impl Iterator<Item = &Shape> {
        self.shapes.iter().chain(self.live.as_ref())
    }

    fn push_edit(&mut self) {
        self.wheel_target = None;
        self.undo.push(self.shapes.clone());
    }

    pub fn add(&mut self, shape: Shape) {
        self.push_edit();
        self.shapes.push(shape);
        self.width_target = Some(self.shapes.len() - 1);
        self.renumber();
    }

    pub fn undo(&mut self) -> bool {
        self.finish();
        self.selected = None;
        self.edit_target = None;
        self.wheel_target = None;
        self.width_target = None;
        if let Some(prev) = self.undo.undo(self.shapes.clone()) {
            self.shapes = prev;
            self.renumber();
            return true;
        }
        false
    }

    pub fn redo(&mut self) -> bool {
        self.selected = None;
        self.edit_target = None;
        self.wheel_target = None;
        self.width_target = None;
        if let Some(next) = self.undo.redo(self.shapes.clone()) {
            self.shapes = next;
            self.renumber();
            return true;
        }
        false
    }

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.undo_len()
    }

    pub fn clear(&mut self) {
        self.shapes.clear();
        self.live = None;
        self.selected = None;
        self.next_number = 1;
        self.undo.clear();
        self.text_edit = None;
        self.sel_mode = None;
        self.edit_target = None;
        self.width_target = None;
        self.wheel_target = None;
        self.wheel_delta = 0;
    }

    pub fn renumber(&mut self) {
        let mut n = 1;
        for sh in &mut self.shapes {
            if sh.kind == Tool::Number {
                sh.number = n;
                n += 1;
            }
        }
        self.next_number = n;
    }

    pub fn finish(&mut self) {
        if let Some(ed) = self.text_edit.take() {
            if !ed.text.trim().is_empty() {
                if let Some(idx) = ed.existing {
                    if idx < self.shapes.len() {
                        self.push_edit();
                        self.shapes[idx].text = ed.text;
                    }
                } else {
                    let mut sh =
                        Shape::new(Tool::Text, vec![ed.img_pt], self.style(Tool::Text).clone());
                    sh.text = ed.text;
                    self.add(sh);
                }
            }
        }
        if let Some(sh) = self.live.take() {
            let ok = sh.pts.len() > 1
                && (sh.pts[0].manhattan(*sh.pts.last().unwrap()) > 2.0 || sh.pts.len() > 2);
            if ok {
                self.add(sh);
            }
        }
        self.sel_mode = None;
    }

    pub fn set_style_value(&mut self, kind: Tool, setter: impl Fn(&mut Style)) {
        if let Some(live) = self.live.as_mut() {
            if live.kind == kind {
                setter(&mut live.style);
                setter(self.style_mut(kind));
                return;
            }
        }
        if let Some(idx) = self.option_target() {
            if self.shapes[idx].kind == kind {
                self.push_edit();
                setter(&mut self.shapes[idx].style);
            }
        }
        setter(self.style_mut(kind));
    }

    /// Change the default style for *new* drawings only (picking Arrow/Line in
    /// the toolbar must not rewrite the last committed shape).
    pub fn set_style_default(&mut self, kind: Tool, setter: impl Fn(&mut Style)) {
        setter(self.style_mut(kind));
    }

    /// Index of the committed shape that style/width edits apply to.
    /// Python: `live or selected or edit_target or width_target` — live is handled
    /// separately because it is not in `shapes`.
    pub fn option_target(&self) -> Option<usize> {
        if self.live.is_some() {
            return None;
        }
        for cand in [self.selected, self.edit_target, self.width_target] {
            if let Some(i) = cand {
                if i < self.shapes.len() {
                    return Some(i);
                }
            }
        }
        None
    }

    pub fn option_tool(&self) -> Option<Tool> {
        if matches!(self.tool, Some(Tool::Eraser | Tool::Mosaic)) {
            return self.tool;
        }
        if let Some(live) = &self.live {
            return Some(live.kind);
        }
        if let Some(i) = self.option_target() {
            return Some(self.shapes[i].kind);
        }
        self.tool
    }

    /// Width shown in the toolbar panel: live / selected shape, else the tool default.
    pub fn option_width(&self) -> i32 {
        if let Some(live) = &self.live {
            return live.style.width;
        }
        if let Some(i) = self.option_target() {
            return self.shapes[i].style.width;
        }
        if let Some(t) = self.tool.filter(|t| *t != Tool::Select) {
            return self.style(t).width;
        }
        0
    }

    pub fn adjust_width(&mut self, delta: i32) -> bool {
        if delta == 0 {
            return false;
        }
        if let Some(live) = self.live.as_mut() {
            let kind = live.kind;
            Self::nudge_style(&mut live.style, kind, delta);
            let st = live.style.clone();
            *self.style_mut(kind) = st;
            return true;
        }
        let target = self.option_target();
        let kind = target
            .map(|i| self.shapes[i].kind)
            .or(self.tool.filter(|t| *t != Tool::Select));
        let Some(kind) = kind else {
            return false;
        };
        if let Some(i) = target {
            let now = Instant::now();
            if self.wheel_target != Some(i)
                || now.duration_since(self.wheel_time) > Duration::from_millis(500)
            {
                self.push_edit();
            }
            self.wheel_target = Some(i);
            self.wheel_time = now;
            Self::nudge_style(&mut self.shapes[i].style, kind, delta);
            let st = self.shapes[i].style.clone();
            *self.style_mut(kind) = st;
        } else {
            Self::nudge_style(self.style_mut(kind), kind, delta);
        }
        true
    }

    /// Qt `QWheelEvent.angleDelta().y()` — 120 units per notch. Truncates toward 0.
    pub fn wheel(&mut self, angle_delta: i32) -> bool {
        self.wheel_delta += angle_delta;
        let steps = self.wheel_delta / 120;
        self.wheel_delta -= steps * 120;
        if steps != 0 {
            self.adjust_width(steps)
        } else {
            false
        }
    }

    fn nudge_style(st: &mut Style, kind: Tool, delta: i32) {
        match kind {
            Tool::Text => st.font_size = (st.font_size + 2 * delta).clamp(8, 120),
            Tool::Mosaic => st.size = (st.size + delta).clamp(1, 30),
            Tool::Number => {
                st.width = (st.width + 2 * delta).clamp(12, 80);
                st.font_size = (st.width as f32 * 0.65) as i32;
                st.font_size = st.font_size.max(8);
            }
            _ => st.width = (st.width + delta).clamp(1, 60),
        }
    }

    pub fn hit_test(&self, img_pt: Point) -> Option<usize> {
        for (i, sh) in self.shapes.iter().enumerate().rev() {
            if sh.kind == Tool::Eraser {
                if self.hits_shape(sh, img_pt) {
                    return None;
                }
                continue;
            }
            if self.hits_shape(sh, img_pt) {
                return Some(i);
            }
        }
        None
    }

    fn hits_shape(&self, sh: &Shape, p: Point) -> bool {
        let b = sh.bounds();
        let margin = (sh.style.width as f32 / 2.0)
            .max(sh.style.font_size as f32 / 7.0)
            .max(sh.style.size as f32 * 3.0)
            + 2.0;
        let expanded = b.adjusted(-margin, -margin, margin, margin);
        if !expanded.contains(p) && sh.kind != Tool::Line {
            if !matches!(sh.kind, Tool::Pen | Tool::Marker) {
                return false;
            }
        }
        match sh.kind {
            Tool::Rect => {
                if sh.style.filled {
                    b.contains(p)
                } else {
                    near_rect_edge(p, b, (sh.style.width as f32 / 2.0).max(6.0))
                }
            }
            Tool::Ellipse => {
                if sh.style.filled {
                    point_in_ellipse(p, b)
                } else {
                    let inside = point_in_ellipse(p, b);
                    let shrink = b.adjusted(4.0, 4.0, -4.0, -4.0);
                    inside && !point_in_ellipse(p, shrink)
                }
            }
            Tool::Line | Tool::Pen | Tool::Marker => {
                dist_to_polyline(p, &sh.pts) <= (sh.style.width as f32 / 2.0).max(8.0)
            }
            Tool::Number => b.contains(p),
            Tool::Text => b.contains(p),
            Tool::Mosaic => {
                if sh.style.mode == DrawMode::Brush {
                    dist_to_polyline(p, &sh.pts) <= (sh.style.size as f32 * 3.0).max(4.0)
                } else {
                    b.contains(p)
                }
            }
            Tool::Eraser => {
                if sh.style.mode == DrawMode::Brush {
                    dist_to_polyline(p, &sh.pts) <= (sh.style.width as f32 / 2.0).max(4.0)
                } else {
                    b.contains(p)
                }
            }
            Tool::Select => false,
        }
    }

    pub fn hit_handle_of_selected(&self, img_pt: Point) -> Option<Handle> {
        let i = self.selected?;
        let b = self.shapes.get(i)?.bounds().adjusted(-2.0, -2.0, 2.0, 2.0);
        hit_handle(b, img_pt, HANDLE_SIZE / 2.0 + 2.0)
    }

    pub fn delete_selected(&mut self) -> bool {
        if let Some(i) = self.selected {
            if i < self.shapes.len() {
                self.push_edit();
                self.shapes.remove(i);
                self.selected = None;
                self.renumber();
                return true;
            }
        }
        false
    }

    pub fn is_selecting(&self) -> bool {
        self.sel_mode.is_some()
    }

    pub fn is_brush_live(&self) -> bool {
        self.live
            .as_ref()
            .map(|s| s.kind.is_brush(&s.style))
            .unwrap_or(false)
    }

    /// Left-button press in image coordinates. Returns true if the annotator consumed the event.
    pub fn press(&mut self, img_pt: Point, shift: bool) -> bool {
        let tool = self.tool;

        // Python: Select, or any tool except eraser/mosaic, steals a hit on an
        // existing shape (edge / handle) so the 8 handles appear.
        let steal = tool == Some(Tool::Select)
            || (tool != Some(Tool::Eraser)
                && tool != Some(Tool::Mosaic)
                && (self.hit_test(img_pt).is_some()
                    || self.hit_handle_of_selected(img_pt).is_some()));
        if steal {
            if let Some(h) = self.hit_handle_of_selected(img_pt) {
                if self.selected.is_some() {
                    self.sel_mode = Some(SelMode::Handle(h));
                    self.sel_origin = img_pt;
                    self.sel_dirty = false;
                    self.sel_pts0 = self.shapes[self.selected.unwrap()].pts.clone();
                    return true;
                }
            }
            let hit = self.hit_test(img_pt);
            self.selected = hit;
            self.edit_target = hit;
            if let Some(i) = hit {
                self.sel_mode = Some(SelMode::Move);
                self.sel_origin = img_pt;
                self.sel_dirty = false;
                self.sel_pts0 = self.shapes[i].pts.clone();
            }
            return true;
        }

        if self.selected.is_some() || self.edit_target.is_some() {
            self.selected = None;
            self.edit_target = None;
        }
        let Some(tool) = tool else {
            return false;
        };
        self.width_target = None;
        self.edit_target = None;

        let st = self.style(tool).clone();
        match tool {
            Tool::Text => {
                if let Some(ed) = &self.text_edit {
                    if ed.text.trim().is_empty()
                        || ed.started.elapsed() < Duration::from_millis(200)
                    {
                        self.start_text(img_pt, None);
                        return true;
                    }
                    self.commit_text(false);
                    return true;
                }
                self.start_text(img_pt, None);
                true
            }
            Tool::Number => {
                let mut sh = Shape::new(Tool::Number, vec![img_pt], st);
                sh.number = self.next_number;
                self.next_number += 1;
                self.add(sh);
                true
            }
            Tool::Line if shift && self.shapes.last().map(|s| s.kind) == Some(Tool::Line) => {
                // Continue the last polyline. `add()` already snapshotted the pre-line
                // document, so popping here returns to that snapshot without a second undo push.
                if let Some(mut sh) = self.shapes.pop() {
                    sh.pts.push(img_pt);
                    self.live = Some(sh);
                }
                true
            }
            _ if tool.is_brush(&st) => {
                self.live = Some(Shape::new(tool, vec![img_pt], st));
                true
            }
            _ => {
                self.live = Some(Shape::new(tool, vec![img_pt, img_pt], st));
                true
            }
        }
    }

    pub fn start_text(&mut self, pt: Point, existing: Option<usize>) {
        let (text, existing) = if let Some(idx) = existing {
            match self.shapes.get(idx) {
                Some(sh) if sh.kind == Tool::Text => (sh.text.clone(), Some(idx)),
                _ => (String::new(), None),
            }
        } else {
            (String::new(), None)
        };
        self.text_edit = Some(TextEdit {
            img_pt: pt,
            text,
            existing,
            started: Instant::now(),
        });
    }

    /// Wheel over a shape selects it (so width/font follow the hit) then adjusts.
    pub fn wheel_at(&mut self, img_pt: Point, angle_delta: i32) -> bool {
        if self.live.is_none() && self.text_edit.is_none() && self.selected.is_none() {
            if let Some(i) = self.hit_test(img_pt) {
                self.selected = Some(i);
                self.edit_target = Some(i);
                self.width_target = Some(i);
            }
        }
        self.wheel(angle_delta)
    }

    pub fn begin_edit_text(&mut self, idx: usize) {
        let Some(sh) = self.shapes.get(idx) else {
            return;
        };
        if sh.kind != Tool::Text {
            return;
        }
        let pt = sh.pts.first().copied().unwrap_or(Point::ZERO);
        self.set_tool(Some(Tool::Select));
        self.selected = Some(idx);
        self.edit_target = Some(idx);
        self.start_text(pt, Some(idx));
    }

    /// Dashed editor box in image coordinates (Python QLineEdit size).
    pub fn text_edit_rect(&self) -> Option<Rect> {
        let ed = self.text_edit.as_ref()?;
        let st = if let Some(i) = ed.existing {
            self.shapes
                .get(i)
                .map(|s| s.style.clone())
                .unwrap_or_else(|| self.style(Tool::Text).clone())
        } else {
            self.style(Tool::Text).clone()
        };
        let fs = st.font_size.max(8) as f32;
        let w = (approx_text_width(&(ed.text.clone() + "MM"), st.font_size)).max(80.0);
        let h = fs * 1.25 + 4.0;
        Some(Rect::new(ed.img_pt.x, ed.img_pt.y, w, h))
    }

    pub fn type_text(&mut self, ch: char) {
        if let Some(ed) = &mut self.text_edit {
            if ch == '\u{8}' {
                ed.text.pop();
            } else if !ch.is_control() && ch != '\u{2013}' && ch != '\u{2014}' && ch != '\u{ffe3}' {
                ed.text.push(ch);
            }
        }
    }

    pub fn commit_text(&mut self, cancel: bool) {
        if cancel {
            self.text_edit = None;
            return;
        }
        self.finish();
    }

    pub fn move_to(&mut self, img_pt: Point, shift: bool) -> bool {
        if let (Some(mode), Some(idx)) = (self.sel_mode, self.selected) {
            if img_pt == self.sel_origin {
                return true;
            }
            if !self.sel_dirty {
                self.push_edit();
                self.sel_dirty = true;
            }
            match mode {
                SelMode::Move => {
                    let d = img_pt.sub(self.sel_origin);
                    self.shapes[idx].pts = self.sel_pts0.iter().map(|p| p.add(d)).collect();
                }
                SelMode::Handle(h) => self.resize_selected(idx, h, img_pt),
            }
            return true;
        }
        let Some(sh) = self.live.as_mut() else {
            return false;
        };
        if sh.kind.is_brush(&sh.style) {
            if sh
                .pts
                .last()
                .map(|p| p.manhattan(img_pt) >= 0.5)
                .unwrap_or(true)
            {
                sh.pts.push(img_pt);
            }
            return true;
        }
        let mut pt = img_pt;
        if shift && matches!(sh.kind, Tool::Rect | Tool::Ellipse) {
            pt = constrain_square(sh.pts[0], img_pt);
        }
        if let Some(last) = sh.pts.last_mut() {
            *last = pt;
        }
        true
    }

    fn resize_selected(&mut self, idx: usize, h: Handle, pt: Point) {
        let sh = &mut self.shapes[idx];
        if sh.kind == Tool::Number && self.sel_pts0.len() == 1 {
            let c = self.sel_pts0[0];
            let r = pt.manhattan(c).max(8.0);
            sh.style.width = (r * 2.0) as i32;
            sh.style.font_size = (r * 1.2) as i32;
            return;
        }
        if matches!(sh.kind, Tool::Pen | Tool::Marker | Tool::Line) && self.sel_pts0.len() > 2 {
            let xs = self.sel_pts0.iter().map(|p| p.x);
            let ys = self.sel_pts0.iter().map(|p| p.y);
            let x0 = xs.clone().fold(f32::MAX, f32::min);
            let x1 = xs.fold(f32::MIN, f32::max);
            let y0 = ys.clone().fold(f32::MAX, f32::min);
            let y1 = ys.fold(f32::MIN, f32::max);
            let b0 = Rect::from_xyxy(x0, y0, x1, y1);
            let c = b0.center();
            let dx0 = (b0.w.abs() / 2.0).max(1.0);
            let dy0 = (b0.h.abs() / 2.0).max(1.0);
            let sx = (pt.x - c.x).abs() / dx0;
            let sy = (pt.y - c.y).abs() / dy0;
            let s = ((sx + sy) / 2.0).max(0.05);
            sh.pts = self
                .sel_pts0
                .iter()
                .map(|p| c.add(p.sub(c).scale(s)))
                .collect();
            return;
        }
        let mut b = if self.sel_pts0.len() >= 2 {
            Rect::from_points(self.sel_pts0[0], *self.sel_pts0.last().unwrap()).normalized()
        } else {
            sh.bounds()
        };
        if h.has_left() {
            b.w += b.x - pt.x;
            b.x = pt.x;
        }
        if h.has_right() {
            b.w = pt.x - b.x;
        }
        if h.has_top() {
            b.h += b.y - pt.y;
            b.y = pt.y;
        }
        if h.has_bottom() {
            b.h = pt.y - b.y;
        }
        let b = b.normalized();
        if sh.kind == Tool::Text {
            sh.pts = vec![b.top_left()];
        } else {
            sh.pts = vec![b.top_left(), b.bottom_right()];
        }
    }

    pub fn release(&mut self, _img_pt: Point) -> bool {
        if self.sel_mode.is_some() || self.tool == Some(Tool::Select) {
            self.sel_mode = None;
            self.sel_pts0.clear();
            return true;
        }
        if self.live.is_some() {
            self.finish();
            return true;
        }
        false
    }

    pub fn selected_handle_points(&self) -> Vec<(Handle, Point)> {
        if let Some(i) = self.selected {
            if let Some(sh) = self.shapes.get(i) {
                return handle_points(sh.bounds().adjusted(-2.0, -2.0, 2.0, 2.0)).to_vec();
            }
        }
        Vec::new()
    }

    pub fn palette() -> [Rgba; 20] {
        palette_colors()
    }
}

fn near_rect_edge(p: Point, r: Rect, tol: f32) -> bool {
    let on_v = p.y >= r.y - tol && p.y <= r.bottom() + tol;
    let on_h = p.x >= r.x - tol && p.x <= r.right() + tol;
    (on_v && ((p.x - r.x).abs() <= tol || (p.x - r.right()).abs() <= tol))
        || (on_h && ((p.y - r.y).abs() <= tol || (p.y - r.bottom()).abs() <= tol))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_shape() -> Shape {
        Shape::new(
            Tool::Rect,
            vec![Point::new(40.0, 40.0), Point::new(200.0, 160.0)],
            Style::default(),
        )
    }

    #[test]
    fn hollow_hit() {
        let mut a = Annotator::new();
        a.add(rect_shape());
        assert!(a.hit_test(Point::new(100.0, 100.0)).is_none());
        assert_eq!(a.hit_test(Point::new(40.0, 80.0)), Some(0));
        a.shapes[0].style.filled = true;
        assert_eq!(a.hit_test(Point::new(100.0, 100.0)), Some(0));
    }

    #[test]
    fn drag_existing_with_drawing_tool_selects_and_moves() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.add(rect_shape());
        assert!(a.press(Point::new(40.0, 80.0), false));
        assert_eq!(a.selected, Some(0));
        a.move_to(Point::new(70.0, 100.0), false);
        a.release(Point::new(70.0, 100.0));
        assert_eq!(a.shapes.len(), 1);
        assert_eq!(a.shapes[0].pts[0], Point::new(70.0, 60.0));
    }

    #[test]
    fn drag_empty_with_drawing_tool_starts_new_shape() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.add(rect_shape());
        assert!(a.press(Point::new(10.0, 10.0), false));
        a.move_to(Point::new(25.0, 25.0), false);
        a.release(Point::new(25.0, 25.0));
        assert_eq!(a.shapes.len(), 2);
        assert_eq!(a.shapes[1].kind, Tool::Rect);
    }

    #[test]
    fn width_after_release_and_undo() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.add(rect_shape());
        a.selected = Some(0);
        a.adjust_width(2);
        assert_eq!(a.shapes[0].style.width, 5);
        assert_eq!(a.style(Tool::Rect).width, 5);
        a.undo();
        assert_eq!(a.shapes[0].style.width, 3);
        a.undo();
        assert!(a.shapes.is_empty());
    }

    #[test]
    fn drag_without_tool() {
        let mut a = Annotator::new();
        a.add(rect_shape());
        a.set_tool(None);
        assert!(a.press(Point::new(40.0, 80.0), false));
        assert_eq!(a.selected, Some(0));
        a.move_to(Point::new(60.0, 80.0), false);
        a.release(Point::new(60.0, 80.0));
        assert_eq!(a.shapes[0].pts[0], Point::new(60.0, 40.0));
    }

    #[test]
    fn mosaic_does_not_drag_existing() {
        let mut a = Annotator::new();
        a.add(rect_shape());
        a.set_tool(Some(Tool::Mosaic));
        a.press(Point::new(40.0, 80.0), false);
        assert!(a.live.as_ref().unwrap().kind == Tool::Mosaic);
        assert!(a.sel_mode.is_none());
        a.finish();
    }

    #[test]
    fn style_edit_undo() {
        let mut a = Annotator::new();
        a.add(rect_shape());
        a.selected = Some(0);
        a.set_style_value(Tool::Rect, |s| s.filled = true);
        assert!(a.shapes[0].style.filled);
        a.undo();
        assert!(!a.shapes[0].style.filled);
    }

    #[test]
    fn eraser_hides_hit() {
        let mut a = Annotator::new();
        let mut r = rect_shape();
        r.style.filled = true;
        a.add(r);
        a.add(Shape::new(
            Tool::Eraser,
            vec![Point::new(40.0, 80.0), Point::new(200.0, 80.0)],
            Style {
                width: 16,
                mode: DrawMode::Brush,
                ..Style::default()
            },
        ));
        assert!(a.hit_test(Point::new(100.0, 80.0)).is_none());
    }

    #[test]
    fn numbered_markers_renumber_on_undo() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Number));
        a.press(Point::new(10.0, 10.0), false);
        a.press(Point::new(30.0, 10.0), false);
        assert_eq!(a.shapes.len(), 2);
        assert_eq!(a.shapes[0].number, 1);
        assert_eq!(a.shapes[1].number, 2);
        a.undo();
        assert_eq!(a.shapes.len(), 1);
        assert_eq!(a.shapes[0].number, 1);
        assert_eq!(a.next_number, 2);
    }

    #[test]
    fn draw_rect_shift_square() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.press(Point::new(0.0, 0.0), false);
        a.move_to(Point::new(40.0, 10.0), true);
        a.release(Point::new(40.0, 10.0));
        let b = a.shapes[0].bounds();
        assert!((b.w - b.h).abs() < 0.01);
    }

    #[test]
    fn undo_stack_limit() {
        let mut u = UndoStack::default();
        for i in 0..60 {
            u.push(vec![Shape::new(
                Tool::Rect,
                vec![Point::new(i as f32, 0.0)],
                Style::default(),
            )]);
        }
        assert_eq!(u.undo_len(), UndoStack::LIMIT);
    }

    #[test]
    fn text_commit() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Text));
        a.press(Point::new(8.0, 8.0), false);
        a.type_text('H');
        a.type_text('i');
        a.type_text('\u{ffe3}');
        a.type_text('\u{2014}');
        a.commit_text(false);
        assert_eq!(a.shapes.len(), 1);
        assert_eq!(a.shapes[0].text, "Hi");
        assert!(a.text_edit.is_none());
        assert_eq!(a.width_target, Some(0));
    }

    #[test]
    fn option_target_prefers_selected_then_edit_then_width() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.add(rect_shape());
        a.add(Shape::new(
            Tool::Ellipse,
            vec![Point::new(10.0, 10.0), Point::new(30.0, 30.0)],
            Style::default(),
        ));
        a.selected = None;
        a.edit_target = None;
        assert_eq!(a.option_target(), Some(1)); // last add → width_target
        a.edit_target = Some(0);
        assert_eq!(a.option_target(), Some(0));
        a.selected = Some(1);
        assert_eq!(a.option_target(), Some(1));
        a.live = Some(Shape::new(
            Tool::Pen,
            vec![Point::new(1.0, 1.0)],
            Style::default(),
        ));
        assert!(a.option_target().is_none());
        a.adjust_width(2);
        assert_eq!(a.live.as_ref().unwrap().style.width, 5);
        assert_eq!(a.option_width(), 5);
    }

    #[test]
    fn option_width_follows_selected_not_tool_default() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.add(rect_shape());
        a.selected = Some(0);
        a.shapes[0].style.width = 9;
        assert_eq!(a.style(Tool::Rect).width, 3);
        assert_eq!(a.option_width(), 9);
        a.adjust_width(1);
        assert_eq!(a.option_width(), 10);
    }

    #[test]
    fn wheel_accumulates_120_and_merges_undo() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.add(rect_shape());
        a.selected = Some(0);
        assert!(!a.wheel(60));
        assert_eq!(a.shapes[0].style.width, 3);
        assert!(a.wheel(60));
        assert_eq!(a.shapes[0].style.width, 4);
        assert!(a.wheel(120));
        assert_eq!(a.shapes[0].style.width, 5);
        // same target within 0.5s → one undo snapshot
        assert_eq!(a.undo_len(), 2); // add + first merged wheel
        a.undo();
        assert_eq!(a.shapes[0].style.width, 3);
    }

    #[test]
    fn wheel_without_selection_uses_width_target() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.add(rect_shape());
        a.selected = None;
        a.edit_target = None;
        assert_eq!(a.width_target, Some(0));
        assert!(a.wheel(120));
        assert_eq!(a.shapes[0].style.width, 4);
    }

    #[test]
    fn text_editor_rect_and_cancel() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Text));
        a.press(Point::new(20.0, 30.0), false);
        a.type_text('A');
        let r = a.text_edit_rect().expect("editor");
        assert!(r.w >= 80.0);
        a.commit_text(true);
        assert!(a.shapes.is_empty());
        assert!(a.text_edit.is_none());
    }

    #[test]
    fn picking_arrow_does_not_convert_last_line() {
        let mut a = Annotator::new();
        a.set_style_default(Tool::Line, |s| s.arrow = false);
        a.set_tool(Some(Tool::Line));
        a.press(Point::new(0.0, 0.0), false);
        a.move_to(Point::new(40.0, 0.0), false);
        a.release(Point::new(40.0, 0.0));
        assert_eq!(a.shapes.len(), 1);
        assert!(!a.shapes[0].style.arrow);
        a.set_tool(Some(Tool::Line));
        a.set_style_default(Tool::Line, |s| s.arrow = true);
        assert!(!a.shapes[0].style.arrow);
        assert!(a.style(Tool::Line).arrow);
    }

    #[test]
    fn click_shape_edge_selects_even_with_draw_tool() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Line));
        a.set_style_default(Tool::Line, |s| s.arrow = false);
        a.press(Point::new(0.0, 0.0), false);
        a.move_to(Point::new(40.0, 0.0), false);
        a.release(Point::new(40.0, 0.0));
        a.set_tool(Some(Tool::Text));
        assert!(a.press(Point::new(20.0, 0.0), false));
        assert_eq!(a.selected, Some(0));
        assert!(a.text_edit.is_none());
        assert!(a.live.is_none());
    }

    #[test]
    fn new_tool_on_empty_starts_draw() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Line));
        a.press(Point::new(0.0, 0.0), false);
        a.move_to(Point::new(30.0, 0.0), false);
        a.release(Point::new(30.0, 0.0));
        a.set_tool(Some(Tool::Pen));
        assert!(a.press(Point::new(15.0, 20.0), false));
        assert!(a.live.as_ref().map(|s| s.kind) == Some(Tool::Pen));
        assert_eq!(a.shapes.len(), 1);
    }

    #[test]
    fn text_first_click_opens_editor_second_empty_relocates() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Text));
        assert!(a.press(Point::new(20.0, 30.0), false));
        assert!(a.text_edit.is_some());
        assert!(a.shapes.is_empty());
        assert!(a.press(Point::new(40.0, 50.0), false));
        let ed = a.text_edit.as_ref().expect("still editing");
        assert!(ed.text.is_empty());
        assert_eq!(ed.img_pt, Point::new(40.0, 50.0));
        assert!(a.shapes.is_empty());
    }

    #[test]
    fn wheel_at_selects_hit_shape() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.press(Point::new(10.0, 10.0), false);
        a.move_to(Point::new(40.0, 40.0), false);
        a.release(Point::new(40.0, 40.0));
        a.set_tool(Some(Tool::Rect));
        a.selected = None;
        a.width_target = None;
        assert!(a.wheel_at(Point::new(10.0, 10.0), 120));
        assert_eq!(a.selected, Some(0));
        assert_eq!(a.shapes[0].style.width, 4);
    }

    /// Overlay `handle` MouseInput / CursorMoved / MouseInput — not QueryPointer recover.
    #[test]
    fn event_path_draws_rect_ellipse_arrow() {
        for (tool, arrow) in [
            (Tool::Rect, false),
            (Tool::Ellipse, false),
            (Tool::Line, true),
        ] {
            let mut a = Annotator::new();
            a.set_tool(Some(tool));
            if arrow {
                a.set_style_default(Tool::Line, |s| s.arrow = true);
            }
            assert!(a.press(Point::new(20.0, 20.0), false), "{tool:?} press");
            assert!(a.live.is_some(), "{tool:?} live after press");
            assert!(a.move_to(Point::new(90.0, 70.0), false), "{tool:?} move");
            assert!(a.release(Point::new(90.0, 70.0)), "{tool:?} release");
            assert_eq!(a.shapes.len(), 1, "{tool:?}");
            assert_eq!(a.shapes[0].kind, tool);
            if arrow {
                assert!(a.shapes[0].style.arrow, "arrow flag");
            }
            let b = a.shapes[0].bounds();
            assert!(b.w >= 60.0 && b.h >= 40.0, "{tool:?} bounds {b:?}");
        }
    }

    #[test]
    fn empty_recover_release_does_not_block_event_path_stroke() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.press(Point::new(10.0, 10.0), false);
        a.release(Point::new(10.0, 10.0));
        assert!(a.shapes.is_empty());
        assert!(a.live.is_none());
        a.press(Point::new(10.0, 10.0), false);
        a.move_to(Point::new(50.0, 40.0), false);
        a.release(Point::new(50.0, 40.0));
        assert_eq!(a.shapes.len(), 1);
        let b = a.shapes[0].bounds();
        assert!(b.w >= 30.0 && b.h >= 20.0);
    }
}

//! PinShot's own icons (ported from `pinshot/icons.py`). No third-party assets.

use crate::color::Rgba;
use crate::geom::Rect;
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

const FG: Rgba = Rgba {
    r: 50,
    g: 50,
    b: 50,
    a: 255,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconKind {
    Rect,
    Ellipse,
    RoundRect,
    Line,
    /// Grouped line/arrow toolbar entry (polyline + small head). Independent of
    /// the straight-line / arrow presets in the dropdown.
    LineGroup,
    /// Toggle chip: short line beside a small arrow.
    LineArrowFlip,
    Arrow,
    ArrowThick,
    ArrowHollow,
    Pen,
    Marker,
    Mosaic,
    Text,
    Number,
    Undo,
    Redo,
    Close,
    Pin,
    Save,
    Copy,
    Select,
    Eraser,
    Check,
    App,
    Ocr,
    Translate,
    Fill,
    Outline,
    Dash,
    Dot,
    Blur,
    Bold,
    Italic,
    TextOutline,
}

impl IconKind {
    pub fn from_id(s: &str) -> Option<Self> {
        Some(match s {
            "rect" => Self::Rect,
            "ellipse" => Self::Ellipse,
            "shape" | "roundrect" => Self::RoundRect,
            "line" => Self::Line,
            "linegroup" | "polyline" | "line_arrow" => Self::LineGroup,
            "linearrowflip" | "line_arrow_flip" => Self::LineArrowFlip,
            "arrow" => Self::Arrow,
            "arrow_thick" | "thickarrow" => Self::ArrowThick,
            "arrow_hollow" | "hollowarrow" => Self::ArrowHollow,
            "pen" | "pencil" => Self::Pen,
            "marker" => Self::Marker,
            "mosaic" => Self::Mosaic,
            "text" => Self::Text,
            "number" => Self::Number,
            "undo" => Self::Undo,
            "redo" => Self::Redo,
            "close" | "cancel" => Self::Close,
            "pin" => Self::Pin,
            "save" => Self::Save,
            "copy" => Self::Copy,
            "select" => Self::Select,
            "eraser" => Self::Eraser,
            "check" => Self::Check,
            "app" => Self::App,
            "ocr" => Self::Ocr,
            "translate" => Self::Translate,
            "fill" => Self::Fill,
            "outline" => Self::Outline,
            "dash" => Self::Dash,
            "dot" => Self::Dot,
            "blur" => Self::Blur,
            "bold" => Self::Bold,
            "italic" => Self::Italic,
            "text_outline" => Self::TextOutline,
            _ => return None,
        })
    }
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c.to_skia());
    p.anti_alias = true;
    p
}

fn stroke(width: f32) -> Stroke {
    Stroke {
        width,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Stroke::default()
    }
}

fn line_path(a: (f32, f32), b: (f32, f32)) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(a.0, a.1);
    pb.line_to(b.0, b.1);
    pb.finish()
}

/// 4-pointed sparkle (Snipping Tool "Text actions" accent).
fn fill_sparkle(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Rgba, t: Transform) {
    let mut pb = PathBuilder::new();
    let inner = r * 0.32;
    pb.move_to(cx, cy - r);
    pb.line_to(cx + inner, cy - inner);
    pb.line_to(cx + r, cy);
    pb.line_to(cx + inner, cy + inner);
    pb.line_to(cx, cy + r);
    pb.line_to(cx - inner, cy + inner);
    pb.line_to(cx - r, cy);
    pb.line_to(cx - inner, cy - inner);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, &paint(color), FillRule::Winding, t, None);
    }
}

fn arrow_head(
    pixmap: &mut Pixmap,
    a: (f32, f32),
    b: (f32, f32),
    head: f32,
    color: Rgba,
    t: Transform,
) {
    arrow_head_ex(pixmap, a, b, head, head * 0.55, color, t, false);
}

fn arrow_geom(
    a: (f32, f32),
    b: (f32, f32),
    head: f32,
    half_w: f32,
) -> Option<((f32, f32), (f32, f32), (f32, f32))> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return None;
    }
    let ux = dx / len;
    let uy = dy / len;
    let vx = -uy;
    let vy = ux;
    let left = (b.0 - head * ux + half_w * vx, b.1 - head * uy + half_w * vy);
    let right = (b.0 - head * ux - half_w * vx, b.1 - head * uy - half_w * vy);
    let base = (b.0 - head * ux, b.1 - head * uy);
    Some((left, right, base))
}

fn fill_sharp_icon(
    pixmap: &mut Pixmap,
    a: (f32, f32),
    b: (f32, f32),
    color: Rgba,
    t: Transform,
) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let vx = -uy;
    let vy = ux;
    let tail_hw = 0.28;
    let head_len = (len * 0.38).max(5.2);
    let barb_hw = 2.15;
    let neck_hw = 0.52;
    let nx = b.0 - ux * head_len;
    let ny = b.1 - uy * head_len;
    let mut pb = PathBuilder::new();
    pb.move_to(a.0 + vx * tail_hw, a.1 + vy * tail_hw);
    pb.line_to(nx + vx * neck_hw, ny + vy * neck_hw);
    pb.line_to(nx + vx * barb_hw, ny + vy * barb_hw);
    pb.line_to(b.0, b.1);
    pb.line_to(nx - vx * barb_hw, ny - vy * barb_hw);
    pb.line_to(nx - vx * neck_hw, ny - vy * neck_hw);
    pb.line_to(a.0 - vx * tail_hw, a.1 - vy * tail_hw);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, &paint(color), FillRule::Winding, t, None);
    }
}

fn arrow_head_ex(
    pixmap: &mut Pixmap,
    a: (f32, f32),
    b: (f32, f32),
    head: f32,
    half_w: f32,
    color: Rgba,
    t: Transform,
    hollow: bool,
) {
    let Some((left, right, _)) = arrow_geom(a, b, head, half_w) else {
        return;
    };
    if hollow {
        let mut pb = PathBuilder::new();
        pb.move_to(left.0, left.1);
        pb.line_to(b.0, b.1);
        pb.line_to(right.0, right.1);
        if let Some(path) = pb.finish() {
            let mut st = stroke(1.6);
            st.line_cap = tiny_skia::LineCap::Round;
            st.line_join = tiny_skia::LineJoin::Round;
            pixmap.stroke_path(&path, &paint(color), &st, t, None);
        }
    } else {
        let mut pb = PathBuilder::new();
        pb.move_to(b.0, b.1);
        pb.line_to(left.0, left.1);
        pb.line_to(right.0, right.1);
        pb.close();
        if let Some(path) = pb.finish() {
            pixmap.fill_path(&path, &paint(color), FillRule::Winding, t, None);
        }
    }
}

/// Draw a 20×20 logical icon into `cell` (icons.py coordinates).
pub fn draw_icon(pixmap: &mut Pixmap, kind: IconKind, cell: Rect, color: Option<Rgba>) {
    let color = color.unwrap_or(FG);
    let s = cell.w.min(cell.h) / 20.0;
    if s <= 0.0 {
        return;
    }
    let t = Transform::from_translate(cell.x, cell.y).pre_scale(s, s);
    let p = paint(color);
    let st = stroke(1.6);

    match kind {
        IconKind::Bold => {
            let mut pb = PathBuilder::new();
            pb.move_to(4.0, 3.0);
            pb.line_to(10.0, 3.0);
            pb.cubic_to(17.0, 3.0, 17.0, 9.0, 12.0, 9.8);
            pb.cubic_to(18.0, 10.0, 18.0, 17.0, 10.0, 17.0);
            pb.line_to(4.0, 17.0);
            pb.close();
            pb.move_to(7.0, 5.5);
            pb.line_to(10.0, 5.5);
            pb.cubic_to(13.5, 5.5, 13.5, 8.5, 10.0, 8.5);
            pb.line_to(7.0, 8.5);
            pb.close();
            pb.move_to(7.0, 11.0);
            pb.line_to(10.0, 11.0);
            pb.cubic_to(14.5, 11.0, 14.5, 14.5, 10.0, 14.5);
            pb.line_to(7.0, 14.5);
            pb.close();
            if let Some(path) = pb.finish() {
                pixmap.fill_path(&path, &p, FillRule::EvenOdd, t, None);
            }
        }
        IconKind::Italic => {
            for (a, b) in [((8.0, 3.0), (15.0, 3.0)), ((12.0, 3.0), (8.0, 17.0)), ((5.0, 17.0), (12.0, 17.0))] {
                if let Some(path) = line_path(a, b) {
                    pixmap.stroke_path(&path, &p, &stroke(2.0), t, None);
                }
            }
        }
        IconKind::TextOutline => {
            let mut pb = PathBuilder::new();
            pb.move_to(3.0, 17.0);
            for (x, y) in [(8.0, 3.0), (12.0, 3.0), (17.0, 17.0), (13.0, 17.0),
                (11.5, 13.0), (8.5, 13.0), (7.0, 17.0)] { pb.line_to(x, y); }
            pb.close();
            pb.move_to(9.0, 10.0);
            pb.line_to(11.0, 10.0);
            pb.line_to(10.0, 6.5);
            pb.close();
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &p, &stroke(1.1), t, None);
            }
        }
        IconKind::Rect => {
            if let Some(r) = tiny_skia::Rect::from_xywh(3.5, 5.5, 13.0, 9.0) {
                pixmap.stroke_path(&PathBuilder::from_rect(r), &p, &st, t, None);
            }
        }
        IconKind::Ellipse => {
            if let Some(r) = tiny_skia::Rect::from_xywh(3.0, 5.0, 14.0, 10.0) {
                let path = PathBuilder::from_oval(r).unwrap();
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
        }
        IconKind::RoundRect => {
            // Combined rect/ellipse entry: rectangle with an ellipse overlapping
            // its bottom-right.
            if let Some(r) = tiny_skia::Rect::from_xywh(2.6, 2.8, 11.6, 9.2) {
                pixmap.stroke_path(&PathBuilder::from_rect(r), &p, &st, t, None);
            }
            if let Some(r) = tiny_skia::Rect::from_xywh(6.6, 8.6, 11.6, 8.0) {
                if let Some(path) = PathBuilder::from_oval(r) {
                    pixmap.stroke_path(&path, &p, &st, t, None);
                }
            }
        }
        IconKind::Line => {
            // Straight shaft, no head — first item in the arrow menu.
            if let Some(path) = line_path((3.5, 16.5), (16.5, 3.5)) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
        }
        IconKind::LineGroup => {
            // Entry glyph only: 参考工具 折线 + small filled head.
            let a = (2.4, 15.8);
            let b = (8.0, 10.0);
            let c = (11.4, 13.4);
            let tip = (17.6, 3.6);
            let (head, half_w) = (4.4, 2.0);
            let shaft_end = arrow_geom(c, tip, head, half_w).map(|g| g.2).unwrap_or(tip);
            let mut pb = PathBuilder::new();
            pb.move_to(a.0, a.1);
            pb.line_to(b.0, b.1);
            pb.line_to(c.0, c.1);
            pb.line_to(shaft_end.0, shaft_end.1);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            arrow_head_ex(pixmap, c, tip, head, half_w, color, t, false);
        }
        IconKind::LineArrowFlip => {
            // Line | arrow, same baseline — the flip chip.
            if let Some(path) = line_path((1.4, 10.0), (7.6, 10.0)) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            fill_sharp_icon(pixmap, (9.2, 10.0), (18.6, 10.0), color, t);
        }
        IconKind::Arrow => {
            fill_sharp_icon(pixmap, (3.5, 16.5), (16.5, 3.5), color, t);
        }
        IconKind::ArrowThick => {
            let a = (4.0, 16.0);
            let b = (16.2, 3.8);
            let (head, half_w) = (6.2, 2.7);
            let shaft_end = arrow_geom(a, b, head, half_w).map(|g| g.2).unwrap_or(b);
            if let Some(path) = line_path(a, shaft_end) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            arrow_head_ex(pixmap, a, b, head, half_w, color, t, false);
        }
        IconKind::ArrowHollow => {
            if let Some(path) = line_path((4.0, 16.0), (16.0, 4.0)) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            arrow_head_ex(pixmap, (4.0, 16.0), (16.0, 4.0), 6.4, 3.3, color, t, true);
        }
        IconKind::Pen => {
            // Pencil: two parallel sides, filled triangular tip, filled eraser.
            let tip = (17.2f32, 2.3f32);
            let back = (2.7f32, 17.8f32);
            let dx = back.0 - tip.0;
            let dy = back.1 - tip.1;
            let len = (dx * dx + dy * dy).sqrt();
            let ux = dx / len;
            let uy = dy / len;
            let vx = -uy;
            let vy = ux;
            let hw = 1.55;
            let lead = 3.5;
            let erase = 2.4;
            let wood = (tip.0 + ux * lead, tip.1 + uy * lead);
            let neck = (tip.0 + ux * (len - erase), tip.1 + uy * (len - erase));
            let wa = (wood.0 + vx * hw, wood.1 + vy * hw);
            let wb = (wood.0 - vx * hw, wood.1 - vy * hw);
            let na = (neck.0 + vx * hw, neck.1 + vy * hw);
            let nb = (neck.0 - vx * hw, neck.1 - vy * hw);
            let ba = (back.0 + vx * hw, back.1 + vy * hw);
            let bb = (back.0 - vx * hw, back.1 - vy * hw);
            if let Some(path) = line_path(wa, na) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            if let Some(path) = line_path(wb, nb) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            let mut wood_p = PathBuilder::new();
            wood_p.move_to(tip.0, tip.1);
            wood_p.line_to(wa.0, wa.1);
            wood_p.line_to(wb.0, wb.1);
            wood_p.close();
            if let Some(path) = wood_p.finish() {
                pixmap.fill_path(&path, &p, FillRule::Winding, t, None);
            }
            let mut eraser = PathBuilder::new();
            eraser.move_to(na.0, na.1);
            eraser.line_to(ba.0, ba.1);
            eraser.line_to(bb.0, bb.1);
            eraser.line_to(nb.0, nb.1);
            eraser.close();
            if let Some(path) = eraser.finish() {
                pixmap.fill_path(&path, &p, FillRule::Winding, t, None);
            }
        }
        IconKind::Marker => {
            // Highlighter: wider hollow tube, open chisel, solid back cap.
            let face = (16.9f32, 5.2f32);
            let back = (3.1f32, 16.6f32);
            let dx = face.0 - back.0;
            let dy = face.1 - back.1;
            let len = (dx * dx + dy * dy).sqrt();
            let ux = dx / len;
            let uy = dy / len;
            let vx = -uy;
            let vy = ux;
            let hw = 2.55;
            let nib_hw = 1.85;
            let chisel = 3.3;
            let cap = 2.6;
            let neck = (face.0 - ux * chisel, face.1 - uy * chisel);
            let cap_c = (back.0 + ux * cap, back.1 + uy * cap);
            let fa = (face.0 + vx * nib_hw, face.1 + vy * nib_hw);
            let fb = (face.0 - vx * nib_hw, face.1 - vy * nib_hw);
            let na = (neck.0 + vx * hw, neck.1 + vy * hw);
            let nb = (neck.0 - vx * hw, neck.1 - vy * hw);
            let ca = (cap_c.0 + vx * hw, cap_c.1 + vy * hw);
            let cb = (cap_c.0 - vx * hw, cap_c.1 - vy * hw);
            let ba = (back.0 + vx * hw * 0.72, back.1 + vy * hw * 0.72);
            let bb = (back.0 - vx * hw * 0.72, back.1 - vy * hw * 0.72);
            if let Some(path) = line_path(na, ca) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            if let Some(path) = line_path(nb, cb) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            let mut nib = PathBuilder::new();
            nib.move_to(na.0, na.1);
            nib.line_to(fa.0, fa.1);
            nib.line_to(fb.0, fb.1);
            nib.line_to(nb.0, nb.1);
            if let Some(path) = nib.finish() {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            if let Some(path) = line_path(na, nb) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            let mut cap_p = PathBuilder::new();
            cap_p.move_to(ca.0, ca.1);
            cap_p.line_to(ba.0, ba.1);
            cap_p.line_to(bb.0, bb.1);
            cap_p.line_to(cb.0, cb.1);
            cap_p.close();
            if let Some(path) = cap_p.finish() {
                pixmap.fill_path(&path, &p, FillRule::Winding, t, None);
            }
        }
        IconKind::Mosaic => {
            for i in 0..3 {
                for j in 0..3 {
                    if (i + j) % 2 == 0 {
                        if let Some(r) = tiny_skia::Rect::from_xywh(
                            4.0 + i as f32 * 4.0,
                            4.0 + j as f32 * 4.0,
                            4.0,
                            4.0,
                        ) {
                            pixmap.fill_rect(r, &p, t, None);
                        }
                    }
                }
            }
            if let Some(r) = tiny_skia::Rect::from_xywh(4.0, 4.0, 12.0, 12.0) {
                pixmap.stroke_path(&PathBuilder::from_rect(r), &p, &st, t, None);
            }
        }
        IconKind::Text => {
            // A simple "T" built from two strokes (no system font needed).
            if let Some(path) = line_path((5.0, 5.0), (15.0, 5.0)) {
                pixmap.stroke_path(&path, &p, &stroke(2.0), t, None);
            }
            if let Some(path) = line_path((10.0, 5.0), (10.0, 16.0)) {
                pixmap.stroke_path(&path, &p, &stroke(2.0), t, None);
            }
        }
        IconKind::Number => {
            if let Some(r) = tiny_skia::Rect::from_xywh(3.0, 3.0, 14.0, 14.0) {
                pixmap.fill_path(
                    &PathBuilder::from_oval(r).unwrap(),
                    &p,
                    FillRule::Winding,
                    t,
                    None,
                );
            }
            let ink = if color.lightness() > 140 {
                Rgba::rgb(20, 20, 20)
            } else {
                Rgba::WHITE
            };
            // Digit "1"
            if let Some(path) = line_path((10.0, 7.0), (10.0, 14.0)) {
                pixmap.stroke_path(&path, &paint(ink), &stroke(1.8), t, None);
            }
        }
        IconKind::Undo | IconKind::Redo => {
            // Flip 180° about the horizontal axis so the arrow sits on top
            // (↶ / ↷). Redo is then mirrored left-right.
            let mut tf = t.pre_translate(0.0, 20.0).pre_scale(1.0, -1.0);
            if matches!(kind, IconKind::Redo) {
                tf = tf.pre_translate(20.0, 0.0).pre_scale(-1.0, 1.0);
            }
            let top = 5.4;
            let bot = 14.6;
            let mid = (top + bot) * 0.5;
            let r = (bot - top) * 0.5;
            let cx = 11.2;
            let k = 0.5522847498 * r;
            let mut pb = PathBuilder::new();
            pb.move_to(4.8, top);
            pb.line_to(cx, top);
            pb.cubic_to(cx + k, top, cx + r, mid - k, cx + r, mid);
            pb.cubic_to(cx + r, mid + k, cx + k, bot, cx, bot);
            pb.line_to(6.6, bot);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &p, &stroke(2.0), tf, None);
            }
            arrow_head(pixmap, (8.8, bot), (3.4, bot), 4.8, color, tf);
        }
        IconKind::Close => {
            let red = Rgba::rgb(210, 50, 50);
            if let Some(path) = line_path((5.0, 5.0), (15.0, 15.0)) {
                pixmap.stroke_path(&path, &paint(red), &stroke(1.8), t, None);
            }
            if let Some(path) = line_path((15.0, 5.0), (5.0, 15.0)) {
                pixmap.stroke_path(&path, &paint(red), &stroke(1.8), t, None);
            }
        }
        IconKind::Pin => {
            if let Some(path) = line_path((10.0, 12.0), (10.0, 18.0)) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            if let Some(r) = tiny_skia::Rect::from_xywh(6.0, 3.0, 8.0, 7.0) {
                pixmap.stroke_path(&PathBuilder::from_rect(r), &p, &st, t, None);
            }
            if let Some(path) = line_path((4.0, 11.0), (16.0, 11.0)) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
        }
        IconKind::Save => {
            if let Some(path) = line_path((10.0, 3.0), (10.0, 12.0)) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            arrow_head(pixmap, (10.0, 3.0), (10.0, 13.0), 4.0, color, t);
            let mut pb = PathBuilder::new();
            pb.move_to(4.0, 12.0);
            pb.line_to(4.0, 16.0);
            pb.line_to(16.0, 16.0);
            pb.line_to(16.0, 12.0);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
        }
        IconKind::Copy => {
            if let Some(r) = tiny_skia::Rect::from_xywh(7.0, 3.0, 9.0, 11.0) {
                pixmap.stroke_path(&PathBuilder::from_rect(r), &p, &st, t, None);
            }
            let mut pb = PathBuilder::new();
            pb.move_to(5.0, 6.0);
            pb.line_to(4.0, 6.0);
            pb.line_to(4.0, 17.0);
            pb.line_to(12.0, 17.0);
            pb.line_to(12.0, 16.0);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
        }
        IconKind::Select => {
            if let Some(r) = tiny_skia::Rect::from_xywh(4.0, 4.0, 10.0, 10.0) {
                pixmap.stroke_path(&PathBuilder::from_rect(r), &p, &st, t, None);
            }
            if let Some(path) = line_path((14.0, 14.0), (17.0, 17.0)) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
        }
        IconKind::Eraser => {
            let mut pb = PathBuilder::new();
            pb.move_to(4.0, 13.0);
            pb.line_to(11.0, 5.0);
            pb.line_to(16.0, 10.0);
            pb.line_to(10.0, 16.0);
            pb.line_to(6.0, 16.0);
            pb.close();
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
        }
        IconKind::Check => {
            let green = Rgba::rgb(30, 150, 60);
            let mut pb = PathBuilder::new();
            pb.move_to(4.0, 10.0);
            pb.line_to(8.5, 14.5);
            pb.line_to(16.0, 5.0);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &paint(green), &stroke(2.2), t, None);
            }
        }
        IconKind::Ocr => {
            // Windows Snipping Tool "Text actions": page with lines + sparkle.
            if let Some(r) = tiny_skia::Rect::from_xywh(2.5, 4.0, 11.0, 13.0) {
                pixmap.stroke_path(&PathBuilder::from_rect(r), &p, &st, t, None);
            }
            for y in [7.5, 10.5, 13.5] {
                if let Some(path) = line_path((4.5, y), (11.5, y)) {
                    pixmap.stroke_path(&path, &p, &stroke(1.15), t, None);
                }
            }
            fill_sparkle(pixmap, 15.6, 5.0, 3.8, color, t);
            fill_sparkle(pixmap, 17.6, 9.2, 1.7, color, t);
        }
        IconKind::Translate => {
            // Latin A + CJK 口 — a two-script mark, never a T (Text) or Tₓ.
            if let Some(path) = line_path((3.5, 15.0), (7.0, 4.5)) {
                pixmap.stroke_path(&path, &p, &stroke(1.7), t, None);
            }
            if let Some(path) = line_path((7.0, 4.5), (10.5, 15.0)) {
                pixmap.stroke_path(&path, &p, &stroke(1.7), t, None);
            }
            if let Some(path) = line_path((5.0, 11.2), (8.8, 11.2)) {
                pixmap.stroke_path(&path, &p, &stroke(1.4), t, None);
            }
            if let Some(r) = tiny_skia::Rect::from_xywh(11.0, 5.5, 6.5, 10.0) {
                pixmap.stroke_path(&PathBuilder::from_rect(r), &p, &stroke(1.5), t, None);
            }
            if let Some(path) = line_path((11.0, 10.5), (17.5, 10.5)) {
                pixmap.stroke_path(&path, &p, &stroke(1.3), t, None);
            }
        }
        IconKind::Fill => {
            if let Some(r) = tiny_skia::Rect::from_xywh(4.0, 5.0, 12.0, 10.0) {
                pixmap.fill_rect(r, &p, t, None);
            }
        }
        IconKind::Outline => {
            if let Some(r) = tiny_skia::Rect::from_xywh(4.0, 5.0, 12.0, 10.0) {
                pixmap.stroke_path(&PathBuilder::from_rect(r), &p, &st, t, None);
            }
        }
        IconKind::Dash => {
            for (a, b) in [((4.0, 16.0), (8.0, 12.0)), ((11.0, 9.0), (16.0, 4.0))] {
                if let Some(path) = line_path(a, b) {
                    pixmap.stroke_path(&path, &p, &st, t, None);
                }
            }
        }
        IconKind::Dot => {
            for (x, y) in [(5.0, 15.0), (10.0, 10.0), (15.0, 5.0)] {
                if let Some(r) = tiny_skia::Rect::from_xywh(x - 1.2, y - 1.2, 2.4, 2.4) {
                    pixmap.fill_path(
                        &PathBuilder::from_oval(r).unwrap(),
                        &p,
                        FillRule::Winding,
                        t,
                        None,
                    );
                }
            }
        }
        IconKind::Blur => {
            if let Some(r) = tiny_skia::Rect::from_xywh(4.0, 4.0, 12.0, 12.0) {
                pixmap.stroke_path(&PathBuilder::from_rect(r), &p, &stroke(1.0), t, None);
            }
            let soft = paint(color.with_alpha(90));
            if let Some(r) = tiny_skia::Rect::from_xywh(6.0, 6.0, 8.0, 8.0) {
                pixmap.fill_rect(r, &soft, t, None);
            }
        }
        IconKind::App => {
            if let Some(r) = tiny_skia::Rect::from_xywh(1.0, 1.0, 18.0, 18.0) {
                pixmap.fill_rect(r, &paint(Rgba::BLUE), t, None);
            }
            pixmap.fill_rect(
                tiny_skia::Rect::from_xywh(5.0, 5.0, 10.0, 7.0).unwrap(),
                &paint(Rgba::WHITE),
                t,
                None,
            );
            pixmap.fill_rect(
                tiny_skia::Rect::from_xywh(9.0, 12.0, 2.0, 4.0).unwrap(),
                &paint(Rgba::WHITE),
                t,
                None,
            );
        }
    }
}

/// App icon (icons.py `app_image`): blue rounded tile, white crop corners, yellow pin.
pub fn draw_app_icon(size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size, size).expect("icon pixmap");
    let s = size as f32 / 32.0;
    let t = Transform::from_scale(s, s);
    if let Some(r) = tiny_skia::Rect::from_xywh(1.0, 1.0, 30.0, 30.0) {
        pm.fill_rect(r, &paint(Rgba::BLUE), t, None);
    }
    let mut st = stroke((2.6_f32).max(1.0 / s));
    st.line_cap = tiny_skia::LineCap::Square;
    st.line_join = tiny_skia::LineJoin::Miter;
    let white = paint(Rgba::WHITE);
    let a = 7.0;
    let b = 25.0;
    let l = 6.0;
    for (x, y, dx, dy) in [
        (a, a, 1.0, 1.0),
        (b, a, -1.0, 1.0),
        (a, b, 1.0, -1.0),
        (b, b, -1.0, -1.0),
    ] {
        if let Some(path) = line_path((x, y), (x + dx * l, y)) {
            pm.stroke_path(&path, &white, &st, t, None);
        }
        if let Some(path) = line_path((x, y), (x, y + dy * l)) {
            pm.stroke_path(&path, &white, &st, t, None);
        }
    }
    if let Some(r) = tiny_skia::Rect::from_xywh(16.0 - 3.6, 16.0 - 3.6, 7.2, 7.2) {
        pm.fill_path(
            &PathBuilder::from_oval(r).unwrap(),
            &paint(Rgba::MARKER_YELLOW),
            FillRule::Winding,
            t,
            None,
        );
    }
    pm
}

pub fn app_icon_rgba(size: u32) -> (u32, u32, Vec<u8>) {
    let pm = draw_app_icon(size);
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for px in pm.pixels() {
        let c = px.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    (size, size, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_icon_is_opaque_blue_center() {
        let pm = draw_app_icon(32);
        let c = pm.pixel(16, 16).unwrap().demultiply();
        // yellow pin in the center
        assert!(c.red() > 200 && c.green() > 180);
        assert_eq!(pm.width(), 32);
    }

    #[test]
    fn every_toolbar_icon_paints() {
        let mut pm = Pixmap::new(24, 24).unwrap();
        for kind in [
            IconKind::Rect,
            IconKind::Ellipse,
            IconKind::RoundRect,
            IconKind::Line,
            IconKind::LineGroup,
            IconKind::LineArrowFlip,
            IconKind::Arrow,
            IconKind::ArrowThick,
            IconKind::ArrowHollow,
            IconKind::Pen,
            IconKind::Marker,
            IconKind::Mosaic,
            IconKind::Text,
            IconKind::Number,
            IconKind::Undo,
            IconKind::Redo,
            IconKind::Close,
            IconKind::Pin,
            IconKind::Save,
            IconKind::Copy,
            IconKind::Ocr,
            IconKind::Translate,
            IconKind::Fill,
            IconKind::Outline,
            IconKind::Eraser,
            IconKind::Bold,
            IconKind::Italic,
            IconKind::TextOutline,
        ] {
            pm.fill(tiny_skia::Color::TRANSPARENT);
            draw_icon(&mut pm, kind, Rect::new(2.0, 2.0, 20.0, 20.0), None);
            let painted = pm.pixels().iter().any(|p| p.alpha() > 0);
            assert!(painted, "{kind:?} produced an empty icon");
            if matches!(kind, IconKind::Bold | IconKind::Italic | IconKind::TextOutline) {
                if let Ok(dir) = std::env::var("PINSHOT_DUMP_ICONS") {
                    let dir = std::path::PathBuf::from(dir);
                    let _ = std::fs::create_dir_all(&dir);
                    let mut preview = Pixmap::new(120, 120).unwrap();
                    preview.fill(tiny_skia::Color::WHITE);
                    draw_icon(&mut preview, kind, Rect::new(10.0, 10.0, 100.0, 100.0), None);
                    preview.save_png(dir.join(format!("{kind:?}.png"))).unwrap();
                }
            }
        }
    }

    #[test]
    fn undo_and_redo_are_reply_arrows() {
        let cell = Rect::new(2.0, 2.0, 20.0, 20.0);
        let mut undo = Pixmap::new(24, 24).unwrap();
        let mut redo = Pixmap::new(24, 24).unwrap();
        draw_icon(&mut undo, IconKind::Undo, cell, None);
        draw_icon(&mut redo, IconKind::Redo, cell, None);
        assert_ne!(
            undo.data(),
            redo.data(),
            "Undo and Redo must be distinct icons"
        );

        let ink = |pm: &Pixmap, x: u32, y: u32| pm.pixel(x, y).unwrap().alpha() > 40;
        // Undo: left-pointing head on the top, curve on the right.
        assert!(ink(&undo, 5, 7), "undo arrow tip should sit top-left");
        assert!(ink(&undo, 18, 12), "undo should curve on the right");
        assert!(
            !ink(&undo, 18, 18),
            "undo should not put the arrow in the bottom-right"
        );
        // Redo: right-pointing head on the top, curve on the left.
        assert!(ink(&redo, 18, 7), "redo arrow tip should sit top-right");
        assert!(ink(&redo, 5, 12), "redo should curve on the left");
        assert!(
            !ink(&redo, 5, 18),
            "redo should not put the arrow in the bottom-left"
        );
        if let Ok(dir) = std::env::var("PINSHOT_DUMP_ICONS") {
            let dir = std::path::PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            dump_icon_preview(&dir.join("undo.png"), IconKind::Undo);
            dump_icon_preview(&dir.join("redo.png"), IconKind::Redo);
        }
    }

    fn dump_icon_preview(path: &std::path::Path, kind: IconKind) {
        let mut pm = Pixmap::new(80, 80).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(36, 36, 36, 255));
        draw_icon(
            &mut pm,
            kind,
            Rect::new(8.0, 8.0, 64.0, 64.0),
            Some(Rgba::rgb(230, 230, 230)),
        );
        if let Ok(png) = pm.encode_png() {
            let _ = std::fs::write(path, png);
        }
    }

    #[test]
    fn shape_group_icon_is_rect_over_ellipse() {
        let cell = Rect::new(2.0, 2.0, 20.0, 20.0);
        let ink = |pm: &Pixmap, x: u32, y: u32| pm.pixel(x, y).unwrap().alpha() > 40;
        let mut grouped = Pixmap::new(24, 24).unwrap();
        let mut rect = Pixmap::new(24, 24).unwrap();
        let mut ellipse = Pixmap::new(24, 24).unwrap();
        draw_icon(&mut grouped, IconKind::RoundRect, cell, None);
        draw_icon(&mut rect, IconKind::Rect, cell, None);
        draw_icon(&mut ellipse, IconKind::Ellipse, cell, None);
        assert_ne!(grouped.data(), rect.data());
        assert_ne!(grouped.data(), ellipse.data());
        // Cell origin (2,2). Rect top-left; flattened ellipse bottom-right.
        assert!(ink(&grouped, 5, 5), "rectangle stroke at top-left");
        assert!(ink(&grouped, 19, 16), "ellipse stroke at bottom-right");
        assert!(!ink(&grouped, 6, 20), "bottom-left is not a circle");
        if let Ok(dir) = std::env::var("PINSHOT_DUMP_ICONS") {
            let dir = std::path::PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            dump_icon_preview(&dir.join("shape-group.png"), IconKind::RoundRect);
        }
    }

    #[test]
    fn line_icon_has_no_arrow_head() {
        let cell = Rect::new(2.0, 2.0, 20.0, 20.0);
        let mut line = Pixmap::new(24, 24).unwrap();
        let mut arrow = Pixmap::new(24, 24).unwrap();
        draw_icon(&mut line, IconKind::Line, cell, None);
        draw_icon(&mut arrow, IconKind::Arrow, cell, None);
        assert_ne!(
            line.data(),
            arrow.data(),
            "Line and Arrow must be distinct icons"
        );
        let ink = |pm: &Pixmap, x: u32, y: u32| pm.pixel(x, y).unwrap().alpha() > 40;
        assert!(ink(&line, 5, 17), "line starts at the bottom-left");
        assert!(ink(&line, 18, 6), "line ends at the top-right");
        assert!(
            !ink(&line, 9, 8),
            "straight line has no polyline kink above the shaft"
        );
        assert!(
            !ink(&line, 19, 3),
            "straight line has no arrow head at the tip"
        );
        let mut grouped = Pixmap::new(24, 24).unwrap();
        draw_icon(&mut grouped, IconKind::LineGroup, cell, None);
        assert_ne!(
            line.data(),
            grouped.data(),
            "toolbar entry is the polyline glyph, not the straight line"
        );
        assert!(ink(&grouped, 18, 6), "entry keeps a small arrow at the tip");
        assert!(
            ink(&grouped, 10, 12),
            "entry polyline has a corner"
        );
        let mut flip = Pixmap::new(24, 24).unwrap();
        draw_icon(&mut flip, IconKind::LineArrowFlip, cell, None);
        assert!(ink(&flip, 5, 12), "flip chip has a line on the left");
        assert!(ink(&flip, 19, 12), "flip chip has an arrow on the right");
        assert_ne!(flip.data(), grouped.data());
        let mut thick = Pixmap::new(24, 24).unwrap();
        let mut hollow = Pixmap::new(24, 24).unwrap();
        draw_icon(&mut thick, IconKind::ArrowThick, cell, None);
        draw_icon(&mut hollow, IconKind::ArrowHollow, cell, None);
        assert_ne!(arrow.data(), thick.data(), "thin and thick arrows differ");
        assert_ne!(
            thick.data(),
            hollow.data(),
            "thick and hollow arrows differ"
        );
    }

    #[test]
    fn pen_and_marker_are_tool_glyphs() {
        let cell = Rect::new(2.0, 2.0, 20.0, 20.0);
        let ink = |pm: &Pixmap, x: u32, y: u32| pm.pixel(x, y).unwrap().alpha() > 40;
        let mut pen = Pixmap::new(24, 24).unwrap();
        let mut marker = Pixmap::new(24, 24).unwrap();
        draw_icon(&mut pen, IconKind::Pen, cell, None);
        draw_icon(&mut marker, IconKind::Marker, cell, None);
        assert_ne!(pen.data(), marker.data());
        assert!(ink(&pen, 18, 5), "pencil tip is a filled triangle");
        assert!(ink(&pen, 5, 19), "pencil eraser is filled");
        assert!(
            !ink(&pen, 11, 12),
            "pencil barrel stays hollow between the sides"
        );
        assert!(ink(&marker, 18, 8), "highlighter has an open chisel");
        assert!(ink(&marker, 6, 19), "highlighter back cap is solid");
        assert!(
            !ink(&marker, 11, 14),
            "highlighter barrel stays a hollow tube"
        );
        if let Ok(dir) = std::env::var("PINSHOT_DUMP_ICONS") {
            let dir = std::path::PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            dump_icon_preview(&dir.join("line.png"), IconKind::Line);
            dump_icon_preview(&dir.join("pen.png"), IconKind::Pen);
            dump_icon_preview(&dir.join("marker.png"), IconKind::Marker);
            dump_icon_preview(&dir.join("undo.png"), IconKind::Undo);
            dump_icon_preview(&dir.join("redo.png"), IconKind::Redo);
            let mut strip = Pixmap::new(420, 88).unwrap();
            strip.fill(tiny_skia::Color::from_rgba8(245, 245, 245, 255));
            for (i, kind) in [
                IconKind::Line,
                IconKind::Pen,
                IconKind::Marker,
                IconKind::Undo,
                IconKind::Redo,
            ]
            .into_iter()
            .enumerate()
            {
                draw_icon(
                    &mut strip,
                    kind,
                    Rect::new(12.0 + i as f32 * 82.0, 12.0, 64.0, 64.0),
                    Some(Rgba::rgb(40, 40, 40)),
                );
            }
            if let Ok(png) = strip.encode_png() {
                let _ = std::fs::write(dir.join("toolbar-icons.png"), png);
            }
        }
    }

    #[test]
    fn ocr_and_translate_are_not_a_t() {
        let cell = Rect::new(2.0, 2.0, 20.0, 20.0);
        let mut text = Pixmap::new(24, 24).unwrap();
        let mut ocr = Pixmap::new(24, 24).unwrap();
        let mut tr = Pixmap::new(24, 24).unwrap();
        draw_icon(&mut text, IconKind::Text, cell, None);
        draw_icon(&mut ocr, IconKind::Ocr, cell, None);
        draw_icon(&mut tr, IconKind::Translate, cell, None);
        assert_ne!(text.data(), ocr.data(), "OCR must not match Text T");
        assert_ne!(text.data(), tr.data(), "Translate must not match Text T");
        assert_ne!(ocr.data(), tr.data(), "OCR and Translate must differ");
        // Sparkle lives in the top-right of the 20×20 icon (cell origin 2,2).
        let sparkle = ocr.pixel(18, 6).unwrap().demultiply();
        assert!(
            sparkle.alpha() > 0,
            "OCR icon needs the Snipping Tool sparkle"
        );
    }
}

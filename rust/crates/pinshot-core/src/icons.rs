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
}

impl IconKind {
    pub fn from_id(s: &str) -> Option<Self> {
        Some(match s {
            "rect" => Self::Rect,
            "ellipse" => Self::Ellipse,
            "shape" | "roundrect" => Self::RoundRect,
            "line" => Self::Line,
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
            if let Some(path) = line_path((4.0, 16.0), (16.0, 4.0)) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
        }
        IconKind::Arrow => {
            let a = (4.0, 16.0);
            let b = (16.0, 4.0);
            let (head, half_w) = (5.2, 2.2);
            let shaft_end = arrow_geom(a, b, head, half_w).map(|g| g.2).unwrap_or(b);
            if let Some(path) = line_path(a, shaft_end) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            arrow_head_ex(pixmap, a, b, head, half_w, color, t, false);
        }
        IconKind::ArrowThick => {
            let a = (4.0, 16.0);
            let b = (16.2, 3.8);
            let (head, half_w) = (7.2, 3.6);
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
            let mut pb = PathBuilder::new();
            pb.move_to(3.0, 14.0);
            pb.cubic_to(6.0, 4.0, 10.0, 18.0, 17.0, 5.0);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
        }
        IconKind::Marker => {
            let mut fat = stroke(6.0);
            fat.line_cap = tiny_skia::LineCap::Butt;
            let mp = paint(color.with_alpha(110));
            if let Some(path) = line_path((3.0, 12.0), (17.0, 12.0)) {
                pixmap.stroke_path(&path, &mp, &fat, t, None);
            }
            if let Some(path) = line_path((5.0, 6.0), (14.0, 6.0)) {
                pixmap.stroke_path(&path, &p, &st, t, None);
            }
            let _ = mp;
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
            let flip = matches!(kind, IconKind::Redo);
            let tf = if flip {
                t.pre_translate(20.0, 0.0).pre_scale(-1.0, 1.0)
            } else {
                t
            };
            // Reply / forward: parallel bars, semicircle on the right, arrow on the
            // bottom (↩ / ↪). Redo is a horizontal flip of Undo.
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
        ] {
            pm.fill(tiny_skia::Color::TRANSPARENT);
            draw_icon(&mut pm, kind, Rect::new(2.0, 2.0, 20.0, 20.0), None);
            let painted = pm.pixels().iter().any(|p| p.alpha() > 0);
            assert!(painted, "{kind:?} produced an empty icon");
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
        // Undo: left-pointing head on the bottom, curve on the right.
        assert!(ink(&undo, 5, 16), "undo arrow tip should sit bottom-left");
        assert!(ink(&undo, 18, 12), "undo should curve on the right");
        assert!(
            !ink(&undo, 18, 5),
            "undo should not put the arrow in the top-right"
        );
        // Redo: right-pointing head on the bottom, curve on the left.
        assert!(ink(&redo, 18, 16), "redo arrow tip should sit bottom-right");
        assert!(ink(&redo, 5, 12), "redo should curve on the left");
        assert!(
            !ink(&redo, 5, 5),
            "redo should not put the arrow in the top-left"
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

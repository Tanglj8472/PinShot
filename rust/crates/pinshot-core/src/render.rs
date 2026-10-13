//! Software compose of annotations onto a [`Raster`] via tiny-skia.

use crate::annotation::{ArrowKind, DrawMode, Effect, Shape, Tool};
use crate::color::Rgba;
use crate::geom::Point;
use crate::raster::Raster;
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, StrokeDash, Transform,
};

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c.to_skia());
    p.anti_alias = true;
    p
}

fn stroke_for(sh: &Shape) -> Stroke {
    let dash = match sh.style.line_style {
        crate::annotation::LineStyleKind::Solid => None,
        crate::annotation::LineStyleKind::Dash => StrokeDash::new(
            vec![sh.style.width as f32 * 3.0, sh.style.width as f32 * 2.0],
            0.0,
        ),
        crate::annotation::LineStyleKind::Dot => StrokeDash::new(
            vec![sh.style.width as f32, sh.style.width as f32 * 1.5],
            0.0,
        ),
    };
    Stroke {
        width: sh.style.width.max(1) as f32,
        miter_limit: 4.0,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        dash,
    }
}

fn polyline(pts: &[Point]) -> Option<Path> {
    if pts.is_empty() {
        return None;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].x, pts[0].y);
    for p in &pts[1..] {
        pb.line_to(p.x, p.y);
    }
    if pts.len() == 1 {
        pb.line_to(pts[0].x + 0.1, pts[0].y);
    }
    pb.finish()
}

fn apply_effect_region(base: &Raster, sh: &Shape) -> Raster {
    match sh.style.effect {
        Effect::Blur => base.blur(sh.style.size.max(1) as u32),
        Effect::Mosaic => base.mosaic((sh.style.size.max(1) as u32) * 2),
    }
}

fn blit_clipped(dst: &mut Raster, src: &Raster, pred: impl Fn(i32, i32) -> bool) {
    let w = dst.width.min(src.width);
    let h = dst.height.min(src.height);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if pred(x, y) {
                if let Some(c) = src.get(x, y) {
                    dst.set(x, y, c);
                }
            }
        }
    }
}

fn in_brush(pts: &[Point], width: f32, x: i32, y: i32) -> bool {
    crate::geom::dist_to_polyline(Point::from_i32(x, y), pts) <= width / 2.0
}

/// Flatten shapes onto a copy of `base` (image coordinates).
pub fn compose(base: &Raster, shapes: &[Shape]) -> Raster {
    let mut out = base.clone();
    for sh in shapes {
        if sh.pts.is_empty() {
            continue;
        }
        match sh.kind {
            Tool::Mosaic => {
                let fx = apply_effect_region(base, sh);
                if sh.style.mode == DrawMode::Brush {
                    let w = (sh.style.size * 6) as f32;
                    blit_clipped(&mut out, &fx, |x, y| in_brush(&sh.pts, w, x, y));
                } else {
                    let r = sh.bounds();
                    blit_clipped(&mut out, &fx, |x, y| r.contains(Point::from_i32(x, y)));
                }
            }
            Tool::Eraser => {
                if sh.style.mode == DrawMode::Brush {
                    let w = sh.style.width as f32;
                    blit_clipped(&mut out, base, |x, y| in_brush(&sh.pts, w, x, y));
                } else {
                    let r = sh.bounds();
                    blit_clipped(&mut out, base, |x, y| r.contains(Point::from_i32(x, y)));
                }
            }
            _ => {
                if let Some(pm) = stroke_shape(sh, out.width, out.height) {
                    blit_pixmap(&mut out, &pm);
                }
            }
        }
    }
    out
}

/// Stroke vector annotations onto an existing pixmap (no full-buffer clone).
/// Mosaic / eraser stay on the `compose()` path because they sample the photo.
pub fn paint_shapes(pm: &mut Pixmap, shapes: &[Shape]) {
    for sh in shapes {
        if sh.pts.is_empty() {
            continue;
        }
        match sh.kind {
            Tool::Mosaic | Tool::Eraser => {}
            _ => draw_shape(pm, sh),
        }
    }
}

fn blit_pixmap(dst: &mut Raster, pm: &Pixmap) {
    for y in 0..pm.height() {
        for x in 0..pm.width() {
            if let Some(px) = pm.pixel(x, y) {
                if px.alpha() == 0 {
                    continue;
                }
                let c = px.demultiply();
                let src = Rgba::rgba(c.red(), c.green(), c.blue(), c.alpha());
                if src.a == 255 {
                    dst.set(x as i32, y as i32, src);
                } else if src.a > 0 {
                    if let Some(bg) = dst.get(x as i32, y as i32) {
                        let a = src.a as u16;
                        let ia = 255 - a;
                        dst.set(
                            x as i32,
                            y as i32,
                            Rgba::rgba(
                                ((src.r as u16 * a + bg.r as u16 * ia) / 255) as u8,
                                ((src.g as u16 * a + bg.g as u16 * ia) / 255) as u8,
                                ((src.b as u16 * a + bg.b as u16 * ia) / 255) as u8,
                                255,
                            ),
                        );
                    }
                }
            }
        }
    }
}

fn stroke_shape(sh: &Shape, w: u32, h: u32) -> Option<Pixmap> {
    let mut pm = Pixmap::new(w, h)?;
    draw_shape(&mut pm, sh);
    Some(pm)
}

/// Python `hit_test` 3×3 paint: translate so `pt` lands at pixel (1,1).
pub fn shape_paints_at(sh: &Shape, pt: Point) -> bool {
    if matches!(sh.kind, Tool::Mosaic | Tool::Eraser) {
        return false;
    }
    let Some(mut pm) = Pixmap::new(3, 3) else {
        return false;
    };
    draw_shape_t(
        &mut pm,
        sh,
        Transform::from_translate(1.0 - pt.x, 1.0 - pt.y),
    );
    for y in 0..3 {
        for x in 0..3 {
            if pm.pixel(x, y).map(|p| p.alpha() > 16).unwrap_or(false) {
                return true;
            }
        }
    }
    false
}

fn draw_shape(pm: &mut Pixmap, sh: &Shape) {
    draw_shape_t(pm, sh, Transform::identity());
}

fn draw_shape_t(pm: &mut Pixmap, sh: &Shape, t: Transform) {
    let b = sh.bounds();
    match sh.kind {
        Tool::Rect => {
            if let Some(r) = tiny_skia::Rect::from_xywh(b.x, b.y, b.w.max(1.0), b.h.max(1.0)) {
                if sh.style.filled {
                    pm.fill_rect(r, &paint(sh.style.color), t, None);
                } else {
                    pm.stroke_path(
                        &PathBuilder::from_rect(r),
                        &paint(sh.style.color),
                        &stroke_for(sh),
                        t,
                        None,
                    );
                }
            }
        }
        Tool::Ellipse => {
            if let Some(r) = tiny_skia::Rect::from_xywh(b.x, b.y, b.w.max(1.0), b.h.max(1.0)) {
                if let Some(path) = PathBuilder::from_oval(r) {
                    if sh.style.filled {
                        pm.fill_path(&path, &paint(sh.style.color), FillRule::Winding, t, None);
                    } else {
                        pm.stroke_path(&path, &paint(sh.style.color), &stroke_for(sh), t, None);
                    }
                }
            }
        }
        Tool::Line => {
            let kind = sh.style.arrow_kind();
            let pts = shaft_pts(&sh.pts, sh.style.width, kind);
            if let Some(path) = polyline(&pts) {
                pm.stroke_path(&path, &paint(sh.style.color), &stroke_for(sh), t, None);
            }
            if kind.is_arrow() && sh.pts.len() > 1 {
                draw_arrow_head(pm, sh, kind, t);
            }
        }
        Tool::Pen | Tool::Marker => {
            let mut c = sh.style.color;
            let mut st = stroke_for(sh);
            if sh.kind == Tool::Marker {
                c = c.with_alpha(110);
                st.line_cap = LineCap::Butt;
            }
            if sh.pts.len() == 1 {
                let r = sh.style.width as f32 / 2.0;
                if let Some(oval) =
                    tiny_skia::Rect::from_xywh(sh.pts[0].x - r, sh.pts[0].y - r, r * 2.0, r * 2.0)
                {
                    if let Some(path) = PathBuilder::from_oval(oval) {
                        pm.fill_path(&path, &paint(c), FillRule::Winding, t, None);
                    }
                }
            } else if let Some(path) = polyline(&sh.pts) {
                pm.stroke_path(&path, &paint(c), &st, t, None);
            }
        }
        Tool::Number => {
            let r = (sh.style.width.max(8) as f32) / 2.0;
            let c = sh.pts[0];
            if let Some(oval) = tiny_skia::Rect::from_xywh(c.x - r, c.y - r, r * 2.0, r * 2.0) {
                let ring = if sh.style.filled {
                    sh.style.color.darker()
                } else {
                    sh.style.color
                };
                if sh.style.filled {
                    if let Some(path) = PathBuilder::from_oval(oval) {
                        pm.fill_path(&path, &paint(sh.style.color), FillRule::Winding, t, None);
                    }
                }
                if let Some(path) = PathBuilder::from_oval(oval) {
                    pm.stroke_path(
                        &path,
                        &paint(ring),
                        &Stroke {
                            width: 2.0,
                            ..Stroke::default()
                        },
                        t,
                        None,
                    );
                }
                let label = if sh.number > 0 {
                    sh.number.to_string()
                } else {
                    String::new()
                };
                if !label.is_empty() {
                    let fs = sh.style.font_size.max(8) as f32;
                    let (tw, th) = crate::font::measure(&label, fs);
                    crate::font::blit_pixmap(
                        pm,
                        &label,
                        c.x - tw / 2.0 + t.tx,
                        c.y - th / 2.0 + t.ty,
                        fs,
                        if sh.style.filled {
                            Rgba::WHITE
                        } else {
                            sh.style.color
                        },
                        false,
                    );
                }
            }
        }
        Tool::Text => {
            if sh.text.is_empty() {
                if let Some(r) = tiny_skia::Rect::from_xywh(b.x, b.y + b.h - 2.0, b.w.max(4.0), 2.0)
                {
                    pm.fill_rect(r, &paint(sh.style.color), t, None);
                }
            } else {
                crate::font::blit_pixmap_ex(
                    pm,
                    &sh.text,
                    sh.pts[0].x + t.tx,
                    sh.pts[0].y + t.ty,
                    sh.style.font_size.max(8) as f32,
                    sh.style.color,
                    sh.style.outline,
                    &sh.style.family,
                    sh.style.bold,
                    sh.style.italic,
                );
            }
        }
        _ => {}
    }
}

/// `(length, half-angle rad, shaft pull as a fraction of length)`.
/// Snipaste: filled isosceles triangle, or an open V of two strokes.
fn arrow_metrics(width: i32, kind: ArrowKind) -> Option<(f32, f32, f32)> {
    let w = width.max(1) as f32;
    match kind {
        ArrowKind::None => None,
        ArrowKind::Thin => Some(((w * 3.1 + 7.0).max(11.0), 0.40, 0.90)),
        ArrowKind::Thick => Some(((w * 4.4 + 10.0).max(16.0), 0.44, 0.90)),
        // Shaft runs to the tip; the head is two wings, not a closed triangle.
        ArrowKind::Hollow => Some(((w * 3.8 + 9.0).max(14.0), 0.50, 0.12)),
    }
}

fn shaft_pts(pts: &[Point], width: i32, kind: ArrowKind) -> Vec<Point> {
    let mut out = pts.to_vec();
    let Some((l, _, pull)) = arrow_metrics(width, kind) else {
        return out;
    };
    if out.len() < 2 {
        return out;
    }
    let n = out.len();
    let a = out[n - 2];
    let b = out[n - 1];
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return out;
    }
    let back = (l * pull).min(len * 0.85);
    out[n - 1] = Point::new(b.x - dx / len * back, b.y - dy / len * back);
    out
}

fn wing(b: Point, ang: f32, spread: f32, l: f32) -> (Point, Point) {
    (
        Point::new(
            b.x - l * (ang - spread).cos(),
            b.y - l * (ang - spread).sin(),
        ),
        Point::new(
            b.x - l * (ang + spread).cos(),
            b.y - l * (ang + spread).sin(),
        ),
    )
}

fn draw_arrow_head(pm: &mut Pixmap, sh: &Shape, kind: ArrowKind, t: Transform) {
    let a = sh.pts[sh.pts.len() - 2];
    let bpt = sh.pts[sh.pts.len() - 1];
    let dx = bpt.x - a.x;
    let dy = bpt.y - a.y;
    if dx.abs() + dy.abs() < 0.5 {
        return;
    }
    let ang = dy.atan2(dx);
    let Some((l, spread, _)) = arrow_metrics(sh.style.width, kind) else {
        return;
    };
    let (left, right) = wing(bpt, ang, spread, l);
    match kind {
        ArrowKind::None => {}
        ArrowKind::Hollow => {
            // Open V: two strokes, same width as the shaft (Snipaste).
            let mut st = stroke_for(sh);
            st.line_cap = LineCap::Round;
            st.line_join = LineJoin::Round;
            let mut pb = PathBuilder::new();
            pb.move_to(left.x, left.y);
            pb.line_to(bpt.x, bpt.y);
            pb.line_to(right.x, right.y);
            if let Some(path) = pb.finish() {
                pm.stroke_path(&path, &paint(sh.style.color), &st, t, None);
            }
        }
        ArrowKind::Thin | ArrowKind::Thick => {
            let mut pb = PathBuilder::new();
            pb.move_to(bpt.x, bpt.y);
            pb.line_to(left.x, left.y);
            pb.line_to(right.x, right.y);
            pb.close();
            if let Some(path) = pb.finish() {
                pm.fill_path(&path, &paint(sh.style.color), FillRule::Winding, t, None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::Style;
    use crate::geom::Point;

    #[test]
    fn filled_rect_paints_interior() {
        let mut base = Raster::new(80, 60);
        base.fill(Rgba::WHITE);
        let sh = Shape::new(
            Tool::Rect,
            vec![Point::new(10.0, 10.0), Point::new(40.0, 40.0)],
            Style {
                filled: true,
                color: Rgba::RED,
                ..Style::default()
            },
        );
        let out = compose(&base, &[sh]);
        assert_eq!(out.get(20, 20), Some(Rgba::RED));
        assert_eq!(out.get(70, 50), Some(Rgba::WHITE));
    }

    #[test]
    fn paint_shapes_does_not_clone_photo() {
        let mut pm = Pixmap::new(40, 40).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
        let sh = Shape::new(
            Tool::Rect,
            vec![Point::new(5.0, 5.0), Point::new(20.0, 20.0)],
            Style {
                filled: true,
                color: Rgba::RED,
                ..Style::default()
            },
        );
        paint_shapes(&mut pm, &[sh]);
        let c = pm.pixel(10, 10).unwrap().demultiply();
        assert_eq!((c.red(), c.green(), c.blue()), (255, 0, 0));
        let bg = pm.pixel(30, 30).unwrap().demultiply();
        assert_eq!((bg.red(), bg.green(), bg.blue()), (255, 255, 255));
    }

    #[test]
    fn mosaic_changes_pixels() {
        let mut base = Raster::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                base.set(x, y, Rgba::rgb((x * 8) as u8, (y * 8) as u8, 40));
            }
        }
        let sh = Shape::new(
            Tool::Mosaic,
            vec![Point::new(0.0, 0.0), Point::new(31.0, 31.0)],
            Style {
                mode: DrawMode::Rect,
                size: 6,
                ..Style::default()
            },
        );
        let out = compose(&base, &[sh]);
        assert_ne!(out.get(1, 1), base.get(1, 1));
    }

    fn count_red(out: &Raster, xs: std::ops::Range<i32>, ys: std::ops::Range<i32>) -> u32 {
        let mut found = 0u32;
        for y in ys {
            for x in xs.clone() {
                if out.get(x, y) == Some(Rgba::RED) {
                    found += 1;
                }
            }
        }
        found
    }

    fn line_shape(kind: crate::annotation::ArrowKind) -> Shape {
        let mut st = Style::for_tool(Tool::Line);
        st.set_arrow_kind(kind);
        st.color = Rgba::RED;
        st.width = 3;
        Shape::new(
            Tool::Line,
            vec![Point::new(8.0, 20.0), Point::new(50.0, 20.0)],
            st,
        )
    }

    #[test]
    fn arrow_head_paints_beyond_plain_line() {
        let mut base = Raster::new(80, 40);
        base.fill(Rgba::WHITE);
        let out = compose(&base, &[line_shape(crate::annotation::ArrowKind::Thin)]);
        let found = count_red(&out, 38..62, 10..30);
        assert!(found > 20, "arrowhead pixels={found}");
    }

    #[test]
    fn thick_arrow_covers_more_than_thin() {
        let mut base = Raster::new(80, 40);
        base.fill(Rgba::WHITE);
        let thin = count_red(
            &compose(&base, &[line_shape(crate::annotation::ArrowKind::Thin)]),
            30..62,
            4..36,
        );
        let thick = count_red(
            &compose(&base, &[line_shape(crate::annotation::ArrowKind::Thick)]),
            30..62,
            4..36,
        );
        assert!(thick > thin, "thick={thick} thin={thin}");
    }

    #[test]
    fn hollow_arrow_is_outline_not_a_solid_wedge() {
        let mut base = Raster::new(80, 40);
        base.fill(Rgba::WHITE);
        let out = compose(&base, &[line_shape(crate::annotation::ArrowKind::Hollow)]);
        let edge = count_red(&out, 38..62, 10..30);
        assert!(edge > 12, "hollow outline pixels={edge}");
        // Off the shaft, inside the V — must stay empty (not a filled triangle).
        let interior = count_red(&out, 40..43, 25..28);
        assert!(
            interior < 8,
            "hollow should not fill the wedge, interior={interior}"
        );
    }

    #[test]
    fn thin_arrow_fills_the_triangle() {
        let mut base = Raster::new(80, 40);
        base.fill(Rgba::WHITE);
        let out = compose(&base, &[line_shape(crate::annotation::ArrowKind::Thin)]);
        // Centroid of the filled head sits a few px behind the tip at (50,20).
        let core = count_red(&out, 44..48, 18..23);
        assert!(core > 8, "filled triangle core pixels={core}");
    }

    #[test]
    fn line_tool_default_has_no_arrow() {
        assert!(!Style::for_tool(Tool::Line).has_arrow());
        assert_eq!(
            Style::for_tool(Tool::Line).arrow_kind(),
            crate::annotation::ArrowKind::None
        );
    }

    #[test]
    fn pixel_hit_follows_stroke_not_bbox_slop() {
        let sh = Shape::new(
            Tool::Line,
            vec![Point::new(10.0, 20.0), Point::new(50.0, 20.0)],
            Style {
                width: 2,
                arrow: false,
                color: Rgba::RED,
                ..Style::default()
            },
        );
        assert!(shape_paints_at(&sh, Point::new(30.0, 20.0)));
        assert!(!shape_paints_at(&sh, Point::new(30.0, 32.0)));
        let hollow = Shape::new(
            Tool::Rect,
            vec![Point::new(8.0, 8.0), Point::new(40.0, 30.0)],
            Style {
                width: 2,
                filled: false,
                color: Rgba::RED,
                ..Style::default()
            },
        );
        assert!(shape_paints_at(&hollow, Point::new(8.0, 18.0)));
        assert!(!shape_paints_at(&hollow, Point::new(24.0, 19.0)));
    }
}

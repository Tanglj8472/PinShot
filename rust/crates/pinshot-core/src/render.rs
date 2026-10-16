//! Software compose of annotations onto a [`Raster`] via tiny-skia.

use crate::annotation::{ArrowKind, DrawMode, Effect, Shape, Tool};
use crate::color::Rgba;
use crate::geom::{Point, Rect};
use std::cell::RefCell;
use std::sync::Arc;
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

/// Build a smooth freehand path from sampled pointer positions. Using the
/// midpoint between consecutive samples as the quadratic endpoint avoids the
/// visibly segmented, ruler-like appearance caused by sparse mouse events.
fn brush_path(pts: &[Point]) -> Option<Path> {
    if pts.is_empty() {
        return None;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].x, pts[0].y);
    if pts.len() == 1 {
        pb.line_to(pts[0].x + 0.1, pts[0].y);
        return pb.finish();
    }
    for pair in pts.windows(2) {
        let a = pair[0];
        let b = pair[1];
        let mid = Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        pb.quad_to(a.x, a.y, mid.x, mid.y);
    }
    let last = pts[pts.len() - 1];
    let prev = pts[pts.len() - 2];
    pb.quad_to(prev.x, prev.y, last.x, last.y);
    pb.finish()
}

fn smoothed_brush_points(pts: &[Point]) -> Vec<Point> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(pts.len() * 2);
    out.push(pts[0]);
    for pair in pts.windows(2) {
        let a = pair[0];
        let b = pair[1];
        out.push(Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5));
        out.push(b);
    }
    out
}

fn apply_effect_region(base: &Raster, sh: &Shape) -> Raster {
    match sh.style.effect {
        Effect::Blur => base.blur(sh.style.size.max(1) as u32),
        Effect::Mosaic => base.mosaic((sh.style.size.max(1) as u32) * 2),
    }
}

#[derive(Default)]
struct EffectCache {
    base: Option<Raster>,
    effects: Vec<(Effect, i32, Arc<Raster>)>,
}

thread_local! {
    static EFFECT_CACHE: RefCell<EffectCache> = RefCell::new(EffectCache::default());
}

fn cached_effect(base: &Raster, sh: &Shape) -> Arc<Raster> {
    EFFECT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.base.as_ref() != Some(base) {
            cache.base = Some(base.clone());
            cache.effects.clear();
        }
        let size = sh.style.size.max(1);
        if let Some((_, _, image)) = cache.effects.iter().find(|(effect, n, _)| {
            *effect == sh.style.effect && *n == size
        }) {
            return Arc::clone(image);
        }
        let image = Arc::new(apply_effect_region(base, sh));
        // Bound memory while retaining both effects and recently used sizes.
        if cache.effects.len() >= 4 {
            cache.effects.remove(0);
        }
        cache.effects.push((sh.style.effect, size, Arc::clone(&image)));
        image
    })
}

fn copy_effect_rect(dst: &mut Raster, src: &Raster, r: Rect) {
    let x0 = r.x.floor().max(0.0) as usize;
    let y0 = r.y.floor().max(0.0) as usize;
    let x1 = r.right().ceil().max(0.0).min(dst.width.min(src.width) as f32) as usize;
    let y1 = r.bottom().ceil().max(0.0).min(dst.height.min(src.height) as f32) as usize;
    if x0 >= x1 || y0 >= y1 { return; }
    for y in y0..y1 {
        let d = (y * dst.width as usize + x0) * 4;
        let s = (y * src.width as usize + x0) * 4;
        dst.pixels[d..d + (x1 - x0) * 4].copy_from_slice(&src.pixels[s..s + (x1 - x0) * 4]);
    }
}

fn copy_effect_brush(dst: &mut Raster, src: &Raster, pts: &[Point], width: f32) {
    let clip = Rect::new(0.0, 0.0, dst.width as f32, dst.height as f32);
    copy_effect_brush_clipped(dst, src, pts, width, clip);
}

fn copy_effect_brush_clipped(dst: &mut Raster, src: &Raster, pts: &[Point], width: f32, clip: Rect) {
    if pts.is_empty() { return; }
    let pts = smoothed_brush_points(pts);
    let radius = width.max(1.0) / 2.0;
    // Each segment touches only its local bounding box, never the full image.
    let segments = pts.len().saturating_sub(1).max(1);
    for i in 0..segments {
        let a = pts[i];
        let b = pts.get(i + 1).copied().unwrap_or(a);
        let r = Rect::from_points(a, b).normalized().inflated(radius);
        let x0 = r.x.floor().max(0.0).max(clip.x) as i32;
        let y0 = r.y.floor().max(0.0).max(clip.y) as i32;
        let x1 = (r.right().floor() + 1.0).max(0.0).min(dst.width.min(src.width) as f32).min(clip.right()) as i32;
        let y1 = (r.bottom().floor() + 1.0).max(0.0).min(dst.height.min(src.height) as f32).min(clip.bottom()) as i32;
        for y in y0..y1 {
            for x in x0..x1 {
                if in_brush(&[a, b], width, x, y) {
                    let d = (y as usize * dst.width as usize + x as usize) * 4;
                    let s = (y as usize * src.width as usize + x as usize) * 4;
                    dst.pixels[d..d + 4].copy_from_slice(&src.pixels[s..s + 4]);
                }
            }
        }
    }
}

fn in_brush(pts: &[Point], width: f32, x: i32, y: i32) -> bool {
    crate::geom::dist_to_polyline(Point::from_i32(x, y), pts) <= width / 2.0
}

#[cfg(test)]
#[test]
fn bounded_brush_matches_full_image_mask() {
    let mut source = Raster::new(40, 32);
    for y in 0..32 { for x in 0..40 { source.set(x, y, Rgba::rgb(x as u8, y as u8, 100)); } }
    let pts = [Point::new(-3.0, 5.0), Point::new(30.0, 20.0), Point::new(8.0, 28.0)];
    let mut expected = Raster::new(40, 32);
    for y in 0..32 { for x in 0..40 {
        if in_brush(&pts, 12.0, x, y) { expected.set(x, y, source.get(x, y).unwrap()); }
    } }
    let mut actual = Raster::new(40, 32);
    copy_effect_brush(&mut actual, &source, &pts, 12.0);
    assert_eq!(actual.pixels, expected.pixels);
}

#[cfg(test)]
#[test]
fn marker_closed_freehand_path_keeps_stroke_and_click_paints_dot() {
    use crate::annotation::Style;
    let style = Style::for_tool(Tool::Marker);
    let start = Point::new(12.0, 12.0);
    let loop_shape = Shape::new(Tool::Marker,
        vec![start, Point::new(45.0, 12.0), Point::new(45.0, 45.0), start], style.clone());
    assert!(shape_paints_at(&loop_shape, Point::new(45.0, 30.0)));
    let dot = Shape::new(Tool::Marker, vec![start], style);
    assert!(shape_paints_at(&dot, start));
}

#[cfg(test)]
#[test]
fn effect_strength_changes_pixels_without_changing_brush_coverage() {
    use crate::annotation::Style;
    let mut base = Raster::new(64, 48);
    for y in 0..48 { for x in 0..64 {
        base.set(x, y, Rgba::rgb((x * 31) as u8, (y * 29) as u8, (x * y) as u8));
    } }
    for effect in [Effect::Mosaic, Effect::Blur] {
        let mut style = Style::for_tool(Tool::Mosaic);
        style.effect = effect;
        style.width = 12;
        let pts = vec![Point::new(10.0, 10.0), Point::new(35.0, 10.0), Point::new(35.0, 30.0)];
        let mut outputs = Vec::new();
        for size in [1, 15] {
            style.size = size;
            let sh = Shape::new(Tool::Mosaic, pts.clone(), style.clone());
            let fx = apply_effect_region(&base, &sh);
            let actual = compose(&base, &[sh]);
            for y in 0..48 { for x in 0..64 {
                let source = if in_brush(&pts, 12.0, x, y) { &fx } else { &base };
                assert_eq!(actual.get(x, y), source.get(x, y));
            } }
            outputs.push(actual);
        }
        assert_ne!(outputs[0].pixels, outputs[1].pixels);
    }
}

/// Flatten shapes onto a copy of `base` (image coordinates).
pub fn compose(base: &Raster, shapes: &[Shape]) -> Raster {
    COMPOSE_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        let same_base = cache.base.as_ref() == Some(base);
        if same_base {
            if cache.last_shapes == shapes {
                if let Some(frame) = &cache.last_frame { return frame.clone(); }
            }
            if let Some(tail) = appended_effect_stroke(&cache.last_shapes, shapes) {
                if let Some(frame) = cache.last_frame.take() {
                    let out = compose_onto(base, &[tail], frame);
                    cache.last_shapes = shapes.to_vec();
                    cache.last_frame = Some(out.clone());
                    return out;
                }
            }
            if let Some(dirty) = changed_effect_rect(&cache.last_shapes, shapes) {
                if let Some(mut out) = cache.last_frame.take() {
                    recompose_region(base, shapes, &mut out, dirty);
                    cache.last_shapes = shapes.to_vec();
                    cache.last_frame = Some(out.clone());
                    return out;
                }
            }
        }
        let prefix_len = shapes.len().saturating_sub(1);
        let prefix = &shapes[..prefix_len];
        // A completed live stroke is already flattened. Promote that frame
        // when the next stroke starts instead of replaying the old stroke.
        if same_base && prefix == cache.last_shapes && cache.last_frame.is_some() {
            cache.frame = cache.last_frame.clone();
            cache.shapes = prefix.to_vec();
        }
        if !same_base || !prefix.starts_with(&cache.shapes) || cache.frame.is_none() {
            cache.base = Some(base.clone());
            cache.shapes.clear();
            cache.frame = Some(base.clone());
        }
        let old_len = cache.shapes.len();
        if old_len < prefix_len {
            let frame = cache.frame.take().unwrap();
            cache.frame = Some(compose_onto(base, &prefix[old_len..], frame));
            cache.shapes = prefix.to_vec();
        }
        let out = compose_onto(base, &shapes[prefix_len..], cache.frame.as_ref().unwrap().clone());
        cache.last_shapes = shapes.to_vec();
        cache.last_frame = Some(out.clone());
        out
    })
}

#[derive(Default)]
struct ComposeCache {
    base: Option<Raster>,
    shapes: Vec<Shape>,
    frame: Option<Raster>,
    last_shapes: Vec<Shape>,
    last_frame: Option<Raster>,
}

fn appended_effect_stroke(before: &[Shape], after: &[Shape]) -> Option<Shape> {
    if before.len() != after.len() || before.is_empty() { return None; }
    let last = before.len() - 1;
    if before[..last] != after[..last] { return None; }
    let a = &before[last];
    let b = &after[last];
    if !matches!(b.kind, Tool::Mosaic | Tool::Eraser)
        || b.style.mode != DrawMode::Brush || a.kind != b.kind
        || a.style != b.style || a.text != b.text || a.number != b.number
        || a.pts.is_empty() || b.pts.len() <= a.pts.len()
        || !b.pts.starts_with(&a.pts)
    { return None; }
    let mut tail = b.clone();
    tail.pts = b.pts[a.pts.len() - 1..].to_vec();
    Some(tail)
}

// Only a single rectangular effect changed; everything outside its old/new
// coverage is identical. Other edits keep the general prefix-cache path.
fn changed_effect_rect(before: &[Shape], after: &[Shape]) -> Option<Rect> {
    if before.len() != after.len() { return None; }
    let mut changes = before.iter().zip(after).filter(|(a, b)| a != b);
    let (a, b) = changes.next()?;
    if changes.next().is_some()
        || a.kind != Tool::Mosaic || b.kind != Tool::Mosaic
        || a.style.mode != DrawMode::Rect || b.style.mode != DrawMode::Rect
        || a.pts.is_empty() || b.pts.is_empty()
    { return None; }
    let coverage = |sh: &Shape| {
        let r = sh.bounds();
        Rect::new(r.x.floor(), r.y.floor(), r.right().ceil() - r.x.floor(), r.bottom().ceil() - r.y.floor())
    };
    Some(coverage(a).union(coverage(b)))
}

fn recompose_region(base: &Raster, shapes: &[Shape], out: &mut Raster, dirty: Rect) {
    let x = dirty.x.floor().max(0.0).min(base.width as f32);
    let y = dirty.y.floor().max(0.0).min(base.height as f32);
    let right = dirty.right().ceil().max(0.0).min(base.width as f32);
    let bottom = dirty.bottom().ceil().max(0.0).min(base.height as f32);
    if right <= x || bottom <= y { return; }
    let clip = Rect::new(x, y, right - x, bottom - y);
    copy_effect_rect(out, base, clip);
    let mut pm = Pixmap::new(clip.w as u32, clip.h as u32).unwrap();
    for sh in shapes {
        if sh.pts.is_empty() { continue; }
        match sh.kind {
            Tool::Mosaic | Tool::Eraser => {
                let width = sh.style.width;
                let reach = if sh.style.mode == DrawMode::Brush { width.max(1) as f32 / 2.0 + 1.0 } else { 1.0 };
                if !sh.bounds().inflated(reach).intersects(clip) { continue; }
                let fx = if sh.kind == Tool::Mosaic { Some(cached_effect(base, sh)) } else { None };
                let source = fx.as_deref().unwrap_or(base);
                if sh.style.mode == DrawMode::Brush {
                    copy_effect_brush_clipped(out, source, &sh.pts, width as f32, clip);
                } else {
                    let r = sh.bounds().intersect(clip);
                    if !r.is_empty() { copy_effect_rect(out, source, r); }
                }
            }
            _ => {
                pm.fill(tiny_skia::Color::TRANSPARENT);
                draw_shape_t(&mut pm, sh, Transform::from_translate(-x, -y));
                blit_pixmap_at(out, &pm, x as i32, y as i32);
            }
        }
    }
}

thread_local! {
    static COMPOSE_CACHE: RefCell<ComposeCache> = RefCell::new(ComposeCache::default());
}

fn compose_onto(base: &Raster, shapes: &[Shape], mut out: Raster) -> Raster {
    for sh in shapes {
        if sh.pts.is_empty() {
            continue;
        }
        match sh.kind {
            Tool::Mosaic => {
                let fx = cached_effect(base, sh);
                if sh.style.mode == DrawMode::Brush {
                    let w = sh.style.width.max(1) as f32;
                    copy_effect_brush(&mut out, &fx, &sh.pts, w);
                } else {
                    let r = sh.bounds();
                    copy_effect_rect(&mut out, &fx, r);
                }
            }
            Tool::Eraser => {
                if sh.style.mode == DrawMode::Brush {
                    let w = sh.style.width as f32;
                    copy_effect_brush(&mut out, base, &sh.pts, w);
                } else {
                    let r = sh.bounds();
                    copy_effect_rect(&mut out, base, r);
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

#[cfg(test)]
#[test]
fn incremental_compose_matches_replay_after_edits_and_undo() {
    use crate::annotation::Style;
    let mut base = Raster::new(64, 48);
    for y in 0..48 { for x in 0..64 { base.set(x,y,Rgba::rgb((x*3) as u8,(y*4) as u8,80)); } }
    let mut shapes = Vec::new();
    let verify = |base: &Raster, shapes: &[Shape]| {
        assert_eq!(compose(base, shapes), compose_onto(base, shapes, base.clone()));
    };
    for i in 0..12 {
        let kind = if i % 3 == 0 { Tool::Mosaic } else { Tool::Marker };
        shapes.push(Shape::new(kind, vec![Point::new(5.0,i as f32+4.0),Point::new(50.0,35.0)],Style::for_tool(kind)));
        verify(&base,&shapes);
    }
    shapes.last_mut().unwrap().pts[1] = Point::new(40.0,25.0);
    verify(&base,&shapes);
    shapes[0].style.size = 3;
    verify(&base,&shapes);
    shapes.truncate(4);
    verify(&base,&shapes);
    base.set(10,10,Rgba::RED);
    verify(&base,&shapes);
    verify(&base,&[]);
}

#[cfg(test)]
#[test]
fn growing_overlapping_effect_strokes_match_full_replay() {
    use crate::annotation::Style;
    let mut base = Raster::new(72, 56);
    for y in 0..56 { for x in 0..72 {
        base.set(x, y, Rgba::rgb((x * 17) as u8, (y * 23) as u8, (x * y) as u8));
    } }
    let mut shapes = Vec::new();
    for i in 0..8 {
        let kind = if i == 7 { Tool::Eraser } else { Tool::Mosaic };
        let mut style = Style::for_tool(kind);
        style.effect = if i % 2 == 0 { Effect::Mosaic } else { Effect::Blur };
        style.size = 2 + i;
        let start = Point::new(10.0, 12.0);
        shapes.push(Shape::new(kind, vec![start], style));
        assert_eq!(compose(&base, &shapes), compose_onto(&base, &shapes, base.clone()));
        for pt in [Point::new(60.0, 12.0), Point::new(40.0, 44.0), start, Point::new(-5.0, 20.0)] {
            shapes.last_mut().unwrap().pts.push(pt);
            assert_eq!(compose(&base, &shapes), compose_onto(&base, &shapes, base.clone()));
        }
        // A style change must replay the entire stroke, not just its tail.
        shapes.last_mut().unwrap().style.width = 8;
        assert_eq!(compose(&base, &shapes), compose_onto(&base, &shapes, base.clone()));
    }
    shapes.truncate(3);
    assert_eq!(compose(&base, &shapes), compose_onto(&base, &shapes, base.clone()));
    base.set(12, 12, Rgba::RED);
    assert_eq!(compose(&base, &shapes), compose_onto(&base, &shapes, base.clone()));
}

#[cfg(test)]
#[test]
#[ignore = "manual release-mode rendering benchmark"]
fn benchmark_growing_overlapping_effect_stroke() {
    use crate::annotation::Style;
    use std::time::Instant;
    let base = Raster::new(1280, 720);
    let mut shapes = Vec::new();
    for i in 0..20 {
        let mut style = Style::for_tool(Tool::Mosaic);
        style.effect = if i % 2 == 0 { Effect::Mosaic } else { Effect::Blur };
        let pts = (0..200).map(|n| Point::new(400.0 + (n % 80) as f32, 250.0 + (n % 40) as f32)).collect();
        shapes.push(Shape::new(Tool::Mosaic, pts, style));
    }
    let prefix = compose(&base, &shapes);
    let mut stroke = shapes.last().unwrap().clone();
    stroke.pts = (0..500).map(|n| Point::new(400.0 + (n % 80) as f32, 250.0 + (n % 40) as f32)).collect();
    shapes.push(stroke.clone());
    compose(&base, &shapes);
    let start = Instant::now();
    for n in 0..30 {
        shapes.last_mut().unwrap().pts.push(Point::new(450.0 + n as f32, 270.0));
        std::hint::black_box(compose(&base, &shapes));
    }
    let incremental = start.elapsed();
    let start = Instant::now();
    for n in 0..30 {
        stroke.pts.push(Point::new(450.0 + n as f32, 270.0));
        std::hint::black_box(compose_onto(&base, std::slice::from_ref(&stroke), prefix.clone()));
    }
    eprintln!("30 frames, 1280x720, 21 overlapping layers, 500-point stroke: incremental={incremental:?}, replay_stroke={:?}", start.elapsed());
}

#[cfg(test)]
#[test]
fn moving_rect_effect_matches_full_replay_with_overlapping_layers() {
    use crate::annotation::Style;
    let mut base = Raster::new(96, 72);
    for y in 0..72 { for x in 0..96 {
        base.set(x, y, Rgba::rgb((x * 19) as u8, (y * 23) as u8, (x * y) as u8));
    } }
    let mut style = Style::for_tool(Tool::Mosaic);
    style.mode = DrawMode::Rect;
    style.effect = Effect::Blur;
    let mut shapes = vec![Shape::new(Tool::Mosaic,
        vec![Point::new(12.25, 8.75), Point::new(40.5, 31.25)], style)];
    for kind in [Tool::Marker, Tool::Rect, Tool::Line, Tool::Mosaic, Tool::Eraser] {
        shapes.push(Shape::new(kind, vec![Point::new(1.5, 2.25), Point::new(83.75, 62.5)], Style::for_tool(kind)));
    }
    compose(&base, &shapes);
    for (a, b) in [(-8.5, 11.25), (20.5, 28.75), (80.25, 60.5), (110.0, 90.0), (11.0, 9.0)] {
        shapes[0].pts = vec![Point::new(a, b), Point::new(a + 28.25, b + 22.5)];
        assert_eq!(compose(&base, &shapes), compose_onto(&base, &shapes, base.clone()));
    }
    shapes[0].style.effect = Effect::Mosaic;
    assert_eq!(compose(&base, &shapes), compose_onto(&base, &shapes, base.clone()));
    shapes[0].pts = vec![Point::new(8.5, 9.5), Point::new(8.5, 9.5)];
    assert_eq!(compose(&base, &shapes), compose_onto(&base, &shapes, base.clone()));
    shapes[0].pts = vec![Point::new(15.5, 19.5), Point::new(25.5, 29.5)];
    assert_eq!(compose(&base, &shapes), compose_onto(&base, &shapes, base.clone()));
    shapes.remove(0);
    assert_eq!(compose(&base, &shapes), compose_onto(&base, &shapes, base.clone()));
}

#[cfg(test)]
#[test]
#[ignore = "manual release-mode rendering benchmark"]
fn benchmark_moving_rect_effect() {
    use crate::annotation::Style;
    use std::time::Instant;
    let base = Raster::new(1280, 720);
    let mut style = Style::for_tool(Tool::Mosaic);
    style.mode = DrawMode::Rect;
    style.effect = Effect::Blur;
    let mut shapes = vec![Shape::new(Tool::Mosaic,
        vec![Point::new(100.0, 100.0), Point::new(250.0, 200.0)], style)];
    for i in 0..30 {
        shapes.push(Shape::new(Tool::Marker,
            vec![Point::new(10.0, i as f32 * 15.0), Point::new(1100.0, 600.0)],
            Style::for_tool(Tool::Marker)));
    }
    compose(&base, &shapes);
    let start = Instant::now();
    for i in 0..10 {
        shapes[0].pts[0].x = 101.0 + i as f32;
        std::hint::black_box(compose(&base, &shapes));
    }
    let regional = start.elapsed();
    let start = Instant::now();
    for i in 0..10 {
        shapes[0].pts[0].x = 101.0 + i as f32;
        std::hint::black_box(compose_onto(&base, &shapes, base.clone()));
    }
    eprintln!("10 frames, 1280x720, 31 layers: regional={regional:?}, full={:?}", start.elapsed());
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
    blit_pixmap_at(dst, pm, 0, 0);
}

fn blit_pixmap_at(dst: &mut Raster, pm: &Pixmap, dx: i32, dy: i32) {
    for y in 0..pm.height() {
        for x in 0..pm.width() {
            if let Some(px) = pm.pixel(x, y) {
                if px.alpha() == 0 {
                    continue;
                }
                let c = px.demultiply();
                let src = Rgba::rgba(c.red(), c.green(), c.blue(), c.alpha());
                if src.a == 255 {
                    dst.set(x as i32 + dx, y as i32 + dy, src);
                } else if src.a > 0 {
                    if let Some(bg) = dst.get(x as i32 + dx, y as i32 + dy) {
                        let a = src.a as u16;
                        let ia = 255 - a;
                        dst.set(
                            x as i32 + dx,
                            y as i32 + dy,
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
            if kind == ArrowKind::Thin && sh.pts.len() > 1 {
                // Sharp: tapered filled shaft + acute triangle, not a kite.
                if sh.pts.len() > 2 {
                    if let Some(path) = polyline(&sh.pts[..sh.pts.len() - 1]) {
                        pm.stroke_path(&path, &paint(sh.style.color), &stroke_for(sh), t, None);
                    }
                }
                let n = sh.pts.len();
                fill_sharp_arrow(
                    pm,
                    sh.pts[n - 2],
                    sh.pts[n - 1],
                    sh.style.width,
                    t,
                    sh.style.color,
                );
            } else {
                let pts = shaft_pts(&sh.pts, sh.style.width, kind);
                if let Some(path) = polyline(&pts) {
                    let mut st = stroke_for(sh);
                    if kind == ArrowKind::Thick {
                        st.line_cap = LineCap::Butt;
                    }
                    pm.stroke_path(&path, &paint(sh.style.color), &st, t, None);
                }
                if kind.is_arrow() && sh.pts.len() > 1 {
                    draw_arrow_head(pm, sh, kind, t);
                }
            }
        }
        Tool::Marker if sh.style.mode == DrawMode::Rect => {
            let r = sh.bounds();
            if let Some(rect) = tiny_skia::Rect::from_xywh(r.x, r.y, r.w, r.h) {
                let path = PathBuilder::from_rect(rect);
                pm.fill_path(&path, &paint(sh.style.color.with_alpha(110)), FillRule::Winding, t, None);
            }
        }
        Tool::Pen | Tool::Marker => {
            let mut c = sh.style.color;
            let mut st = stroke_for(sh);
            if sh.kind == Tool::Marker {
                c = c.with_alpha(110);
                st.line_cap = LineCap::Round;
            }
            if sh.pts.len() == 1
                || (sh.kind == Tool::Marker && sh.pts.iter().all(|p| *p == sh.pts[0]))
            {
                let r = sh.style.width as f32 / 2.0;
                if let Some(oval) =
                    tiny_skia::Rect::from_xywh(sh.pts[0].x - r, sh.pts[0].y - r, r * 2.0, r * 2.0)
                {
                    if let Some(path) = PathBuilder::from_oval(oval) {
                        pm.fill_path(&path, &paint(c), FillRule::Winding, t, None);
                    }
                }
            } else if let Some(path) = brush_path(&sh.pts) {
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

/// `(head length along the shaft, half-width of the perpendicular base)`.
///
/// Textbook construction (GDI / unit dir + perp): tip at B, base corners
/// `B - length·U ± half_width·V` with `U = normalize(B−A)`, `V = (−Uy, Ux)`.
/// Thick: filled triangle on a constant shaft, base perpendicular to the shaft.
/// Hollow: open V, shaft to the tip.
/// Thin: needle shaft (thin→slightly thicker) + a distinct acute triangle.
fn arrow_metrics(width: i32, kind: ArrowKind) -> Option<(f32, f32)> {
    let w = width.max(1) as f32;
    match kind {
        ArrowKind::None | ArrowKind::Thin => None,
        ArrowKind::Thick => {
            let len = (w * 2.8 + 7.0).max(11.0);
            Some((len, w * 1.35 + 2.4))
        }
        ArrowKind::Hollow => {
            let len = (w * 3.0 + 7.0).max(11.0);
            Some((len, w * 1.25 + 2.2))
        }
    }
}

/// Sharp arrow: a thin trapezoid shaft plus a separate acute head.
/// Head size follows stroke width, not total length — a long arrow must
/// stay a line with a tip, not a kite/wedge spanning the whole segment.
fn fill_sharp_arrow(pm: &mut Pixmap, a: Point, b: Point, width: i32, t: Transform, color: Rgba) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let vx = -uy;
    let vy = ux;
    let w = width.max(1) as f32;
    // Snipaste thin: hairline tail, slim neck, larger spear head with
    // longer swept barbs (尾翼) — not a stub triangle.
    let tail_hw = 0.42_f32.max(w * 0.14);
    let neck_hw = (w * 0.42).max(tail_hw + 0.18);
    let barb_hw = (w * 1.2 + 2.8).max(neck_hw * 2.5);
    let head_len = (w * 8.5 + 16.0).min(len * 0.56).max(12.0f32.min(len * 0.72));
    let sweep = (head_len * 0.22).min(6.5);
    let neck = Point::new(b.x - ux * head_len, b.y - uy * head_len);
    let barb = Point::new(neck.x - ux * sweep, neck.y - uy * sweep);
    let mut pb = PathBuilder::new();
    pb.move_to(a.x + vx * tail_hw, a.y + vy * tail_hw);
    pb.line_to(neck.x + vx * neck_hw, neck.y + vy * neck_hw);
    pb.line_to(barb.x + vx * barb_hw, barb.y + vy * barb_hw);
    pb.line_to(b.x, b.y);
    pb.line_to(barb.x - vx * barb_hw, barb.y - vy * barb_hw);
    pb.line_to(neck.x - vx * neck_hw, neck.y - vy * neck_hw);
    pb.line_to(a.x - vx * tail_hw, a.y - vy * tail_hw);
    pb.close();
    if let Some(path) = pb.finish() {
        pm.fill_path(&path, &paint(color), FillRule::Winding, t, None);
    }
}

struct ArrowHead {
    tip: Point,
    left: Point,
    right: Point,
    /// Midpoint of the perpendicular base (filled shaft ends here).
    base: Point,
}

fn arrow_head_geom(a: Point, b: Point, length: f32, half_w: f32) -> Option<ArrowHead> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return None;
    }
    let ux = dx / len;
    let uy = dy / len;
    let vx = -uy;
    let vy = ux;
    let l = length.min(len * 0.85);
    Some(ArrowHead {
        tip: b,
        left: Point::new(b.x - l * ux + half_w * vx, b.y - l * uy + half_w * vy),
        right: Point::new(b.x - l * ux - half_w * vx, b.y - l * uy - half_w * vy),
        base: Point::new(b.x - l * ux, b.y - l * uy),
    })
}

fn shaft_pts(pts: &[Point], width: i32, kind: ArrowKind) -> Vec<Point> {
    let mut out = pts.to_vec();
    if matches!(kind, ArrowKind::None | ArrowKind::Hollow) {
        return out;
    }
    let Some((l, hw)) = arrow_metrics(width, kind) else {
        return out;
    };
    if out.len() < 2 {
        return out;
    }
    let n = out.len();
    if let Some(head) = arrow_head_geom(out[n - 2], out[n - 1], l, hw) {
        out[n - 1] = head.base;
    }
    out
}

fn draw_arrow_head(pm: &mut Pixmap, sh: &Shape, kind: ArrowKind, t: Transform) {
    let a = sh.pts[sh.pts.len() - 2];
    let bpt = sh.pts[sh.pts.len() - 1];
    let Some((l, hw)) = arrow_metrics(sh.style.width, kind) else {
        return;
    };
    let Some(head) = arrow_head_geom(a, bpt, l, hw) else {
        return;
    };
    match kind {
        ArrowKind::None => {}
        ArrowKind::Hollow => {
            let mut st = stroke_for(sh);
            st.line_cap = LineCap::Round;
            st.line_join = LineJoin::Round;
            let mut pb = PathBuilder::new();
            pb.move_to(head.left.x, head.left.y);
            pb.line_to(head.tip.x, head.tip.y);
            pb.line_to(head.right.x, head.right.y);
            if let Some(path) = pb.finish() {
                pm.stroke_path(&path, &paint(sh.style.color), &st, t, None);
            }
        }
        ArrowKind::Thin | ArrowKind::Thick => {
            let mut pb = PathBuilder::new();
            pb.move_to(head.tip.x, head.tip.y);
            pb.line_to(head.left.x, head.left.y);
            pb.line_to(head.right.x, head.right.y);
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

    fn count_reddish(out: &Raster, xs: std::ops::Range<i32>, ys: std::ops::Range<i32>) -> u32 {
        let mut found = 0u32;
        for y in ys {
            for x in xs.clone() {
                if let Some(c) = out.get(x, y) {
                    if c.r > 180 && c.g < 90 && c.b < 90 {
                        found += 1;
                    }
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
        let found = count_reddish(&out, 38..62, 10..30);
        assert!(found > 16, "arrowhead pixels={found}");
    }

    #[test]
    fn thick_arrow_covers_more_than_thin() {
        let mut base = Raster::new(80, 40);
        base.fill(Rgba::WHITE);
        let thin = count_reddish(
            &compose(&base, &[line_shape(crate::annotation::ArrowKind::Thin)]),
            10..18,
            16..24,
        );
        let thick = count_reddish(
            &compose(&base, &[line_shape(crate::annotation::ArrowKind::Thick)]),
            10..18,
            16..24,
        );
        assert!(thick > thin, "thick shaft={thick} thin shaft={thin}");
    }

    #[test]
    fn hollow_arrow_is_outline_not_a_solid_wedge() {
        let mut base = Raster::new(80, 40);
        base.fill(Rgba::WHITE);
        let out = compose(&base, &[line_shape(crate::annotation::ArrowKind::Hollow)]);
        let edge = count_red(&out, 38..62, 10..30);
        assert!(edge > 12, "hollow outline pixels={edge}");
        // Off the shaft, inside the open V — must stay empty (not a filled triangle).
        let interior = count_red(&out, 42..46, 26..30);
        assert!(
            interior < 8,
            "hollow should not fill the wedge, interior={interior}"
        );
    }

    #[test]
    fn thin_arrow_fills_the_tapered_body() {
        let mut base = Raster::new(80, 40);
        base.fill(Rgba::WHITE);
        let out = compose(&base, &[line_shape(crate::annotation::ArrowKind::Thin)]);
        let core = count_reddish(&out, 20..32, 18..23);
        assert!(core > 8, "tapered body pixels={core}");
    }

    #[test]
    fn thin_arrow_is_thin_at_the_tail_and_thick_at_the_head() {
        let mut base = Raster::new(80, 40);
        base.fill(Rgba::WHITE);
        let out = compose(&base, &[line_shape(crate::annotation::ArrowKind::Thin)]);
        let tail_off = count_reddish(&out, 10..20, 12..16);
        let shaft = count_reddish(&out, 16..28, 18..23);
        let barbs = count_reddish(&out, 22..32, 12..17) + count_reddish(&out, 22..32, 23..28);
        assert!(shaft > 8, "needle shaft pixels={shaft}");
        assert!(
            tail_off < 3,
            "tail must stay a thin shaft, not a wedge, off-axis={tail_off}"
        );
        assert!(barbs > 4, "distinct head barbs pixels={barbs}");
        let tip = count_reddish(&out, 44..52, 16..25);
        assert!(tip > 4, "sharp tip pixels={tip}");
    }

    #[test]
    fn filled_arrow_base_is_perpendicular_to_shaft() {
        let mut base = Raster::new(80, 40);
        base.fill(Rgba::WHITE);
        let out = compose(&base, &[line_shape(crate::annotation::ArrowKind::Thick)]);
        // Horizontal shaft to (50, 20); thick barbs sit on a vertical line behind the tip.
        let barb = count_red(&out, 33..38, 11..16) + count_red(&out, 33..38, 24..29);
        assert!(barb > 4, "perpendicular barbs should paint, pixels={barb}");
        // Behind the base the head must not flare (angle-spread chevrons did).
        let behind = count_red(&out, 10..18, 8..13);
        assert!(
            behind < 3,
            "no chevron flare behind the base, pixels={behind}"
        );
        if let Ok(dir) = std::env::var("PINSHOT_DUMP_ARROWS") {
            let mut canvas = Raster::new(280, 200);
            canvas.fill(Rgba::rgb(48, 48, 48));
            let kinds = [
                crate::annotation::ArrowKind::None,
                crate::annotation::ArrowKind::Thin,
                crate::annotation::ArrowKind::Thick,
                crate::annotation::ArrowKind::Hollow,
            ];
            let shapes: Vec<Shape> = kinds
                .into_iter()
                .enumerate()
                .map(|(i, kind)| {
                    let mut st = Style::for_tool(Tool::Line);
                    st.set_arrow_kind(kind);
                    st.color = Rgba::RED;
                    st.width = 4;
                    let y = 28.0 + i as f32 * 44.0;
                    Shape::new(
                        Tool::Line,
                        vec![Point::new(24.0, y), Point::new(250.0, y)],
                        st,
                    )
                })
                .collect();
            let preview = compose(&canvas, &shapes);
            if let Some(pm) = preview.to_skia() {
                if let Ok(png) = pm.encode_png() {
                    let path = std::path::PathBuf::from(dir.clone()).join("line_arrow_styles.png");
                    let _ =
                        std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
                    let _ = std::fs::write(path, png);
                }
            }
            let mut shot = Raster::new(360, 160);
            shot.fill(Rgba::rgb(48, 48, 48));
            let mut st = Style::for_tool(Tool::Line);
            st.set_arrow_kind(crate::annotation::ArrowKind::Thin);
            st.color = Rgba::RED;
            st.width = 3;
            let preview = compose(
                &shot,
                &[Shape::new(
                    Tool::Line,
                    vec![Point::new(36.0, 80.0), Point::new(280.0, 80.0)],
                    st,
                )],
            );
            if let Some(pm) = preview.to_skia() {
                if let Ok(png) = pm.encode_png() {
                    let path = std::path::PathBuf::from(&dir).join("thin_arrow_snipaste.png");
                    let _ = std::fs::write(path, png);
                }
            }
        }
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

//! Map `pinshot_core::CursorKind` onto the winit window cursor.

use pinshot_core::CursorKind;
use winit::window::{CursorIcon, Window};

pub fn apply(window: &Window, kind: CursorKind) {
    window.set_cursor_visible(true);
    window.set_cursor(match kind {
        CursorKind::Arrow => CursorIcon::Default,
        CursorKind::Crosshair => CursorIcon::Crosshair,
        CursorKind::Text => CursorIcon::Text,
        CursorKind::Cell => CursorIcon::Cell,
        CursorKind::Grab => CursorIcon::Grab,
        CursorKind::Grabbing => CursorIcon::Grabbing,
        CursorKind::Move => CursorIcon::Move,
        CursorKind::NwseResize => CursorIcon::NwseResize,
        CursorKind::NeswResize => CursorIcon::NeswResize,
        CursorKind::EwResize => CursorIcon::EwResize,
        CursorKind::NsResize => CursorIcon::NsResize,
    });
}

pub fn brush_width(annot: &pinshot_core::Annotator, kind: CursorKind) -> Option<f32> {
    use pinshot_core::annotation::{Tool, DrawMode};
    if !matches!(kind, CursorKind::Crosshair | CursorKind::Cell) { return None; }
    let tool = annot.tool?;
    let st = annot.style(tool);
    match tool {
        Tool::Line | Tool::Pen => Some(st.width.max(1) as f32),
        Tool::Marker if st.mode == DrawMode::Brush => Some(st.width.max(1) as f32),
        Tool::Mosaic | Tool::Eraser if st.mode == DrawMode::Brush => Some(st.width.max(1) as f32),
        _ => None,
    }
}

pub fn apply_annot(window: &Window, kind: CursorKind, annot: &pinshot_core::Annotator) {
    apply(window, kind);
    if visual_width(annot, kind).is_some() { window.set_cursor_visible(false); }
}

pub fn visual_width(annot: &pinshot_core::Annotator, kind: CursorKind) -> Option<f32> {
    use pinshot_core::annotation::Tool;
    if !matches!(kind, CursorKind::Crosshair | CursorKind::Cell) { return None; }
    if matches!(annot.tool?, Tool::Text | Tool::Select) { return None; }
    Some(brush_width(annot, kind).unwrap_or(0.0))
}

#[cfg(test)]
#[test]
fn eraser_brush_has_round_size_cursor_but_rect_does_not() {
    use pinshot_core::annotation::{Annotator, DrawMode, Tool};
    let mut annot = Annotator::new();
    annot.set_draw_mode(Tool::Eraser, DrawMode::Brush);
    assert_eq!(brush_width(&annot, CursorKind::Cell), Some(16.0));
    annot.set_draw_mode(Tool::Eraser, DrawMode::Rect);
    assert_eq!(brush_width(&annot, CursorKind::Cell), None);
}

pub fn paint_brush(pm: &mut tiny_skia::Pixmap, pt: pinshot_core::geom::Point, width: f32, annot: &pinshot_core::Annotator) -> pinshot_core::geom::Rect {
    use tiny_skia::{PathBuilder, Stroke, Transform, Paint};
    let radius = (width / 2.0).max(1.5);
    if let Some(rect) = tiny_skia::Rect::from_xywh(pt.x-radius, pt.y-radius, radius*2.0, radius*2.0) {
        if let Some(path) = PathBuilder::from_oval(rect) {
            let mut paint = Paint::default();
            paint.set_color_rgba8(255,255,255,230);
            pm.stroke_path(&path,&paint,&Stroke { width: 3.0, ..Default::default() },Transform::identity(),None);
            paint.set_color_rgba8(30,30,30,255);
            pm.stroke_path(&path,&paint,&Stroke { width: 1.0, ..Default::default() },Transform::identity(),None);
        }
    }
    use pinshot_core::{annotation::{Tool, Effect}, icons::{draw_icon, IconKind}, geom::Rect, color::Rgba};
    let ring = Rect::new(pt.x-radius-3.0,pt.y-radius-3.0,radius*2.0+6.0,radius*2.0+6.0);
    let icon = match annot.tool {
        Some(Tool::Pen) => IconKind::Pen,
        Some(Tool::Marker) => IconKind::Marker,
        Some(Tool::Eraser) => IconKind::Eraser,
        Some(Tool::Mosaic) if annot.style(Tool::Mosaic).effect == Effect::Blur => IconKind::Blur,
        Some(Tool::Mosaic) => IconKind::Mosaic,
        Some(Tool::Ellipse) => IconKind::Ellipse,
        Some(Tool::Rect) => IconKind::Rect,
        Some(Tool::Number) => IconKind::Number,
        Some(Tool::Line) if annot.current_line_arrow().is_arrow() => IconKind::Arrow,
        _ => IconKind::Line,
    };
    let offset = (radius + 5.0).max(9.0);
    let badge = Rect::new(pt.x+offset, pt.y+offset, 22.0, 22.0);
    crate::draw::fill_rounded_rect(pm, badge, 4.0, Rgba::rgba(20,27,38,235));
    draw_icon(pm, icon, badge.inflated(-2.0), Some(Rgba::WHITE));
    ring.union(badge.inflated(1.0))
}

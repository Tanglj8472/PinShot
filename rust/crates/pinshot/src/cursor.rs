use pinshot_core::{
    annotation::{Effect, Tool},
    color::Rgba,
    icons::{draw_icon, IconKind},
    Annotator, CursorKind,
};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::{Rc, Weak},
};
use winit::{
    event_loop::ActiveEventLoop,
    window::{CursorIcon, CustomCursor, Window},
};

#[derive(Clone, Copy, PartialEq)]
struct CursorKey {
    icon: IconKind,
    size: u16,
}

#[derive(Default)]
struct ToolCursors {
    cache: VecDeque<(CursorKey, CustomCursor)>,
    pending: HashMap<winit::window::WindowId, (Weak<Window>, CursorKey)>,
}

thread_local! {
    static TOOLS: RefCell<ToolCursors> = RefCell::default();
}

fn cursor_size(annot: &Annotator, scale: f32) -> u16 {
    // A small readable silhouette plus the stroke width in screen pixels.
    // Even widths 1..12 change size; effect strength is not brush width.
    (12.0 + annot.option_width().max(1) as f32 * scale)
        .round()
        .clamp(12.0, 256.0) as u16
}

fn cursor_source(key: CursorKey) -> Option<winit::window::CustomCursorSource> {
    let size = usize::from(key.size) + 4;
    let mut pm = tiny_skia::Pixmap::new(size as u32, size as u32)?;
    draw_icon(
        &mut pm,
        key.icon,
        pinshot_core::geom::Rect::new(2.0, 2.0, key.size as f32, key.size as f32),
        Some(Rgba::WHITE),
    );
    // A constant one-pixel outline stays clear at every size.
    let alpha: Vec<_> = pm.data().chunks_exact(4).map(|p| p[3]).collect();
    let mut rgba = vec![0; size * size * 4];
    for y in 1..size - 1 {
        for x in 1..size - 1 {
            let i = y * size + x;
            let a = alpha[i];
            let edge = [i - 1, i + 1, i - size, i + size]
                .into_iter()
                .map(|j| alpha[j])
                .max()
                .unwrap()
                .max(a);
            if edge > 0 {
                let gray = (u16::from(a) * 255 / u16::from(edge)) as u8;
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[gray, gray, gray, edge]);
            }
        }
    }
    let (hx, hy) = match key.icon {
        IconKind::Pen | IconKind::Marker => (2 + key.size * 3 / 28, 2 + key.size * 24 / 28),
        _ => (size as u16 / 2, size as u16 / 2),
    };
    CustomCursor::from_rgba(rgba, size as u16, size as u16, hx, hy).ok()
}

/// Coalesce size changes within an event batch; keep only 24 native cursors.
/// Building happens only after a tool/size change, never on ordinary movement.
pub fn flush(event_loop: &ActiveEventLoop) {
    TOOLS.with(|slot| {
        let mut tools = slot.borrow_mut();
        let pending = std::mem::take(&mut tools.pending);
        for (_, (window, key)) in pending {
            let Some(window) = window.upgrade() else {
                continue;
            };
            let cursor = if let Some(i) = tools.cache.iter().position(|(k, _)| *k == key) {
                let entry = tools.cache.remove(i).unwrap();
                let cursor = entry.1.clone();
                tools.cache.push_back(entry);
                cursor
            } else {
                let Some(source) = cursor_source(key) else {
                    continue;
                };
                let cursor = event_loop.create_custom_cursor(source);
                tools.cache.push_back((key, cursor.clone()));
                if tools.cache.len() > 24 {
                    tools.cache.pop_front();
                }
                cursor
            };
            window.set_cursor(cursor);
            window.set_cursor_visible(true);
        }
    });
}

pub fn apply(window: &Window, kind: CursorKind) {
    TOOLS.with(|slot| {
        slot.borrow_mut().pending.remove(&window.id());
    });
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

pub fn apply_annot(window: &Rc<Window>, kind: CursorKind, annot: &Annotator, scale: f32) {
    let icon = match (kind, annot.tool) {
        (CursorKind::Crosshair | CursorKind::Cell, Some(tool)) => match tool {
            Tool::Pen => Some(IconKind::Pen),
            Tool::Marker => Some(IconKind::Marker),
            Tool::Eraser => Some(IconKind::Eraser),
            Tool::Mosaic if annot.style(tool).effect == Effect::Blur => Some(IconKind::Blur),
            Tool::Mosaic => Some(IconKind::Mosaic),
            Tool::Line if annot.current_line_arrow().is_arrow() => Some(IconKind::Arrow),
            Tool::Line => Some(IconKind::Line),
            Tool::Number => Some(IconKind::Number),
            Tool::Rect => Some(IconKind::Rect),
            Tool::Ellipse => Some(IconKind::Ellipse),
            _ => None,
        },
        _ => None,
    };
    let Some(icon) = icon else {
        apply(window, kind);
        return;
    };
    let key = CursorKey {
        icon,
        size: cursor_size(annot, scale),
    };
    TOOLS.with(|slot| {
        let mut tools = slot.borrow_mut();
        if let Some(i) = tools.cache.iter().position(|(k, _)| *k == key) {
            let entry = tools.cache.remove(i).unwrap();
            window.set_cursor(entry.1.clone());
            window.set_cursor_visible(true);
            tools.cache.push_back(entry);
            tools.pending.remove(&window.id());
        } else {
            tools
                .pending
                .insert(window.id(), (Rc::downgrade(window), key));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_tracks_width_and_pin_zoom_but_not_effect_strength() {
        let mut annot = Annotator::default();
        annot.set_tool(Some(Tool::Mosaic));
        annot.style_mut(Tool::Mosaic).width = 40;
        assert_eq!(cursor_size(&annot, 1.0), 52);
        assert_eq!(cursor_size(&annot, 0.5), 32);
        annot.style_mut(Tool::Mosaic).size = 60;
        assert_eq!(cursor_size(&annot, 1.0), 52);
        annot.style_mut(Tool::Mosaic).width = 60;
        assert_eq!(cursor_size(&annot, 1.0), 72);
        for width in 1..12 {
            annot.style_mut(Tool::Mosaic).width = width;
            assert_eq!(cursor_size(&annot, 1.0), 12 + width as u16);
        }
    }

    #[test]
    fn every_tool_cursor_has_valid_pixels_and_hotspot_at_size_limits() {
        use IconKind::*;
        for icon in [
            Pen, Marker, Eraser, Mosaic, Blur, Line, Arrow, Ellipse, Rect, Number,
        ] {
            for size in [12, 28, 60, 256] {
                assert!(cursor_source(CursorKey { icon, size }).is_some());
            }
        }
    }
}

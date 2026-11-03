use pinshot_core::{annotation::{Effect, Tool}, color::Rgba, icons::{draw_icon, IconKind}, Annotator, CursorKind};
use winit::{event_loop::ActiveEventLoop, window::{CursorIcon, CustomCursor, Window}};
use std::cell::OnceCell;

thread_local! {
    static TOOLS: OnceCell<Vec<(IconKind, CustomCursor)>> = const { OnceCell::new() };
}

pub fn init(event_loop: &ActiveEventLoop) {
    TOOLS.with(|slot| { slot.get_or_init(|| {
        use IconKind::*;
        [Pen, Marker, Eraser, Mosaic, Blur, Line, Arrow, Ellipse, Rect, Number]
            .into_iter().filter_map(|icon| {
                let mut pm = tiny_skia::Pixmap::new(32, 32)?;
                draw_icon(&mut pm, icon, pinshot_core::geom::Rect::new(2.0, 2.0, 28.0, 28.0), Some(Rgba::WHITE));
                // Give the silhouette a dark outer edge on bright screenshots.
                let alpha: Vec<_> = pm.data().chunks_exact(4).map(|p| p[3]).collect();
                let mut rgba = vec![0; 32 * 32 * 4];
                for y in 1..31 {
                    for x in 1..31 {
                        let i = y * 32 + x;
                        let a = alpha[i];
                        let edge = [i-1, i+1, i-32, i+32].into_iter().map(|j| alpha[j]).max().unwrap().max(a);
                        if edge > 0 {
                            let gray = (u16::from(a) * 255 / u16::from(edge)) as u8;
                            rgba[i*4..i*4+4].copy_from_slice(&[gray, gray, gray, edge]);
                        }
                    }
                }
                let (hx, hy) = match icon { Pen | Marker => (5, 26), _ => (16, 16) };
                let source = CustomCursor::from_rgba(rgba, 32, 32, hx, hy).ok()?;
                Some((icon, event_loop.create_custom_cursor(source)))
            }).collect()
    }); });
}

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

pub fn apply_annot(window: &Window, kind: CursorKind, annot: &Annotator) {
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
    let custom = TOOLS.with(|slot| slot.get()?.iter().find(|(k, _)| Some(*k) == icon).map(|(_, c)| c.clone()));
    if let Some(cursor) = custom {
        window.set_cursor(cursor);
        window.set_cursor_visible(true);
    } else {
        apply(window, kind);
    }
}

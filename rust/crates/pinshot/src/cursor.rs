//! Map `pinshot_core::CursorKind` onto the winit window cursor.

use pinshot_core::CursorKind;
use winit::window::{CursorIcon, Window};

pub fn apply(window: &Window, kind: CursorKind) {
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

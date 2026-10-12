//! Annotation / overlay cursor mapping.
//!
//! Distinguishes tools and interaction states so the pointer itself shows
//! what the next click will do. Overlay and pin both feed [`CursorHint`].

use crate::annotation::Tool;
use crate::geom::Handle;
use crate::selection::SelState;

/// Portable cursor kinds. The host maps these onto winit `CursorIcon`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Arrow,
    Crosshair,
    Text,
    Cell,
    Grab,
    Grabbing,
    Move,
    NwseResize,
    NeswResize,
    EwResize,
    NsResize,
}

impl CursorKind {
    pub fn for_handle(h: Handle) -> Self {
        match h {
            Handle::Tl | Handle::Br => Self::NwseResize,
            Handle::Tr | Handle::Bl => Self::NeswResize,
            Handle::L | Handle::R => Self::EwResize,
            Handle::T | Handle::B => Self::NsResize,
        }
    }
}

/// Pointer context gathered by overlay / pin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorHint {
    /// Toolbar, confirm, or color picker under the pointer.
    pub over_chrome: bool,
    pub tool: Option<Tool>,
    /// Annotator is dragging a shape (`sel_mode`).
    pub selecting: bool,
    pub selecting_handle: Option<Handle>,
    /// Pointer is over an existing shape.
    pub hover_shape: bool,
    pub hover_shape_handle: Option<Handle>,
    pub editing_text: bool,
    /// Live stroke / shape in progress.
    pub drawing: bool,
    /// Overlay region-selection machine. `None` on pin.
    pub sel_state: Option<SelState>,
    pub hover_sel_handle: Option<Handle>,
    pub over_selection: bool,
    /// Pin window when the toolbar is hidden.
    pub pin_idle: bool,
}

pub fn cursor_for(h: CursorHint) -> CursorKind {
    if h.over_chrome {
        return CursorKind::Arrow;
    }
    if h.editing_text {
        return CursorKind::Text;
    }
    if h.selecting {
        if let Some(handle) = h.selecting_handle {
            return CursorKind::for_handle(handle);
        }
        return CursorKind::Grabbing;
    }
    if let Some(handle) = h.hover_shape_handle {
        return CursorKind::for_handle(handle);
    }
    if h.hover_shape {
        return CursorKind::Grab;
    }
    if h.drawing {
        return cursor_for_tool(h.tool);
    }
    if h.tool.is_none() {
        if let Some(st) = h.sel_state {
            match st {
                SelState::Moving => return CursorKind::Move,
                SelState::Resizing => {
                    if let Some(handle) = h.hover_sel_handle {
                        return CursorKind::for_handle(handle);
                    }
                    return CursorKind::Move;
                }
                SelState::Creating => return CursorKind::Crosshair,
                SelState::Selected | SelState::Idle => {
                    if let Some(handle) = h.hover_sel_handle {
                        return CursorKind::for_handle(handle);
                    }
                    if h.over_selection {
                        return CursorKind::Move;
                    }
                }
            }
        }
        if h.pin_idle {
            return CursorKind::Arrow;
        }
        return CursorKind::Crosshair;
    }
    cursor_for_tool(h.tool)
}

fn cursor_for_tool(tool: Option<Tool>) -> CursorKind {
    match tool {
        Some(Tool::Text) => CursorKind::Text,
        Some(Tool::Mosaic) | Some(Tool::Eraser) => CursorKind::Cell,
        Some(Tool::Select) => CursorKind::Arrow,
        Some(_) | None => CursorKind::Crosshair,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint() -> CursorHint {
        CursorHint::default()
    }

    #[test]
    fn text_tool_is_ibeam() {
        let mut h = hint();
        h.tool = Some(Tool::Text);
        assert_eq!(cursor_for(h), CursorKind::Text);
    }

    #[test]
    fn mosaic_and_eraser_are_cell() {
        let mut h = hint();
        h.tool = Some(Tool::Mosaic);
        assert_eq!(cursor_for(h), CursorKind::Cell);
        h.tool = Some(Tool::Eraser);
        assert_eq!(cursor_for(h), CursorKind::Cell);
    }

    #[test]
    fn draw_tools_are_crosshair() {
        for tool in [
            Tool::Rect,
            Tool::Ellipse,
            Tool::Line,
            Tool::Pen,
            Tool::Marker,
            Tool::Number,
        ] {
            let mut h = hint();
            h.tool = Some(tool);
            assert_eq!(cursor_for(h), CursorKind::Crosshair, "{tool:?}");
        }
    }

    #[test]
    fn select_idle_is_arrow() {
        let mut h = hint();
        h.tool = Some(Tool::Select);
        assert_eq!(cursor_for(h), CursorKind::Arrow);
    }

    #[test]
    fn hover_shape_is_grab() {
        let mut h = hint();
        h.tool = Some(Tool::Select);
        h.hover_shape = true;
        assert_eq!(cursor_for(h), CursorKind::Grab);
        h.tool = Some(Tool::Rect);
        assert_eq!(cursor_for(h), CursorKind::Grab);
    }

    #[test]
    fn drag_shape_is_grabbing() {
        let mut h = hint();
        h.tool = Some(Tool::Select);
        h.selecting = true;
        h.hover_shape = true;
        assert_eq!(cursor_for(h), CursorKind::Grabbing);
    }

    #[test]
    fn shape_handle_uses_resize() {
        let mut h = hint();
        h.tool = Some(Tool::Select);
        h.hover_shape_handle = Some(Handle::Tl);
        assert_eq!(cursor_for(h), CursorKind::NwseResize);
        h.hover_shape_handle = Some(Handle::Tr);
        assert_eq!(cursor_for(h), CursorKind::NeswResize);
        h.hover_shape_handle = Some(Handle::L);
        assert_eq!(cursor_for(h), CursorKind::EwResize);
        h.hover_shape_handle = Some(Handle::T);
        assert_eq!(cursor_for(h), CursorKind::NsResize);
        h.selecting = true;
        h.selecting_handle = Some(Handle::Br);
        assert_eq!(cursor_for(h), CursorKind::NwseResize);
    }

    #[test]
    fn chrome_is_arrow() {
        let mut h = hint();
        h.tool = Some(Tool::Text);
        h.over_chrome = true;
        assert_eq!(cursor_for(h), CursorKind::Arrow);
    }

    #[test]
    fn editing_text_is_ibeam() {
        let mut h = hint();
        h.tool = Some(Tool::Select);
        h.editing_text = true;
        assert_eq!(cursor_for(h), CursorKind::Text);
    }

    #[test]
    fn overlay_region_cursors() {
        let mut h = hint();
        h.sel_state = Some(SelState::Creating);
        assert_eq!(cursor_for(h), CursorKind::Crosshair);
        h.sel_state = Some(SelState::Moving);
        h.over_selection = true;
        assert_eq!(cursor_for(h), CursorKind::Move);
        h.sel_state = Some(SelState::Selected);
        assert_eq!(cursor_for(h), CursorKind::Move);
        h.over_selection = false;
        h.hover_sel_handle = Some(Handle::R);
        assert_eq!(cursor_for(h), CursorKind::EwResize);
        h.hover_sel_handle = None;
        h.sel_state = Some(SelState::Idle);
        assert_eq!(cursor_for(h), CursorKind::Crosshair);
    }

    #[test]
    fn pin_idle_is_arrow() {
        let mut h = hint();
        h.pin_idle = true;
        assert_eq!(cursor_for(h), CursorKind::Arrow);
    }

    #[test]
    fn drawing_keeps_tool_cursor() {
        let mut h = hint();
        h.tool = Some(Tool::Mosaic);
        h.drawing = true;
        assert_eq!(cursor_for(h), CursorKind::Cell);
        h.tool = Some(Tool::Pen);
        assert_eq!(cursor_for(h), CursorKind::Crosshair);
    }
}

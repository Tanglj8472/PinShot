//! Full-screen snip overlay session (one winit window covering the virtual desktop).

use std::num::NonZeroU32;
use std::rc::Rc;

use pinshot_core::annotation::{Annotator, Tool};
use pinshot_core::color::Rgba;
use pinshot_core::geom::{hover_rect, Point, Rect};
use pinshot_core::i18n::tr;
use pinshot_core::magnifier::magnifier_visible;
use pinshot_core::raster::Raster;
use pinshot_core::render::compose;
use pinshot_core::selection::{DetectMode, Nudge, PressKind, SelState, Selection};
use softbuffer::{Context, Surface};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

use crate::capture::Capture;
use crate::detect::translated_windows;
use crate::draw::{paint_overlay, present};
use crate::toolbar::{TbAction, Toolbar};
use crate::win32::{apply_overlay_style, hwnd_of};

pub enum OverlayResult {
    Copy(Raster, Rect),
    Pin(Raster, Rect, (i32, i32)),
    Save(Raster),
    Cancel,
}

pub struct Overlay {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    capture: Capture,
    sel: Selection,
    cursor: Point,
    hover: Rect,
    windows: Vec<Rect>,
    elements: Vec<Rect>,
    annot: Annotator,
    toolbar: Option<Toolbar>,
    hex: bool,
    mag_forced: bool,
    detect: DetectMode,
    toast: Option<String>,
    arrow_on: bool,
    dpr: f32,
    result: Option<OverlayResult>,
    dirty: bool,
}

impl Overlay {
    pub fn start(event_loop: &ActiveEventLoop, capture: Capture) -> Result<Self, String> {
        let (vx, vy) = capture.virtual_origin;
        let w = capture.composed.width;
        let h = capture.composed.height;
        let attrs = Window::default_attributes()
            .with_title("PinShot Snip")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(true)
            .with_position(PhysicalPosition::new(vx, vy))
            .with_inner_size(PhysicalSize::new(w, h))
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
        let window = Rc::new(
            event_loop
                .create_window(attrs)
                .map_err(|e| e.to_string())?,
        );
        apply_overlay_style(hwnd_of(&window));
        window.set_cursor(CursorIcon::Crosshair);
        let context = Context::new(window.clone()).map_err(|e| e.to_string())?;
        let surface = Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
        let windows = translated_windows(capture.virtual_origin);
        let overlay = Rect::new(0.0, 0.0, w as f32, h as f32);
        let mut s = Self {
            window,
            surface,
            capture,
            sel: Selection::new(),
            cursor: Point::ZERO,
            hover: overlay,
            windows,
            elements: Vec::new(),
            annot: Annotator::new(),
            toolbar: None,
            hex: false,
            mag_forced: false,
            detect: DetectMode::Window,
            toast: None,
            arrow_on: true,
            dpr: 1.0,
            result: None,
            dirty: true,
        };
        s.redraw();
        Ok(s)
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn take_result(&mut self) -> Option<OverlayResult> {
        self.result.take()
    }

    fn bounds(&self) -> Rect {
        Rect::new(
            0.0,
            0.0,
            self.capture.composed.width as f32,
            self.capture.composed.height as f32,
        )
    }

    fn img_pt(&self, p: Point) -> Point {
        Point::new(p.x * self.dpr, p.y * self.dpr)
    }

    fn update_hover(&mut self) {
        if self.sel.state != SelState::Idle || self.sel.has_sel() {
            return;
        }
        let elems = if self.detect == DetectMode::Element {
            self.elements.as_slice()
        } else {
            &[]
        };
        self.hover = hover_rect(self.cursor, &self.windows, elems, self.bounds());
    }

    fn relayout_toolbar(&mut self) {
        if self.sel.has_sel() && self.sel.state == SelState::Selected {
            self.toolbar = Some(Toolbar::layout(
                self.sel.normalized(),
                self.bounds(),
                self.annot.tool,
                self.arrow_on,
            ));
        } else {
            self.toolbar = None;
        }
    }

    fn crop(&mut self) -> Option<Raster> {
        if !self.sel.has_sel() {
            return None;
        }
        self.annot.finish();
        let composed = compose(&self.capture.composed, &self.annot.shapes);
        Some(composed.crop(self.sel.normalized()))
    }

    fn finish_copy(&mut self) {
        if let Some(img) = self.crop() {
            self.result = Some(OverlayResult::Copy(img, self.sel.normalized()));
            self.window.set_visible(false);
        }
    }

    fn finish_pin(&mut self) {
        if let Some(img) = self.crop() {
            let (vx, vy) = self.capture.virtual_origin;
            let r = self.sel.normalized();
            self.result = Some(OverlayResult::Pin(
                img,
                r,
                (vx + r.x as i32, vy + r.y as i32),
            ));
            self.window.set_visible(false);
        }
    }

    fn finish_save(&mut self) {
        if let Some(img) = self.crop() {
            self.result = Some(OverlayResult::Save(img));
            self.window.set_visible(false);
        }
    }

    fn cancel(&mut self) {
        self.result = Some(OverlayResult::Cancel);
        self.window.set_visible(false);
    }

    fn apply_tb(&mut self, a: TbAction) {
        match a {
            TbAction::Tool(t) => {
                self.arrow_on = t == Tool::Line && self.annot.style(Tool::Line).arrow;
                if t == Tool::Line {
                    self.annot.set_style_value(Tool::Line, |s| s.arrow = false);
                    self.arrow_on = false;
                }
                self.annot.set_tool(Some(t));
            }
            TbAction::Arrow => {
                self.annot.set_style_value(Tool::Line, |s| s.arrow = true);
                self.annot.set_tool(Some(Tool::Line));
                self.arrow_on = true;
            }
            TbAction::Undo => {
                self.annot.undo();
            }
            TbAction::Redo => {
                self.annot.redo();
            }
            TbAction::Close => self.request_abort(),
            TbAction::Pin => self.finish_pin(),
            TbAction::Save => self.finish_save(),
            TbAction::Copy => self.finish_copy(),
            TbAction::Width(d) => {
                self.annot.adjust_width(d);
            }
            TbAction::Color(i) => {
                if let Some(tool) = self.annot.tool.filter(|t| *t != Tool::Select) {
                    let c = pinshot_core::palette_colors()[i as usize];
                    self.annot.set_style_value(tool, |s| s.color = c);
                }
            }
        }
        self.relayout_toolbar();
        self.dirty = true;
    }

    fn request_abort(&mut self) {
        if !self.sel.has_sel() {
            self.cancel();
            return;
        }
        if crate::dialog::confirm_abort(hwnd_of(&self.window)) {
            self.cancel();
        }
    }

    pub fn handle(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Point::new(position.x as f32, position.y as f32);
                if self.annot.is_brush_live() || self.annot.is_selecting() || self.annot.live.is_some()
                {
                    self.annot.move_to(self.img_pt(self.cursor), false);
                } else if matches!(
                    self.sel.state,
                    SelState::Creating | SelState::Moving | SelState::Resizing
                ) {
                    self.sel.motion(self.cursor, self.bounds());
                } else {
                    self.update_hover();
                    if self.sel.has_sel() && self.sel.normalized().contains(self.cursor) && self.annot.tool.is_none()
                    {
                        self.window.set_cursor(CursorIcon::Move);
                    } else {
                        self.window.set_cursor(CursorIcon::Crosshair);
                    }
                }
                self.dirty = true;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pt = self.cursor;
                if *state == ElementState::Pressed && *button == MouseButton::Left {
                    if let Some(tb) = &self.toolbar {
                        if let Some(a) = tb.hit(pt) {
                            self.apply_tb(a);
                            return;
                        }
                    }
                    if self.sel.has_sel() && self.annot.press(self.img_pt(pt), false) {
                        self.dirty = true;
                        return;
                    }
                    match self.sel.press(pt, self.bounds(), self.annot.tool.is_some()) {
                        PressKind::Ignore => {}
                        _ => self.toolbar = None,
                    }
                    self.dirty = true;
                } else if *state == ElementState::Pressed && *button == MouseButton::Right {
                    if self.sel.has_sel() {
                        self.sel.reset();
                        self.annot.set_tool(None);
                        self.annot.clear();
                        self.toolbar = None;
                        self.update_hover();
                    } else {
                        self.cancel();
                    }
                    self.dirty = true;
                } else if *state == ElementState::Released && *button == MouseButton::Left {
                    if self.annot.live.is_some() || self.annot.is_selecting() {
                        self.annot.release(self.img_pt(pt));
                    } else {
                        self.sel.release(pt, self.hover, self.bounds());
                        self.relayout_toolbar();
                    }
                    self.dirty = true;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 120.0,
                };
                if self.annot.tool.is_some() || self.annot.selected.is_some() {
                    self.annot.adjust_width(if dy > 0.0 { 1 } else { -1 });
                    self.dirty = true;
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.key(&event.logical_key, event.text.as_deref());
            }
            WindowEvent::ModifiersChanged(m) => {
                let mods = m.state();
                self.mag_forced = mods.contains(ModifiersState::ALT);
                self.dirty = true;
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::CloseRequested => self.cancel(),
            _ => {}
        }
    }

    fn key(&mut self, key: &Key, text: Option<&str>) {
        if self.annot.text_edit.is_some() {
            match key {
                Key::Named(NamedKey::Escape) => self.annot.commit_text(true),
                Key::Named(NamedKey::Enter) => self.annot.commit_text(false),
                Key::Named(NamedKey::Backspace) => self.annot.type_text('\u{8}'),
                _ => {
                    if let Some(t) = text {
                        for ch in t.chars() {
                            self.annot.type_text(ch);
                        }
                    }
                }
            }
            self.dirty = true;
            return;
        }
        let mods = /* not stored; use named combos via logical key */ ModifiersState::empty();
        let _ = mods;
        match key {
            Key::Named(NamedKey::Escape) => self.request_abort(),
            Key::Named(NamedKey::Enter) => self.finish_copy(),
            Key::Character(c) if c.eq_ignore_ascii_case("c") => {
                // Ctrl+C is delivered as Character("c") with modifiers on some backends;
                // also handle colour copy when no modifiers by checking selection.
                if self.sel.has_sel() && self.annot.tool.is_none() {
                    self.finish_copy();
                } else {
                    self.copy_color();
                }
            }
            Key::Character(c) if c.eq_ignore_ascii_case("s") => self.finish_save(),
            Key::Character(c) if c.eq_ignore_ascii_case("t") => self.finish_pin(),
            Key::Named(NamedKey::F3) => self.finish_pin(),
            Key::Character(c) if c.eq_ignore_ascii_case("z") => {
                self.annot.undo();
                self.dirty = true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("y") => {
                self.annot.redo();
                self.dirty = true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("a") => {
                self.sel.set_rect(self.bounds(), self.bounds());
                self.relayout_toolbar();
                self.dirty = true;
            }
            Key::Character(c) if c == "n" || c == "N" => {
                if self.sel.has_sel() {
                    self.annot.set_tool(Some(Tool::Number));
                    self.relayout_toolbar();
                    self.dirty = true;
                }
            }
            Key::Named(NamedKey::Shift) => {
                self.hex = !self.hex;
                self.dirty = true;
            }
            Key::Named(NamedKey::Tab) => {
                self.detect = self.detect.toggle();
                self.toast = Some(tr(&format!("Detection: {}", self.detect.id())));
                self.update_hover();
                self.dirty = true;
            }
            Key::Named(NamedKey::ArrowLeft) => self.sel.nudge(-1.0, 0.0, Nudge::Move, self.bounds()),
            Key::Named(NamedKey::ArrowRight) => self.sel.nudge(1.0, 0.0, Nudge::Move, self.bounds()),
            Key::Named(NamedKey::ArrowUp) => self.sel.nudge(0.0, -1.0, Nudge::Move, self.bounds()),
            Key::Named(NamedKey::ArrowDown) => self.sel.nudge(0.0, 1.0, Nudge::Move, self.bounds()),
            _ => {}
        }
        if matches!(
            key,
            Key::Named(NamedKey::ArrowLeft)
                | Key::Named(NamedKey::ArrowRight)
                | Key::Named(NamedKey::ArrowUp)
                | Key::Named(NamedKey::ArrowDown)
        ) {
            self.relayout_toolbar();
            self.dirty = true;
        }
    }

    fn copy_color(&mut self) {
        let (c, _) = pinshot_core::sample_color(&self.capture.composed, self.cursor, self.dpr);
        let txt = c.format(self.hex);
        let _ = crate::clipboard::write_text(&txt);
        self.toast = Some(tr(&format!("Copied {txt}")));
        self.dirty = true;
        // Python exits shortly after colour copy.
        self.cancel();
        let _ = Rgba::BLACK;
    }

    pub fn on_double_click(&mut self) {
        if self.annot.tool.is_some() {
            return;
        }
        if self.sel.has_sel() && self.sel.normalized().contains(self.cursor) {
            self.finish_copy();
        }
    }

    pub fn redraw(&mut self) {
        let w = self.capture.composed.width.max(1);
        let h = self.capture.composed.height.max(1);
        if self
            .surface
            .resize(
                NonZeroU32::new(w).unwrap(),
                NonZeroU32::new(h).unwrap(),
            )
            .is_err()
        {
            return;
        }
        let over_tb = self
            .toolbar
            .as_ref()
            .map(|t| t.contains(self.cursor))
            .unwrap_or(false);
        let mag = magnifier_visible(
            self.mag_forced,
            self.annot.tool.is_some(),
            false,
            over_tb,
            matches!(self.sel.state, SelState::Creating | SelState::Resizing),
            self.sel.state == SelState::Idle && !self.sel.has_sel(),
        );
        let pm = paint_overlay(
            &self.capture.composed,
            &self.sel,
            self.hover,
            self.cursor,
            self.dpr,
            self.hex,
            mag,
            &self.annot,
            self.toolbar.as_ref(),
            self.toast.as_deref(),
            110,
        );
        if let Ok(mut buf) = self.surface.buffer_mut() {
            present(&mut buf, &pm);
            let _ = buf.present();
        }
        self.dirty = false;
    }

    pub fn pump_redraw(&mut self) {
        if self.dirty {
            self.window.request_redraw();
        }
    }
}

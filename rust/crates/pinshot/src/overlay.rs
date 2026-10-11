//! Full-screen snip overlay session (one winit window covering the virtual desktop).

use std::rc::Rc;
use std::time::{Duration, Instant};

use pinshot_core::annotation::{Annotator, Tool};
use pinshot_core::color::Rgba;
use pinshot_core::geom::{hover_rect, Point, Rect};
use pinshot_core::i18n::tr;
use pinshot_core::magnifier::magnifier_visible;
use pinshot_core::raster::Raster;
use pinshot_core::render::compose;
use pinshot_core::history::{HistoryEntry, HistoryView, SnipHistory};
use pinshot_core::selection::{DetectMode, Nudge, PressKind, SelState, Selection};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

use crate::capture::Capture;
use crate::detect::{element_source, translated_elements, translated_windows};
#[cfg(not(windows))]
use crate::draw::present;
use crate::draw::OverlayFront;
use crate::native::{self, hwnd_of};
use crate::toolbar::{TbAction, Toolbar};

pub enum OverlayResult {
    Copy(Raster, Rect),
    Pin(Raster, Rect, (i32, i32)),
    Save(Raster),
    Ocr(Raster, bool),
    Cancel,
}

pub struct Overlay {
    window: Rc<Window>,
    front: OverlayFront,
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
    toast_until: Option<Instant>,
    arrow_on: bool,
    dpr: f32,
    result: Option<OverlayResult>,
    dirty: bool,
    pub history: SnipHistory,
    live_shot: Option<Raster>,
    translate_on_select: bool,
    show_cursor: bool,
    mods: ModifiersState,
    history_max: u32,
    #[cfg(unix)]
    key_grab: Option<crate::x11util::OverlayKeyGrab>,
    #[cfg(not(windows))]
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
}

impl Overlay {
    pub fn start(
        event_loop: &ActiveEventLoop,
        capture: Capture,
        history: SnipHistory,
        history_max: u32,
        translate_on_select: bool,
        proxy: winit::event_loop::EventLoopProxy<crate::event::AppEvent>,
        #[cfg(unix)]
        pre_hold: Option<crate::x11util::InputHold>,
    ) -> Result<Self, String> {
        let (vx, vy) = capture.virtual_origin;
        let w = capture.composed.width;
        let h = capture.composed.height;
        let mut attrs = Window::default_attributes()
            .with_title("PinShot Snip")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_position(PhysicalPosition::new(vx, vy))
            .with_inner_size(PhysicalSize::new(w, h))
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            use winit::platform::x11::WindowAttributesExtX11;
            attrs = attrs.with_override_redirect(true);
        }
        let window = Rc::new(
            event_loop
                .create_window(attrs)
                .map_err(|e| e.to_string())?,
        );
        native::apply_overlay_style(&window);
        window.set_cursor(CursorIcon::Crosshair);
        window.set_ime_allowed(true);
        #[cfg(not(windows))]
        let surface = {
            let ctx = softbuffer::Context::new(window.clone()).map_err(|e| format!("{e:?}"))?;
            Some(softbuffer::Surface::new(&ctx, window.clone()).map_err(|e| format!("{e:?}"))?)
        };
        let front = OverlayFront::new(&capture.composed, 110)?;
        let windows = translated_windows(capture.virtual_origin);
        let overlay = Rect::new(0.0, 0.0, w as f32, h as f32);
        let mut s = Self {
            window,
            front,
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
            toast_until: None,
            arrow_on: true,
            dpr: 1.0,
            result: None,
            dirty: true,
            history,
            live_shot: None,
            translate_on_select,
            show_cursor: false,
            mods: ModifiersState::empty(),
            history_max: history_max.max(1),
            #[cfg(unix)]
            key_grab: None,
            #[cfg(not(windows))]
            surface,
        };
        s.window.set_visible(true);
        native::focus_window(&s.window);
        #[cfg(unix)]
        {
            s.key_grab = native::grab_overlay_keys(&s.window, proxy);
            // Drop the pre-capture InputHold only after the overlay is mapped.
            // Never XGrabPointer from another client / Xlib Display: those
            // events never reach winit's XCB loop. Button1 is recovered via
            // QueryPointer (see recover_pointer_drag).
            drop(pre_hold);
            crate::logutil::log("overlay mapped (pointer left on winit window)");
        }
        #[cfg(not(unix))]
        {
            let _ = proxy;
        }
        if let Some((x, y, down)) = native::pointer_in_window(&s.window) {
            let (vx, vy) = s.capture.virtual_origin;
            s.cursor = Point::new((x - vx) as f32, (y - vy) as f32);
            if down && s.sel.state == pinshot_core::selection::SelState::Idle {
                let _ = s.sel.press(s.cursor, s.bounds(), false);
                crate::logutil::log(&format!(
                    "recover Button1 at start ({:.0},{:.0})",
                    s.cursor.x, s.cursor.y
                ));
            }
        }
        s.redraw();
        Ok(s)
    }

    fn show_toast(&mut self, text: String) {
        crate::logutil::log(&format!("toast: {text}"));
        self.toast = Some(text);
        self.toast_until = Some(Instant::now() + Duration::from_millis(1200));
        self.dirty = true;
    }

    fn expire_toast(&mut self) {
        if let Some(t) = self.toast_until {
            if Instant::now() >= t {
                self.toast = None;
                self.toast_until = None;
                self.dirty = true;
            }
        }
    }

    fn sync_ime_grab(&mut self) {
        #[cfg(unix)]
        if let Some(g) = &self.key_grab {
            g.set_pause_keys(self.annot.text_edit.is_some());
        }
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
            let mut tb = Toolbar::layout(
                self.sel.normalized(),
                self.bounds(),
                self.annot.tool,
                self.arrow_on,
            );
            if let Some(t) = self.annot.tool {
                tb.set_width_value(self.annot.style(t).width);
            }
            self.toolbar = Some(tb);
        } else {
            self.toolbar = None;
        }
    }

    fn crop(&mut self) -> Option<Raster> {
        if !self.sel.has_sel() {
            return None;
        }
        self.annot.finish();
        let mut composed = compose(&self.capture.composed, &self.annot.shapes);
        if self.show_cursor {
            if let Some(mut pm) = composed.to_skia() {
                crate::draw::paint_pointer(&mut pm, self.img_pt(self.cursor));
                if let Some(img) = crate::draw::pixmap_to_raster(&pm) {
                    composed = img;
                }
            }
        }
        Some(composed.crop(self.sel.normalized()))
    }

    fn remember(&mut self) {
        self.history.push(
            HistoryEntry {
                rect: self.sel.normalized(),
                image: self.capture.composed.clone(),
            },
            self.history_max,
        );
    }

    fn finish_copy(&mut self) {
        if let Some(img) = self.crop() {
            self.remember();
            crate::logutil::log(&format!(
                "overlay copy {}x{}",
                img.width, img.height
            ));
            self.result = Some(OverlayResult::Copy(img, self.sel.normalized()));
            self.window.set_visible(false);
        }
    }

    fn finish_pin(&mut self) {
        if let Some(img) = self.crop() {
            self.remember();
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
            self.remember();
            self.result = Some(OverlayResult::Save(img));
            self.window.set_visible(false);
        }
    }

    fn finish_ocr(&mut self, translate: bool) {
        if !self.sel.has_sel() {
            return;
        }
        let img = self.capture.composed.crop(self.sel.normalized());
        self.remember();
        self.result = Some(OverlayResult::Ocr(img, translate));
        self.window.set_visible(false);
    }

    fn step_history(&mut self, dir: i32) {
        if self.history.entries.is_empty() {
            return;
        }
        if self.live_shot.is_none() {
            self.live_shot = Some(self.capture.composed.clone());
        }
        match self.history.step(dir) {
            HistoryView::Live => {
                if let Some(live) = self.live_shot.clone() {
                    self.apply_replay(&live, None);
                }
                self.show_toast(tr("Snip history"));
            }
            HistoryView::Replay(e) => {
                let img = e.image.clone();
                let rect = e.rect;
                self.apply_replay(&img, Some(rect));
                self.show_toast(tr("Snip history"));
            }
        }
    }

    fn reuse_last(&mut self) {
        if let Some(e) = self.history.last().cloned() {
            if self.live_shot.is_none() {
                self.live_shot = Some(self.capture.composed.clone());
            }
            self.apply_replay(&e.image, Some(e.rect));
            self.show_toast(tr("Last snip area"));
        }
    }

    fn apply_replay(&mut self, img: &Raster, rect: Option<Rect>) {
        self.capture.composed = img.clone();
        let _ = self.front.replace_shot(img, hwnd_of(&self.window));
        self.annot.clear();
        if let Some(r) = rect {
            self.sel.set_rect(r, self.bounds());
            self.relayout_toolbar();
        } else {
            self.sel.reset();
            self.toolbar = None;
        }
        self.dirty = true;
    }

    fn load_elements(&mut self) {
        if self.elements.is_empty() {
            self.elements = translated_elements(self.capture.virtual_origin);
        }
    }

    fn cancel(&mut self) {
        self.result = Some(OverlayResult::Cancel);
        self.window.set_visible(false);
    }

    fn apply_tb(&mut self, a: TbAction) {
        match a {
            TbAction::Tool(t) => {
                self.annot.set_tool(Some(t));
                if t == Tool::Line {
                    self.annot.set_style_default(Tool::Line, |s| s.arrow = false);
                    self.arrow_on = false;
                } else {
                    self.arrow_on = false;
                }
            }
            TbAction::Arrow => {
                self.annot.set_tool(Some(Tool::Line));
                self.annot.set_style_default(Tool::Line, |s| s.arrow = true);
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
            TbAction::Ocr => self.finish_ocr(false),
            TbAction::Translate => self.finish_ocr(true),
            TbAction::Check => {}
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
        let skip = pinshot_core::config::load()
            .map(|c| c.skip_abort_confirm)
            .unwrap_or(false);
        if skip || crate::dialog::confirm_abort(hwnd_of(&self.window)) {
            self.cancel();
        }
    }

    pub fn handle(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let next = Point::new(position.x as f32, position.y as f32);
                let same_px = next.x.round() == self.cursor.x.round()
                    && next.y.round() == self.cursor.y.round();
                self.cursor = next;
                if self.annot.is_brush_live() || self.annot.is_selecting() || self.annot.live.is_some()
                {
                    self.annot.move_to(self.img_pt(self.cursor), false);
                    self.dirty = true;
                } else if matches!(
                    self.sel.state,
                    SelState::Creating | SelState::Moving | SelState::Resizing
                ) {
                    self.sel.motion(self.cursor, self.bounds());
                    self.dirty = true;
                } else {
                    let prev_hover = self.hover;
                    self.update_hover();
                    if self.sel.has_sel() && self.sel.normalized().contains(self.cursor) && self.annot.tool.is_none()
                    {
                        self.window.set_cursor(CursorIcon::Move);
                    } else {
                        self.window.set_cursor(CursorIcon::Crosshair);
                    }
                    if !same_px || prev_hover != self.hover {
                        self.dirty = true;
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pt = self.cursor;
                if *state == ElementState::Pressed {
                    native::focus_window(&self.window);
                }
                if *state == ElementState::Pressed && *button == MouseButton::Left {
                    if let Some(tb) = &self.toolbar {
                        if let Some(a) = tb.hit(pt) {
                            self.apply_tb(a);
                            return;
                        }
                    }
                    if self.sel.has_sel() && self.annot.press(self.img_pt(pt), false) {
                        self.sync_ime_grab();
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
                        if self.translate_on_select && self.sel.has_sel() {
                            self.finish_ocr(true);
                        }
                    }
                    self.dirty = true;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let angle = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (*y * 120.0) as i32,
                    MouseScrollDelta::PixelDelta(p) => p.y as i32,
                };
                if self.annot.wheel_at(self.img_pt(self.cursor), angle) {
                    self.dirty = true;
                }
            }
            WindowEvent::Ime(winit::event::Ime::Commit(s)) => {
                for ch in s.chars() {
                    self.annot.type_text(ch);
                }
                self.dirty = true;
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                native::focus_window(&self.window);
                self.key(&event.logical_key, event.text.as_deref());
            }
            WindowEvent::Focused(focused) => {
                if *focused {
                    native::focus_window(&self.window);
                    #[cfg(unix)]
                    if let Some(g) = &self.key_grab {
                        g.kick();
                    }
                } else if self.result.is_none() {
                    native::focus_window(&self.window);
                    #[cfg(unix)]
                    if let Some(g) = &self.key_grab {
                        g.kick();
                    }
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                self.mag_forced = self.mods.contains(ModifiersState::ALT);
                self.dirty = true;
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::CloseRequested => self.cancel(),
            _ => {}
        }
    }

    pub fn handle_x11_key(&mut self, keysym: u32, mods: u16, pressed: bool) {
        self.mods = x11_mods_to_winit(mods);
        self.mag_forced = self.mods.contains(ModifiersState::ALT);
        if !pressed {
            self.dirty = true;
            return;
        }
        let Some((key, text)) = keysym_to_winit(keysym) else {
            return;
        };
        self.key(&key, text.as_deref());
    }

    fn key(&mut self, key: &Key, text: Option<&str>) {
        if self.annot.text_edit.is_some() {
            let ctrl = self.mods.control_key();
            match key {
                Key::Named(NamedKey::Escape) => self.annot.commit_text(true),
                Key::Named(NamedKey::Enter) => self.annot.commit_text(false),
                Key::Named(NamedKey::Backspace) => self.annot.type_text('\u{8}'),
                _ if ctrl => {}
                _ => {
                    if let Some(t) = text {
                        for ch in t.chars() {
                            self.annot.type_text(ch);
                        }
                    }
                }
            }
            self.sync_ime_grab();
            self.dirty = true;
            return;
        }
        let ctrl = self.mods.control_key();
        let _shift = self.mods.shift_key();
        match key {
            Key::Named(NamedKey::Escape) => self.request_abort(),
            Key::Named(NamedKey::Enter) => {
                if self.sel.has_sel() {
                    self.finish_copy();
                } else {
                    crate::logutil::log("Enter with no selection");
                }
            }
            Key::Character(c) if c.eq_ignore_ascii_case("c") && ctrl => self.finish_copy(),
            Key::Named(NamedKey::F3) => self.finish_pin(),
            Key::Character(c) if c.eq_ignore_ascii_case("t") && ctrl => self.finish_pin(),
            Key::Character(c) if c.eq_ignore_ascii_case("s") && ctrl => self.finish_save(),
            Key::Character(c) if c.eq_ignore_ascii_case("z") && ctrl => {
                self.annot.undo();
                self.dirty = true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("y") && ctrl => {
                self.annot.redo();
                self.dirty = true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("a") && ctrl => {
                self.sel.set_rect(self.bounds(), self.bounds());
                self.relayout_toolbar();
                self.dirty = true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("r") => self.reuse_last(),
            Key::Character(c) if c.eq_ignore_ascii_case("c") && !ctrl => self.copy_color(),
            Key::Named(NamedKey::Shift) => {
                self.hex = !self.hex;
                self.dirty = true;
            }
            Key::Named(NamedKey::Tab) => {
                self.detect = self.detect.toggle();
                if self.detect == DetectMode::Element {
                    self.load_elements();
                    self.show_toast(format!(
                        "{} ({})",
                        tr("Element detection"),
                        element_source()
                    ));
                } else {
                    self.show_toast(tr("Window detection"));
                }
                self.update_hover();
                self.dirty = true;
            }
            Key::Character(c) if c == "`" || c == "~" => {
                self.show_cursor = !self.show_cursor;
                self.window.set_cursor_visible(!self.show_cursor);
                self.show_toast(tr(if self.show_cursor {
                    "Cursor shown"
                } else {
                    "Cursor hidden"
                }));
                self.dirty = true;
            }
            Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace)
                if self.annot.tool == Some(Tool::Select) =>
            {
                self.annot.delete_selected();
                self.dirty = true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("v") && self.sel.has_sel() => {
                self.annot.set_tool(Some(Tool::Select));
                self.relayout_toolbar();
                self.dirty = true;
            }
            Key::Character(c) if (c == "n" || c == "N") && self.sel.has_sel() && !ctrl => {
                self.annot.set_tool(Some(Tool::Number));
                self.relayout_toolbar();
                self.dirty = true;
            }
            Key::Character(c) if c == "1" && self.annot.tool.is_some() => {
                self.annot.adjust_width(-1);
                self.dirty = true;
            }
            Key::Character(c) if c == "2" && self.annot.tool.is_some() => {
                self.annot.adjust_width(1);
                self.dirty = true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("a") && !ctrl => self.move_cursor(-1.0, 0.0),
            Key::Character(c) if c.eq_ignore_ascii_case("d") && !ctrl => self.move_cursor(1.0, 0.0),
            Key::Character(c) if c.eq_ignore_ascii_case("w") && !ctrl => self.move_cursor(0.0, -1.0),
            Key::Character(c) if c.eq_ignore_ascii_case("s") && !ctrl => self.move_cursor(0.0, 1.0),
            Key::Named(NamedKey::ArrowLeft) => self.nudge_key(-1.0, 0.0),
            Key::Named(NamedKey::ArrowRight) => self.nudge_key(1.0, 0.0),
            Key::Named(NamedKey::ArrowUp) => self.nudge_key(0.0, -1.0),
            Key::Named(NamedKey::ArrowDown) => self.nudge_key(0.0, 1.0),
            Key::Character(c) if c == "," => self.step_history(-1),
            Key::Character(c) if c == "." => self.step_history(1),
            _ => {}
        }
    }

    fn move_cursor(&mut self, dx: f32, dy: f32) {
        let b = self.bounds();
        self.cursor = Point::new(
            pinshot_core::geom::clamp_f32(self.cursor.x + dx, 0.0, (b.w - 1.0).max(0.0)),
            pinshot_core::geom::clamp_f32(self.cursor.y + dy, 0.0, (b.h - 1.0).max(0.0)),
        );
        let _ = self
            .window
            .set_cursor_position(PhysicalPosition::new(self.cursor.x as f64, self.cursor.y as f64));
        if self.sel.state == SelState::Creating {
            self.sel.motion(self.cursor, b);
        } else {
            self.update_hover();
        }
        self.dirty = true;
    }

    fn nudge_key(&mut self, dx: f32, dy: f32) {
        if !self.sel.has_sel() {
            self.move_cursor(dx, dy);
            return;
        }
        let mode = if self.mods.control_key() {
            Nudge::Grow
        } else if self.mods.shift_key() {
            Nudge::Shrink
        } else {
            Nudge::Move
        };
        self.sel.nudge(dx, dy, mode, self.bounds());
        self.relayout_toolbar();
        self.dirty = true;
    }

    fn copy_color(&mut self) {
        let (c, _) = pinshot_core::sample_color(&self.capture.composed, self.cursor, self.dpr);
        let txt = c.format(self.hex);
        let _ = crate::clipboard::write_text(&txt);
        self.show_toast(tr(&format!("Copied {txt}")));
        self.dirty = true;
        // Python exits shortly after colour copy.
        self.cancel();
        let _ = Rgba::BLACK;
    }

    pub fn on_double_click(&mut self) {
        if self.sel.has_sel() {
            let hit = self.annot.hit_test(self.img_pt(self.cursor));
            if let Some(i) = hit {
                if self.annot.shapes.get(i).map(|s| s.kind) == Some(Tool::Text) {
                    self.annot.begin_edit_text(i);
                    self.dirty = true;
                    return;
                }
            }
        }
        if self.annot.tool.is_some() {
            return;
        }
        if self.sel.has_sel() && self.sel.normalized().contains(self.cursor) {
            self.finish_copy();
        }
    }

    pub fn redraw(&mut self) {
        self.expire_toast();
        if !self.dirty {
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
        self.front.paint(
            hwnd_of(&self.window),
            &self.sel,
            self.hover,
            self.cursor,
            self.dpr,
            self.hex,
            mag,
            &            self.annot,
            self.toolbar.as_ref(),
            self.toast.as_deref(),
            self.show_cursor,
            &self.capture.composed,
        );
        #[cfg(not(windows))]
        if let Some(surface) = &mut self.surface {
            let pm = self.front.pixmap();
            if let (Some(nw), Some(nh)) = (std::num::NonZeroU32::new(pm.width()), std::num::NonZeroU32::new(pm.height())) {
                if surface.resize(nw, nh).is_ok() {
                    if let Ok(mut buf) = surface.buffer_mut() {
                        present(&mut buf, pm);
                        let _ = buf.present();
                    }
                }
            }
        }
        self.window.set_visible(true);
        self.dirty = false;
    }

    pub fn pump_redraw(&mut self) {
        self.recover_pointer_drag();
        self.expire_toast();
        if self.dirty {
            self.window.request_redraw();
        }
    }

    /// If ButtonPress landed on the window underneath before the overlay
    /// mapped, adopt the still-held Button1 as a selection drag.
    fn recover_pointer_drag(&mut self) {
        let Some((x, y, down)) = native::pointer_in_window(&self.window) else {
            return;
        };
        let vx = self.capture.virtual_origin.0;
        let vy = self.capture.virtual_origin.1;
        let pt = Point::new((x - vx) as f32, (y - vy) as f32);
        self.cursor = pt;
        if down && self.sel.state == SelState::Idle && !self.sel.has_sel() && self.annot.tool.is_none()
        {
            let _ = self.sel.press(pt, self.bounds(), false);
            crate::logutil::log(&format!("recover Button1 drag ({:.0},{:.0})", pt.x, pt.y));
            self.dirty = true;
        } else if down && self.sel.state == SelState::Creating {
            self.sel.motion(pt, self.bounds());
            self.dirty = true;
        } else if !down && self.sel.state == SelState::Creating {
            self.sel.release(pt, self.hover, self.bounds());
            self.relayout_toolbar();
            if self.sel.has_sel() {
                let r = self.sel.normalized();
                crate::logutil::log(&format!(
                    "recover release sel {:.0}x{:.0}",
                    r.w, r.h
                ));
            }
            self.dirty = true;
        }
    }

    pub fn next_wake(&self) -> Option<Instant> {
        let poll = Instant::now() + Duration::from_millis(16);
        match self.toast_until {
            Some(t) => Some(if t < poll { t } else { poll }),
            None => Some(poll),
        }
    }
}

fn x11_mods_to_winit(mods: u16) -> ModifiersState {
    let mut m = ModifiersState::empty();
    if mods & 1 != 0 {
        m |= ModifiersState::SHIFT;
    }
    if mods & 4 != 0 {
        m |= ModifiersState::CONTROL;
    }
    if mods & 8 != 0 {
        m |= ModifiersState::ALT;
    }
    if mods & 64 != 0 {
        m |= ModifiersState::SUPER;
    }
    m
}

fn keysym_to_winit(keysym: u32) -> Option<(Key, Option<String>)> {
    let named = match keysym {
        0xff1b => Some(NamedKey::Escape),
        0xff0d | 0xff8d => Some(NamedKey::Enter),
        0xff08 => Some(NamedKey::Backspace),
        0xffff | 0xff9f => Some(NamedKey::Delete),
        0xff09 => Some(NamedKey::Tab),
        0xffe1 | 0xffe2 => Some(NamedKey::Shift),
        0xff51 | 0xff96 => Some(NamedKey::ArrowLeft),
        0xff52 | 0xff97 => Some(NamedKey::ArrowUp),
        0xff53 | 0xff98 => Some(NamedKey::ArrowRight),
        0xff54 | 0xff99 => Some(NamedKey::ArrowDown),
        0xffbe => Some(NamedKey::F1),
        0xffbf => Some(NamedKey::F2),
        0xffc0 => Some(NamedKey::F3),
        0xffc1 => Some(NamedKey::F4),
        0xffc2 => Some(NamedKey::F5),
        0xffc3 => Some(NamedKey::F6),
        0x0020 => {
            return Some((Key::Character(" ".into()), Some(" ".into())));
        }
        _ => None,
    };
    if let Some(n) = named {
        return Some((Key::Named(n), None));
    }
    let ch = char::from_u32(keysym)?;
    if ch.is_control() {
        return None;
    }
    let s = ch.to_string();
    Some((Key::Character(s.clone().into()), Some(s)))
}

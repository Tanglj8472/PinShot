//! Frameless always-on-top pin windows.

use std::num::NonZeroU32;
use std::rc::Rc;

use pinshot_core::annotation::{Annotator, Tool};
use pinshot_core::geom::{Point, Rect};
use pinshot_core::groups;
use pinshot_core::i18n::tr;
use pinshot_core::raster::Raster;
use pinshot_core::render::compose;
use softbuffer::{Context, Surface};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};
use crate::draw::{paint_pin, paint_toolbar, present};
use crate::native;
use crate::toolbar::{TbAction, Toolbar};

#[cfg(windows)]
use crate::win32::{hwnd_of, wide};
#[cfg(windows)]
use windows::Win32::Foundation::HWND;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, TrackPopupMenu, HMENU, MF_CHECKED, MF_SEPARATOR,
    MF_STRING, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
};

const ID_COPY: usize = 1;
const ID_SAVE: usize = 2;
const ID_CLOSE: usize = 3;
const ID_CT: usize = 4;
const ID_Z25: usize = 10;
const ID_Z50: usize = 11;
const ID_Z75: usize = 12;
const ID_Z100: usize = 13;
const ID_Z150: usize = 14;
const ID_Z200: usize = 15;
const ID_Z400: usize = 16;
const ID_TOOLBAR: usize = 5;

pub struct PinWin {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    pub img: Raster,
    pub annot: Annotator,
    pub scale: f32,
    pub opacity: f32,
    pub click_through: bool,
    glow: f32,
    drag: Option<Point>,
    last_cursor: Point,
    pub closed: bool,
    dirty: bool,
    pub group: String,
    pub annotating: bool,
    toolbar: Option<Toolbar>,
    arrow_on: bool,
    toast: Option<String>,
}

impl PinWin {
    pub fn create(
        event_loop: &ActiveEventLoop,
        img: Raster,
        pos: (i32, i32),
    ) -> Result<Self, String> {
        let w = img.width.max(8);
        let h = img.height.max(8);
        let attrs = Window::default_attributes()
            .with_title("PinShot Pin")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(true)
            .with_position(PhysicalPosition::new(pos.0, pos.1))
            .with_inner_size(PhysicalSize::new(w, h))
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
        let window = Rc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        native::apply_pin_style(&window);
        window.set_ime_allowed(true);
        let context = Context::new(window.clone()).map_err(|e| e.to_string())?;
        let surface = Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
        let mut pin = Self {
            window,
            surface,
            img,
            annot: Annotator::new(),
            scale: 1.0,
            opacity: 1.0,
            click_through: false,
            glow: 1.0,
            drag: None,
            last_cursor: Point::ZERO,
            closed: false,
            dirty: true,
            group: groups::DEFAULT.into(),
            annotating: false,
            toolbar: None,
            arrow_on: true,
            toast: None,
        };
        pin.redraw();
        Ok(pin)
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    #[cfg(windows)]
    pub fn hwnd(&self) -> HWND {
        hwnd_of(&self.window)
    }

    pub fn pos(&self) -> (i32, i32) {
        self.window
            .outer_position()
            .map(|p| (p.x, p.y))
            .unwrap_or((40, 40))
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    pub fn show(&self) {
        self.window.set_visible(true);
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible().unwrap_or(true)
    }

    pub fn set_scale(&mut self, s: f32) {
        let tl = self.window.outer_position().ok();
        self.scale = s.clamp(0.05, 8.0);
        let w = ((self.img.width as f32) * self.scale).round().max(8.0) as u32;
        let h = ((self.img.height as f32) * self.scale).round().max(8.0) as u32;
        let _ = self.window.request_inner_size(PhysicalSize::new(w, h));
        if let Some(p) = tl {
            self.window.set_outer_position(p);
        }
        self.dirty = true;
    }

    pub fn set_opacity(&mut self, o: f32) {
        self.opacity = o.clamp(0.1, 1.0);
        native::set_opacity(&self.window, self.opacity);
    }

    pub fn set_click_through(&mut self, on: bool) {
        self.click_through = on;
        native::set_click_through(&self.window, on, self.opacity);
        self.dirty = true;
    }

    pub fn rendered(&self) -> Raster {
        compose(&self.img, &self.annot.shapes)
    }

    pub fn tick_glow(&mut self) {
        if self.glow > 0.0 {
            self.glow = (self.glow - 0.05).max(0.0);
            self.dirty = true;
        }
    }

    pub fn show_toolbar(&mut self, on: bool) {
        self.annotating = on;
        if on {
            if self.annot.tool.is_none() {
                self.annot.set_tool(Some(Tool::Rect));
            }
            self.relayout_tb();
        } else {
            self.annot.set_tool(None);
            self.toolbar = None;
        }
        self.apply_size();
        self.dirty = true;
    }

    fn relayout_tb(&mut self) {
        let (iw, ih) = self.img_px();
        let sel = Rect::new(0.0, 0.0, iw as f32, ih as f32);
        let over = Rect::new(0.0, 0.0, iw.max(420) as f32, ih as f32 + 90.0);
        self.toolbar = Some(Toolbar::layout_pin(sel, over, self.annot.tool, self.arrow_on));
    }

    fn img_px(&self) -> (u32, u32) {
        let w = ((self.img.width as f32) * self.scale).round().max(8.0) as u32;
        let h = ((self.img.height as f32) * self.scale).round().max(8.0) as u32;
        (w, h)
    }

    fn apply_size(&mut self) {
        let (w, h) = self.img_px();
        let (ww, hh) = if self.annotating {
            (w.max(420), h + 90)
        } else {
            (w, h)
        };
        let tl = self.window.outer_position().ok();
        let _ = self.window.request_inner_size(PhysicalSize::new(ww, hh));
        if let Some(p) = tl {
            self.window.set_outer_position(p);
        }
    }

    fn img_pt(&self, p: Point) -> Point {
        Point::new(p.x / self.scale.max(0.01), p.y / self.scale.max(0.01))
    }

    fn apply_tb(&mut self, a: TbAction) {
        match a {
            TbAction::Tool(t) => {
                self.arrow_on = t == Tool::Line && self.annot.style(Tool::Line).arrow;
                self.annot.set_tool(Some(t));
            }
            TbAction::Arrow => {
                self.annot.set_style_value(Tool::Line, |s| s.arrow = true);
                self.annot.set_tool(Some(Tool::Line));
                self.arrow_on = true;
            }
            TbAction::Undo => {
                let _ = self.annot.undo();
            }
            TbAction::Redo => {
                let _ = self.annot.redo();
            }
            TbAction::Close => self.close(),
            TbAction::Save => self.save(),
            TbAction::Copy => {
                let _ = crate::clipboard::write_image(&self.rendered());
            }
            TbAction::Check => self.show_toolbar(false),
            TbAction::Width(d) => {
                let _ = self.annot.adjust_width(d);
            }
            TbAction::Color(i) => {
                if let Some(tool) = self.annot.tool.filter(|t| *t != Tool::Select) {
                    let c = pinshot_core::palette_colors()[i as usize];
                    self.annot.set_style_value(tool, |s| s.color = c);
                }
            }
            TbAction::Ocr | TbAction::Translate | TbAction::Pin => {}
        }
        if self.annotating {
            self.relayout_tb();
        }
        self.dirty = true;
    }

    pub fn handle(&mut self, event: &WindowEvent) {
        if self.click_through {
            return;
        }
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let p = Point::new(position.x as f32, position.y as f32);
                self.last_cursor = p;
                if self.annotating
                    && (self.annot.is_brush_live() || self.annot.is_selecting() || self.annot.live.is_some())
                {
                    self.annot.move_to(self.img_pt(p), false);
                    self.dirty = true;
                } else if let Some(off) = self.drag {
                    if let Ok(pos) = self.window.outer_position() {
                        let nx = pos.x + (p.x - off.x) as i32;
                        let ny = pos.y + (p.y - off.y) as i32;
                        self.window
                            .set_outer_position(PhysicalPosition::new(nx, ny));
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button,
                ..
            } => {
                if *button == MouseButton::Right && *state == ElementState::Pressed {
                    self.popup_menu();
                    return;
                }
                if *button != MouseButton::Left {
                    return;
                }
                let p = self.last_cursor;
                if *state == ElementState::Pressed {
                    if self.annotating {
                        if let Some(tb) = &self.toolbar {
                            if let Some(a) = tb.hit(p) {
                                self.apply_tb(a);
                                return;
                            }
                        }
                        let ( _w, ih) = self.img_px();
                        if p.y <= ih as f32 && self.annot.press(self.img_pt(p), false) {
                            self.dirty = true;
                            return;
                        }
                    }
                    self.drag = Some(p);
                } else {
                    if self.annotating && (self.annot.live.is_some() || self.annot.is_selecting()) {
                        self.annot.release(self.img_pt(p));
                        self.dirty = true;
                    }
                    self.drag = None;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let angle = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (*y * 120.0) as i32,
                    MouseScrollDelta::PixelDelta(p) => p.y as i32,
                };
                if self.annotating
                    && (self.annot.tool.is_some()
                        || self.annot.selected.is_some()
                        || self.annot.option_target().is_some())
                {
                    if self.annot.wheel(angle) {
                        self.dirty = true;
                    }
                } else if angle > 0 {
                    self.set_scale(self.scale * 1.1);
                } else if angle < 0 {
                    self.set_scale(self.scale / 1.1);
                }
            }
            WindowEvent::Ime(winit::event::Ime::Commit(s)) => {
                for ch in s.chars() {
                    self.annot.type_text(ch);
                }
                self.dirty = true;
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if self.annot.text_edit.is_some() {
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => self.annot.commit_text(true),
                        Key::Named(NamedKey::Enter) => self.annot.commit_text(false),
                        Key::Named(NamedKey::Backspace) => self.annot.type_text('\u{8}'),
                        _ => {
                            if let Some(t) = event.text.as_deref() {
                                for ch in t.chars() {
                                    self.annot.type_text(ch);
                                }
                            }
                        }
                    }
                    self.dirty = true;
                    return;
                }
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        if self.annotating {
                            self.show_toolbar(false);
                        } else {
                            self.close();
                        }
                    }
                    Key::Named(NamedKey::Space) => self.show_toolbar(!self.annotating),
                    Key::Character(c) if c.eq_ignore_ascii_case("c") => {
                        let _ = crate::clipboard::write_image(&self.rendered());
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("s") => self.save(),
                    Key::Character(c) if c.eq_ignore_ascii_case("w") => self.close(),
                    Key::Character(c) if c.eq_ignore_ascii_case("z") => {
                        self.annot.undo();
                        self.dirty = true;
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("y") => {
                        self.annot.redo();
                        self.dirty = true;
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("r") => {
                        self.show_toolbar(true);
                        self.annot.set_tool(Some(Tool::Rect));
                        self.relayout_tb();
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("e") => {
                        self.show_toolbar(true);
                        self.annot.set_tool(Some(Tool::Ellipse));
                        self.relayout_tb();
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("p") => {
                        self.show_toolbar(true);
                        self.annot.set_tool(Some(Tool::Pen));
                        self.relayout_tb();
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("t") => {
                        self.show_toolbar(true);
                        self.annot.set_tool(Some(Tool::Text));
                        self.relayout_tb();
                    }
                    Key::Character(c) if c == "n" || c == "N" => {
                        self.show_toolbar(true);
                        self.annot.set_tool(Some(Tool::Number));
                        self.relayout_tb();
                    }
                    Key::Character(c) if c == "1" => {
                        if self.annotating && self.annot.tool.is_some() {
                            self.annot.adjust_width(-1);
                        } else {
                            self.rotate(-1);
                        }
                        self.dirty = true;
                    }
                    Key::Character(c) if c == "2" => {
                        if self.annotating && self.annot.tool.is_some() {
                            self.annot.adjust_width(1);
                        } else {
                            self.rotate(1);
                        }
                        self.dirty = true;
                    }
                    Key::Character(c) if c == "3" => {
                        self.flip(true);
                        self.dirty = true;
                    }
                    Key::Character(c) if c == "4" => {
                        self.flip(false);
                        self.dirty = true;
                    }
                    Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace)
                        if self.annot.tool == Some(Tool::Select) =>
                    {
                        self.annot.delete_selected();
                        self.dirty = true;
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::CloseRequested => self.close(),
            _ => {}
        }
    }

    pub fn handle_ctrl_wheel(&mut self, dy: f32) {
        self.set_opacity(self.opacity + if dy > 0.0 { 0.1 } else { -0.1 });
    }

    pub fn rotate(&mut self, turns: i32) {
        self.img = self.rendered().rotate90(turns);
        self.annot.clear();
        self.apply_size();
        self.dirty = true;
    }

    pub fn flip(&mut self, horizontal: bool) {
        self.img = self.rendered().flip(horizontal);
        self.annot.clear();
        self.apply_size();
        self.dirty = true;
    }

    pub fn paste(&mut self) {
        if let crate::clipboard::ClipContent::Image(img) = crate::clipboard::read_best() {
            self.img = img;
            self.annot.clear();
            self.apply_size();
            self.dirty = true;
        }
    }

    fn popup_menu(&mut self) {
        #[cfg(windows)]
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return;
            };
            append(menu, ID_COPY, &tr("Copy image"), false);
            append(menu, ID_SAVE, &tr("Save image as…"), false);
            append(menu, ID_TOOLBAR, &tr("Show toolbar"), self.annotating);
            append(menu, 0, "", true);
            append(menu, ID_CT, &tr("Click-through"), self.click_through);
            append(menu, 0, "", true);
            append(menu, ID_Z25, "25%", (self.scale - 0.25).abs() < 0.02);
            append(menu, ID_Z50, "50%", (self.scale - 0.5).abs() < 0.02);
            append(menu, ID_Z100, "100%", (self.scale - 1.0).abs() < 0.02);
            append(menu, ID_Z200, "200%", (self.scale - 2.0).abs() < 0.02);
            append(menu, ID_Z400, "400%", (self.scale - 4.0).abs() < 0.02);
            append(menu, 0, "", true);
            append(menu, ID_CLOSE, &tr("Close"), false);
            let pos = self.window.outer_position().unwrap_or_default();
            let cx = pos.x + self.last_cursor.x as i32;
            let cy = pos.y + self.last_cursor.y as i32;
            let cmd = TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_RETURNCMD,
                cx,
                cy,
                0,
                self.hwnd(),
                None,
            );
            let _ = DestroyMenu(menu);
            match cmd.0 as usize {
                ID_COPY => {
                    let _ = crate::clipboard::write_image(&self.rendered());
                }
                ID_SAVE => self.save(),
                ID_CLOSE => self.close(),
                ID_TOOLBAR => self.show_toolbar(!self.annotating),
                ID_CT => self.set_click_through(!self.click_through),
                ID_Z25 => self.set_scale(0.25),
                ID_Z50 => self.set_scale(0.5),
                ID_Z75 => self.set_scale(0.75),
                ID_Z100 => self.set_scale(1.0),
                ID_Z150 => self.set_scale(1.5),
                ID_Z200 => self.set_scale(2.0),
                ID_Z400 => self.set_scale(4.0),
                _ => {}
            }
        }
        #[cfg(not(windows))]
        {
            self.run_pin_menu();
        }
    }

    #[cfg(not(windows))]
    fn run_pin_menu(&mut self) {
        // Fallback: apply the most common items via a zenity list when available.
        if let Ok(out) = std::process::Command::new("zenity")
            .args([
                "--list",
                "--title=PinShot",
                "--column=Action",
                &tr("Copy image"),
                &tr("Save image as…"),
                &tr("Show toolbar"),
                &tr("Click-through"),
                "25%",
                "50%",
                "100%",
                "200%",
                "400%",
                &tr("Rotate left"),
                &tr("Rotate right"),
                &tr("Flip horizontal"),
                &tr("Flip vertical"),
                &tr("Paste"),
                &tr("Close"),
            ])
            .output()
        {
            if !out.status.success() {
                return;
            }
            let s = String::from_utf8_lossy(&out.stdout);
            let s = s.trim();
            if s == tr("Copy image") {
                let _ = crate::clipboard::write_image(&self.rendered());
            } else if s == tr("Save image as…") {
                self.save();
            } else if s == tr("Show toolbar") {
                self.show_toolbar(!self.annotating);
            } else if s == tr("Click-through") {
                self.set_click_through(!self.click_through);
            } else if s == "25%" {
                self.set_scale(0.25);
            } else if s == "50%" {
                self.set_scale(0.5);
            } else if s == "100%" {
                self.set_scale(1.0);
            } else if s == "200%" {
                self.set_scale(2.0);
            } else if s == "400%" {
                self.set_scale(4.0);
            } else if s == tr("Rotate left") {
                self.rotate(-1);
            } else if s == tr("Rotate right") {
                self.rotate(1);
            } else if s == tr("Flip horizontal") {
                self.flip(true);
            } else if s == tr("Flip vertical") {
                self.flip(false);
            } else if s == tr("Paste") {
                self.paste();
            } else if s == tr("Close") {
                self.close();
            }
        } else {
            self.show_toolbar(!self.annotating);
        }
    }

    fn save(&mut self) {
        if let Some(path) = crate::dialog::save_path("pinshot.png") {
            let _ = crate::dialog::save_raster(&self.rendered(), &path, 92);
        }
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.window.set_visible(false);
    }

    /// Python `mouseDoubleClickEvent`: first hit shows a tip, then close/hide/none.
    pub fn on_double_click(&mut self, cfg: &mut pinshot_core::Config) {
        if self.click_through {
            return;
        }
        if self.annot.tool == Some(Tool::Select)
            || (self.annot.tool.is_none() && self.annot.selected.is_some())
        {
            if let Some(i) = self.annot.hit_test(self.img_pt(self.last_cursor)) {
                if self.annot.shapes.get(i).map(|s| s.kind) == Some(Tool::Text) {
                    self.show_toolbar(true);
                    self.annot.set_tool(Some(Tool::Select));
                    self.annot.begin_edit_text(i);
                    self.dirty = true;
                    return;
                }
            }
        }
        if self.annot.tool.is_some() {
            return;
        }
        match cfg.dblclick_pin_action.as_str() {
            "none" => return,
            _ => {}
        }
        if !cfg.dblclick_tip_shown {
            cfg.dblclick_tip_shown = true;
            let _ = pinshot_core::config::save(cfg);
            let tip = if cfg.dblclick_pin_action == "hide" {
                tr("Tip: double-click hides a pin.")
            } else {
                tr("Tip: double-click closes a pin.")
            };
            self.toast = Some(format!("{} {}", tip, tr("Double-click again to confirm.")));
            self.dirty = true;
            return;
        }
        if cfg.dblclick_pin_action == "hide" {
            self.hide();
        } else {
            self.close();
        }
    }

    pub fn redraw(&mut self) {
        let (w, h) = self.img_px();
        let (ww, hh) = if self.annotating {
            (w.max(420), h + 90)
        } else {
            (w, h)
        };
        if self
            .surface
            .resize(NonZeroU32::new(ww).unwrap(), NonZeroU32::new(hh).unwrap())
            .is_err()
        {
            return;
        }
        let img_pm = paint_pin(&self.img, &self.annot, self.scale, self.click_through, self.glow);
        let mut pm = tiny_skia::Pixmap::new(ww, hh).unwrap_or_else(|| tiny_skia::Pixmap::new(8, 8).unwrap());
        pm.data_mut().fill(0xF7);
        let img_w = img_pm.width().min(ww);
        let img_h = img_pm.height().min(hh);
        {
            let src = img_pm.data();
            let dst = pm.data_mut();
            let sw = img_pm.width() as usize;
            for y in 0..img_h as usize {
                let so = y * sw * 4;
                let doff = y * ww as usize * 4;
                let n = (img_w as usize) * 4;
                if so + n <= src.len() && doff + n <= dst.len() {
                    dst[doff..doff + n].copy_from_slice(&src[so..so + n]);
                }
            }
        }
        if self.annotating {
            if self.toolbar.is_none() {
                self.relayout_tb();
            }
            if let Some(tb) = &self.toolbar {
                paint_toolbar(&mut pm, tb);
            }
            if let Some(ed) = &self.annot.text_edit {
                if let Some(r) = self.annot.text_edit_rect() {
                    let rr = Rect::new(r.x * self.scale, r.y * self.scale, r.w * self.scale, r.h * self.scale);
                    crate::draw::fill_rect(&mut pm, rr, pinshot_core::color::Rgba::rgba(255, 255, 255, 90));
                    crate::draw::stroke_rect(&mut pm, rr, self.annot.style(Tool::Text).color, 1.0);
                    crate::text::blit_into_pm(
                        &mut pm,
                        &ed.text,
                        rr.x + 2.0,
                        rr.y + 2.0,
                        ((self.annot.style(Tool::Text).font_size as f32) * self.scale) as i32,
                        self.annot.style(Tool::Text).color,
                        true,
                    );
                }
            }
        }
        if let Some(t) = &self.toast {
            crate::draw::paint_toast(&mut pm, t, ww, hh);
        }
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

#[cfg(windows)]
unsafe fn append(menu: HMENU, id: usize, text: &str, checked: bool) {
    if id == 0 {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());
        return;
    }
    let w = wide(text);
    let mut flags = MF_STRING;
    if checked {
        flags |= MF_CHECKED;
    }
    let _ = AppendMenuW(menu, flags, id, windows::core::PCWSTR(w.as_ptr()));
}

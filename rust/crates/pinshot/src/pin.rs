//! Frameless always-on-top pin windows.

use std::num::NonZeroU32;
use std::rc::Rc;

use pinshot_core::annotation::Annotator;
use pinshot_core::geom::Point;
use pinshot_core::i18n::tr;
use pinshot_core::raster::Raster;
use pinshot_core::render::compose;
use softbuffer::{Context, Surface};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, TrackPopupMenu, HMENU, MF_CHECKED, MF_SEPARATOR,
    MF_STRING, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
};

use crate::draw::{paint_pin, present};
use crate::win32::{apply_pin_style, hwnd_of, set_click_through, set_opacity, wide};

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
        apply_pin_style(hwnd_of(&window));
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
        };
        pin.redraw();
        Ok(pin)
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn hwnd(&self) -> HWND {
        hwnd_of(&self.window)
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
        set_opacity(self.hwnd(), self.opacity);
    }

    pub fn set_click_through(&mut self, on: bool) {
        self.click_through = on;
        set_click_through(self.hwnd(), on, self.opacity);
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

    pub fn handle(&mut self, event: &WindowEvent) {
        if self.click_through {
            return;
        }
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let p = Point::new(position.x as f32, position.y as f32);
                self.last_cursor = p;
                if let Some(off) = self.drag {
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
                if *button == MouseButton::Left {
                    self.drag = if *state == ElementState::Pressed {
                        Some(self.last_cursor)
                    } else {
                        None
                    };
                } else if *button == MouseButton::Right && *state == ElementState::Pressed {
                    self.popup_menu();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 120.0,
                };
                // Ctrl is handled in app via modifiers snapshot if needed; default = zoom.
                if dy > 0.0 {
                    self.set_scale(self.scale * 1.1);
                } else {
                    self.set_scale(self.scale / 1.1);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => self.close(),
                    Key::Character(c) if c.eq_ignore_ascii_case("c") => {
                        let _ = crate::clipboard::write_image(&self.rendered());
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("s") => self.save(),
                    Key::Character(c) if c.eq_ignore_ascii_case("w") => self.close(),
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

    fn popup_menu(&mut self) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return;
            };
            append(menu, ID_COPY, &tr("Copy image"), false);
            append(menu, ID_SAVE, &tr("Save image as…"), false);
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

    pub fn redraw(&mut self) {
        let w = ((self.img.width as f32) * self.scale).round().max(8.0) as u32;
        let h = ((self.img.height as f32) * self.scale).round().max(8.0) as u32;
        if self
            .surface
            .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
            .is_err()
        {
            return;
        }
        let pm = paint_pin(&self.img, &self.annot, self.scale, self.click_through, self.glow);
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

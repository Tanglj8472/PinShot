//! Tiny control window used when the desktop has no system tray (Python `_build_fallback`).

use std::num::NonZeroU32;
use std::rc::Rc;

use pinshot_core::geom::{Point, Rect};
use pinshot_core::i18n::tr;
use pinshot_core::icons::app_icon_rgba;
use softbuffer::{Context, Surface};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::draw::{fill_rect, present, stroke_rect, UiTheme};
use crate::event::TrayCmd;

struct Btn {
    rect: Rect,
    label: String,
    cmd: TrayCmd,
}

pub struct FallbackWin {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    buttons: Vec<Btn>,
    cursor: Point,
    dirty: bool,
    theme: UiTheme,
}

impl FallbackWin {
    pub fn create(event_loop: &ActiveEventLoop) -> Result<Self, String> {
        let attrs = Window::default_attributes()
            .with_title("PinShot")
            .with_inner_size(LogicalSize::new(240.0, 200.0))
            .with_min_inner_size(LogicalSize::new(220.0, 180.0))
            .with_resizable(true);
        let window = Rc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        if let Some((_, _, rgba)) = Some(app_icon_rgba(32)) {
            let _ = rgba;
        }
        let context = Context::new(window.clone()).map_err(|e| format!("{e:?}"))?;
        let surface = Surface::new(&context, window.clone()).map_err(|e| format!("{e:?}"))?;
        let mut w = Self {
            window,
            surface,
            buttons: Vec::new(),
            cursor: Point::ZERO,
            dirty: true,
            theme: UiTheme::resolve("system"),
        };
        w.rebuild_buttons();
        w.redraw();
        Ok(w)
    }

    fn rebuild_buttons(&mut self) {
        let labels = [
            (tr("Snip"), TrayCmd::Snip),
            (tr("Pin clipboard"), TrayCmd::Pin),
            (tr("Preferences…"), TrayCmd::Settings),
            (tr("Quit"), TrayCmd::Quit),
        ];
        self.buttons.clear();
        for (i, (label, cmd)) in labels.into_iter().enumerate() {
            self.buttons.push(Btn {
                rect: Rect::new(16.0, 16.0 + i as f32 * 42.0, 208.0, 36.0),
                label,
                cmd,
            });
        }
    }

    pub fn retranslate(&mut self) {
        self.rebuild_buttons();
        self.dirty = true;
        self.window.request_redraw();
    }

    pub fn apply_theme(&mut self, name: &str) {
        self.theme = UiTheme::resolve(name);
        self.dirty = true;
        self.window.request_redraw();
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn show(&self) {
        self.window.set_visible(true);
        self.window.focus_window();
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
        crate::native::unmap_sync(&self.window);
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible().unwrap_or(true)
    }

    pub fn handle(&mut self, event: &WindowEvent) -> Option<TrayCmd> {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Point::new(position.x as f32, position.y as f32);
                self.dirty = true;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                for b in &self.buttons {
                    if b.rect.contains(self.cursor) {
                        return Some(b.cmd.clone());
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match &event.logical_key {
                    Key::Named(NamedKey::F1) => return Some(TrayCmd::Snip),
                    Key::Named(NamedKey::F3) => return Some(TrayCmd::Pin),
                    Key::Character(c) if c.eq_ignore_ascii_case("p") => {
                        return Some(TrayCmd::Settings)
                    }
                    // Python fallback has no Esc=Quit. Esc here used to kill the
                    // process after a pin menu closed (focus fell on fallback).
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::CloseRequested => return Some(TrayCmd::Quit),
            _ => {}
        }
        if self.dirty {
            self.window.request_redraw();
        }
        None
    }

    fn redraw(&mut self) {
        let size = self.window.inner_size();
        let w = size.width.max(8);
        let h = size.height.max(8);
        if self
            .surface
            .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
            .is_err()
        {
            return;
        }
        let Some(mut pm) = tiny_skia::Pixmap::new(w, h) else {
            return;
        };
        fill_rect(
            &mut pm,
            Rect::new(0.0, 0.0, w as f32, h as f32),
            self.theme.bg,
        );
        for b in &self.buttons {
            let hover = b.rect.contains(self.cursor);
            fill_rect(
                &mut pm,
                b.rect,
                if hover {
                    self.theme.hover
                } else {
                    self.theme.panel
                },
            );
            stroke_rect(&mut pm, b.rect, self.theme.border, 1.0);
            crate::text::blit_into_pm(
                &mut pm,
                &b.label,
                b.rect.x + 12.0,
                b.rect.y + 8.0,
                14,
                self.theme.text,
                true,
            );
        }
        if let Ok(mut buf) = self.surface.buffer_mut() {
            present(&mut buf, &pm);
            let _ = buf.present();
        }
        self.dirty = false;
    }
}

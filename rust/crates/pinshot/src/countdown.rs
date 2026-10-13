//! On-screen snip-delay numeral (Python `_countdown_snip` QLabel).

use std::num::NonZeroU32;
use std::rc::Rc;

use pinshot_core::color::Rgba;
use pinshot_core::geom::Rect;
use softbuffer::{Context, Surface};
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId, WindowLevel};

use crate::draw::{fill_rect, present};
use crate::native;

pub struct CountdownWin {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    left: u32,
    dirty: bool,
}

impl CountdownWin {
    pub fn create(event_loop: &ActiveEventLoop, left: u32) -> Result<Self, String> {
        let attrs = native::as_tool_window(
            Window::default_attributes()
                .with_title("PinShot")
                .with_decorations(false)
                .with_resizable(false)
                .with_inner_size(LogicalSize::new(140.0, 140.0))
                .with_window_level(WindowLevel::AlwaysOnTop),
        );
        let window = Rc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        native::apply_pin_style(&window);
        native::exclude_from_capture(&window);
        native::raise_on_top(&window);
        let context = Context::new(window.clone()).map_err(|e| format!("{e:?}"))?;
        let surface = Surface::new(&context, window.clone()).map_err(|e| format!("{e:?}"))?;
        let mut w = Self {
            window,
            surface,
            left,
            dirty: true,
        };
        w.redraw();
        Ok(w)
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn set_left(&mut self, left: u32) {
        self.left = left;
        self.dirty = true;
        self.window.request_redraw();
    }

    pub fn handle(&mut self, event: &WindowEvent) {
        if matches!(event, WindowEvent::RedrawRequested) {
            self.redraw();
        } else if self.dirty {
            self.window.request_redraw();
        }
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
            Rgba::rgba(20, 20, 20, 200),
        );
        let label = self.left.to_string();
        let tw = crate::text::measure(&label, 48, true).0 as f32;
        let x = ((w as f32) - tw) * 0.5;
        let y = (h as f32) * 0.5 - 28.0;
        crate::text::blit_into_pm(&mut pm, &label, x, y, 48, Rgba::WHITE, true);
        if let Ok(mut buf) = self.surface.buffer_mut() {
            present(&mut buf, &pm);
            let _ = buf.present();
        }
        self.window.set_visible(true);
        self.dirty = false;
    }
}

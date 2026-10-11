//! Application shell: tray, hotkeys, overlay, pins.

use std::time::Instant;

use pinshot_core::config::{self, Config};
use pinshot_core::hooks::{SettingsUi, SettingsUiStub};
use pinshot_core::hotkey::Command;
use pinshot_core::i18n::{set_lang, tr};
use pinshot_core::raster::Raster;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::application::ApplicationHandler;
use winit::window::WindowId;

use crate::capture::Capture;
use crate::clipboard::{self, ClipContent};
use crate::event::{AppEvent, TrayCmd};
use crate::overlay::{Overlay, OverlayResult};
use crate::pin::PinWin;
use crate::tray::Tray;
use crate::win32::{self, HotkeyHost};

pub fn run(args: Vec<String>) -> Result<(), String> {
    win32::init_dpi();
    let cfg = config::load().unwrap_or_default();
    set_lang(cfg.lang());
    let start_snip = args.iter().any(|a| a == "--snip" || a == "snip");
    let start_pin = args.iter().any(|a| a == "--pin" || a == "pin");

    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    let mut app = App {
        cfg,
        proxy,
        hotkeys: None,
        tray: None,
        overlay: None,
        pins: Vec::new(),
        closed: Vec::new(),
        settings: SettingsUiStub::default(),
        start_snip,
        start_pin,
        mods: ModifiersState::empty(),
        last_click: None,
        pins_hidden: false,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

struct App {
    cfg: Config,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    hotkeys: Option<HotkeyHost>,
    tray: Option<Tray>,
    overlay: Option<Overlay>,
    pins: Vec<PinWin>,
    closed: Vec<(Raster, (i32, i32))>,
    settings: SettingsUiStub,
    start_snip: bool,
    start_pin: bool,
    mods: ModifiersState,
    last_click: Option<(WindowId, Instant)>,
    pins_hidden: bool,
}

impl App {
    fn begin_snip(&mut self, event_loop: &ActiveEventLoop) {
        if self.overlay.is_some() {
            return;
        }
        match Capture::grab() {
            Ok(cap) => match Overlay::start(event_loop, cap) {
                Ok(ov) => self.overlay = Some(ov),
                Err(e) => eprintln!("PinShot overlay: {e}"),
            },
            Err(e) => eprintln!("PinShot capture: {e}"),
        }
    }

    fn pin_clipboard(&mut self, event_loop: &ActiveEventLoop) {
        match clipboard::read_best() {
            ClipContent::Image(img) => self.spawn_pin(event_loop, img, None),
            ClipContent::Color(c) => {
                self.spawn_pin(event_loop, clipboard::color_card(c, 220, 140), None)
            }
            ClipContent::Text(t) => {
                self.spawn_pin(event_loop, clipboard::text_card(&t, 280, 120), None)
            }
            ClipContent::Empty => eprintln!("{}", tr("Clipboard has no image or text")),
        }
    }

    fn spawn_pin(&mut self, event_loop: &ActiveEventLoop, img: Raster, pos: Option<(i32, i32)>) {
        let n = self.pins.len() as i32;
        let pos = pos.unwrap_or((40 + n * 24, 40 + n * 24));
        match PinWin::create(event_loop, img, pos) {
            Ok(p) => self.pins.push(p),
            Err(e) => eprintln!("PinShot pin: {e}"),
        }
    }

    fn finish_overlay(&mut self, event_loop: &ActiveEventLoop) {
        let Some(ov) = self.overlay.as_mut() else {
            return;
        };
        let Some(res) = ov.take_result() else {
            return;
        };
        self.overlay = None;
        match res {
            OverlayResult::Copy(img, _) => {
                if clipboard::write_image(&img).is_ok() {
                    eprintln!("{}", tr("Copied to clipboard"));
                }
            }
            OverlayResult::Pin(img, _, pos) => self.spawn_pin(event_loop, img, Some(pos)),
            OverlayResult::Save(img) => {
                if let Some(path) = crate::dialog::save_path("pinshot.png") {
                    if crate::dialog::save_raster(&img, &path, self.cfg.image_quality).is_ok() {
                        eprintln!("{}", tr(&format!("Saved {path}")));
                    }
                }
            }
            OverlayResult::Cancel => {}
        }
    }

    fn toggle_pins(&mut self) {
        self.pins_hidden = !self.pins_hidden;
        for p in &self.pins {
            if self.pins_hidden {
                p.hide();
            } else {
                p.show();
            }
        }
    }

    fn restore_closed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some((img, pos)) = self.closed.pop() {
            self.spawn_pin(event_loop, img, Some(pos));
        }
    }

    fn reap_pins(&mut self) {
        let mut i = 0;
        while i < self.pins.len() {
            if self.pins[i].closed {
                let p = self.pins.remove(i);
                if self.closed.len() >= self.cfg.closed_pins_max as usize {
                    self.closed.remove(0);
                }
                self.closed.push((p.img.clone(), (40, 40)));
            } else {
                i += 1;
            }
        }
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.hotkeys.is_none() {
            match HotkeyHost::install(self.proxy.clone(), &self.cfg) {
                Ok(h) => self.hotkeys = Some(h),
                Err(e) => eprintln!("PinShot hotkeys: {e}"),
            }
        }
        if self.tray.is_none() {
            match Tray::new() {
                Ok(t) => self.tray = Some(t),
                Err(e) => eprintln!("PinShot tray: {e}"),
            }
        }
        if self.start_snip {
            self.start_snip = false;
            self.begin_snip(event_loop);
        }
        if self.start_pin {
            self.start_pin = false;
            self.pin_clipboard(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Hotkey(Command::Snip) => self.begin_snip(event_loop),
            AppEvent::Hotkey(Command::PinClipboard) => {
                if self.overlay.is_some() {
                    // F3 during snip is handled by the overlay window.
                } else {
                    self.pin_clipboard(event_loop);
                }
            }
            AppEvent::Hotkey(Command::HideAllPins) => self.toggle_pins(),
            AppEvent::Hotkey(Command::RestoreClosed) => self.restore_closed(event_loop),
            AppEvent::Hotkey(Command::ClickThrough) => {
                if let Some(p) = self.pins.last_mut() {
                    let on = !p.click_through;
                    p.set_click_through(on);
                }
            }
            AppEvent::Hotkey(Command::Quit) => event_loop.exit(),
            AppEvent::Hotkey(Command::OpenSettings) => self.settings.open(),
            AppEvent::Tray(TrayCmd::Snip) => self.begin_snip(event_loop),
            AppEvent::Tray(TrayCmd::Pin) => self.pin_clipboard(event_loop),
            AppEvent::Tray(TrayCmd::HideAll) => self.toggle_pins(),
            AppEvent::Tray(TrayCmd::Settings) => self.settings.open(),
            AppEvent::Tray(TrayCmd::Quit) => event_loop.exit(),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let WindowEvent::ModifiersChanged(m) = &event {
            self.mods = m.state();
        }

        if let Some(ov) = self.overlay.as_mut() {
            if ov.window_id() == id {
                if let WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } = &event
                {
                    let now = Instant::now();
                    if let Some((wid, t)) = self.last_click {
                        if wid == id && now.duration_since(t).as_millis() < 350 {
                            ov.on_double_click();
                            self.last_click = None;
                        } else {
                            self.last_click = Some((id, now));
                        }
                    } else {
                        self.last_click = Some((id, now));
                    }
                }
                ov.handle(&event);
                self.finish_overlay(event_loop);
                return;
            }
        }
        for p in &mut self.pins {
            if p.window_id() == id {
                if let WindowEvent::MouseWheel { delta, .. } = &event {
                    if self.mods.control_key() {
                        let dy = match delta {
                            MouseScrollDelta::LineDelta(_, y) => *y,
                            MouseScrollDelta::PixelDelta(pt) => pt.y as f32 / 120.0,
                        };
                        p.handle_ctrl_wheel(dy);
                        return;
                    }
                }
                p.handle(&event);
                return;
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        if let Some(tray) = &self.tray {
            if let Some(cmd) = tray.poll() {
                let _ = self.proxy.send_event(AppEvent::Tray(cmd));
            }
        }
        if let Some(ov) = &mut self.overlay {
            ov.pump_redraw();
        }
        for p in &mut self.pins {
            p.tick_glow();
            p.pump_redraw();
        }
        self.reap_pins();
        let _ = event_loop;
    }
}

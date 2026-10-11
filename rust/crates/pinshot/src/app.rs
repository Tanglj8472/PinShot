//! Application shell: tray, hotkeys, overlay, pins, prefs, session, OCR.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use pinshot_core::config::{self, data_dir, Config};
use pinshot_core::history::SnipHistory;
use pinshot_core::hotkey::Command;
use pinshot_core::i18n::{set_lang, tr};
use pinshot_core::raster::Raster;
use pinshot_core::session::{
    next_pin_filename, pin_png_path, ClosedPin, ClosedStack, PinSnapshot, SessionFile,
};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::application::ApplicationHandler;
use winit::window::WindowId;

use crate::autostart;
use crate::capture::Capture;
use crate::clipboard::{self, ClipContent};
use crate::event::{AppEvent, TrayCmd};
use crate::countdown::CountdownWin;
use crate::fallback::FallbackWin;
use crate::native::{self, HotkeyHost};
use crate::overlay::{Overlay, OverlayResult};
use crate::pin::PinWin;
use crate::prefs;
use crate::tray::Tray;

pub fn run(args: Vec<String>) -> Result<(), String> {
    crate::logutil::init(&args);
    native::init_process();
    let cfg = config::load().unwrap_or_default();
    set_lang(cfg.lang());

    let cmd = crate::ipc::parse_cli_cmd(&args);
    match crate::ipc::claim_or_forward(cmd)? {
        crate::ipc::Boot::Forwarded => return Ok(()),
        crate::ipc::Boot::Primary(mtx) => {
            std::mem::forget(mtx);
        }
    }

    let start_snip = args.iter().any(|a| a == "--snip" || a == "snip");
    let start_pin = args.iter().any(|a| a == "--pin" || a == "--pin-clipboard" || a == "pin");
    let start_prefs = args.iter().any(|a| a == "--prefs" || a == "prefs");
    let start_translate = args.iter().any(|a| a == "--translate" || a == "translate");
    let dump_capture = args.iter().any(|a| a == "--dump-capture");

    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    crate::ipc::spawn_server(proxy.clone());

    let mut group_vis = HashMap::new();
    for g in &cfg.groups {
        group_vis.insert(g.clone(), true);
    }

    let mut app = App {
        cfg,
        proxy,
        hotkeys: None,
        tray: None,
        fallback: None,
        overlay: None,
        pins: Vec::new(),
        closed: ClosedStack::default(),
        history: SnipHistory::new(),
        group_vis,
        start_snip,
        start_pin,
        start_prefs,
        start_translate,
        dump_capture,
        mods: ModifiersState::empty(),
        last_click: None,
        pins_hidden: false,
        snip_at: None,
        translate_next: false,
        session_restored: false,
        countdown: None,
        countdown_left: 0,
        restore_fallback: false,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

struct App {
    cfg: Config,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    hotkeys: Option<HotkeyHost>,
    tray: Option<Tray>,
    fallback: Option<FallbackWin>,
    overlay: Option<Overlay>,
    pins: Vec<PinWin>,
    closed: ClosedStack,
    history: SnipHistory,
    group_vis: HashMap<String, bool>,
    start_snip: bool,
    start_pin: bool,
    start_prefs: bool,
    start_translate: bool,
    dump_capture: bool,
    mods: ModifiersState,
    last_click: Option<(WindowId, Instant)>,
    pins_hidden: bool,
    snip_at: Option<Instant>,
    translate_next: bool,
    session_restored: bool,
    countdown: Option<CountdownWin>,
    countdown_left: u32,
    restore_fallback: bool,
}

impl App {
    fn group_list(&self) -> Vec<(String, bool)> {
        self.cfg
            .groups
            .iter()
            .map(|g| (g.clone(), *self.group_vis.get(g).unwrap_or(&true)))
            .collect()
    }

    fn rebuild_tray(&mut self) {
        // tray-icon/GTK can panic when the desktop has no indicator (Xfce without a
        // notification plugin). Never take the process down — fallback window exists.
        let groups = self.group_list();
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Tray::new(&groups))) {
            Ok(Ok(t)) => self.tray = Some(t),
            Ok(Err(e)) => {
                crate::logutil::log(&format!("tray: {e}"));
                self.tray = None;
            }
            Err(_) => {
                crate::logutil::log("tray: panic (GTK/indicator missing); using fallback window");
                self.tray = None;
            }
        }
    }

    fn ensure_fallback(&mut self, event_loop: &ActiveEventLoop) {
        let need = {
            #[cfg(unix)]
            {
                let _ = event_loop;
                true
            }
            #[cfg(not(unix))]
            {
                self.tray.is_none()
            }
        };
        if need && self.fallback.is_none() {
            match FallbackWin::create(event_loop) {
                Ok(f) => self.fallback = Some(f),
                Err(e) => crate::logutil::log(&format!("fallback window: {e}")),
            }
        }
    }

    fn apply_cfg(&mut self, cfg: Config) {
        set_lang(cfg.lang());
        if cfg.run_at_login != autostart::is_enabled() {
            if let Err(e) = autostart::set_enabled(cfg.run_at_login) {
                eprintln!("PinShot autostart: {e}");
            }
        }
        self.cfg = cfg;
        self.hotkeys = None;
        self.rebuild_tray();
    }

    fn request_snip(&mut self, event_loop: &ActiveEventLoop, translate: bool) {
        if self.overlay.is_some() || self.snip_at.is_some() || self.countdown.is_some() {
            return;
        }
        self.translate_next = translate;
        let delay = self.cfg.snip_delay;
        if delay > 0 {
            self.countdown_left = delay;
            match CountdownWin::create(event_loop, delay) {
                Ok(c) => self.countdown = Some(c),
                Err(e) => crate::logutil::log(&format!("countdown: {e}")),
            }
            self.snip_at = Some(Instant::now() + Duration::from_secs(1));
        } else {
            self.prepare_capture_hide();
            self.snip_at = Some(Instant::now() + Duration::from_millis(self.hide_wait_ms()));
        }
    }

    fn hide_wait_ms(&self) -> u64 {
        if !self.restore_fallback {
            0
        } else if cfg!(unix) {
            300
        } else {
            150
        }
    }

    fn prepare_capture_hide(&mut self) {
        if let Some(fb) = &self.fallback {
            if fb.is_visible() {
                fb.hide();
                self.restore_fallback = true;
            }
        }
        self.countdown = None;
    }

    fn restore_fallback_if_needed(&mut self) {
        if self.restore_fallback {
            if let Some(fb) = &self.fallback {
                fb.show();
            }
            self.restore_fallback = false;
        }
    }

    fn begin_snip(&mut self, event_loop: &ActiveEventLoop) {
        if self.overlay.is_some() {
            return;
        }
        let translate = self.translate_next;
        self.translate_next = false;
        match Capture::grab(self.dump_capture) {
            Ok(cap) => match Overlay::start(event_loop, cap, self.history.clone(), translate) {
                Ok(ov) => self.overlay = Some(ov),
                Err(e) => eprintln!("PinShot overlay: {e}"),
            },
            Err(e) => eprintln!("PinShot capture: {e}"),
        }
    }

    fn pin_clipboard(&mut self, event_loop: &ActiveEventLoop) {
        match clipboard::read_best() {
            ClipContent::Image(img) => self.spawn_pin(event_loop, img, None, None),
            ClipContent::Color(c) => {
                self.spawn_pin(event_loop, clipboard::color_card(c, 220, 140), None, None)
            }
            ClipContent::Text(t) => {
                self.spawn_pin(event_loop, clipboard::text_card(&t, 280, 120), None, None)
            }
            ClipContent::Empty => eprintln!("{}", tr("Clipboard has no image or text")),
        }
    }

    fn spawn_pin(
        &mut self,
        event_loop: &ActiveEventLoop,
        img: Raster,
        pos: Option<(i32, i32)>,
        group: Option<String>,
    ) {
        let n = self.pins.len() as i32;
        let pos = pos.unwrap_or((40 + n * 24, 40 + n * 24));
        match PinWin::create(event_loop, img, pos) {
            Ok(mut p) => {
                if let Some(g) = group {
                    p.group = g;
                }
                let vis = *self.group_vis.get(&p.group).unwrap_or(&true);
                if !vis || self.pins_hidden {
                    p.hide();
                }
                self.pins.push(p);
                self.persist_session();
            }
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
        self.history = ov.history.clone();
        self.history.entries.truncate(self.cfg.history_max as usize);
        self.overlay = None;
        match res {
            OverlayResult::Copy(img, _) => {
                if clipboard::write_image(&img).is_ok() {
                    eprintln!("{}", tr("Copied to clipboard"));
                }
            }
            OverlayResult::Pin(img, _, pos) => self.spawn_pin(event_loop, img, Some(pos), None),
            OverlayResult::Save(img) => {
                let folder = self.cfg.quick_save_folder.trim();
                let suggested = if folder.is_empty() {
                    "pinshot.png".into()
                } else {
                    {
                        let sep = if cfg!(windows) { "\\" } else { "/" };
                        format!("{folder}{sep}pinshot.png")
                    }
                };
                if let Some(path) = crate::dialog::save_path(&suggested) {
                    if crate::dialog::save_raster(&img, &path, self.cfg.image_quality).is_ok() {
                        eprintln!("{}", tr(&format!("Saved {path}")));
                    }
                }
            }
            OverlayResult::Ocr(img, translate) => self.spawn_ocr(img, translate),
            OverlayResult::Cancel => {}
        }
        self.restore_fallback_if_needed();
    }

    fn spawn_ocr(&self, img: Raster, translate: bool) {
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let text = crate::ocr_run::recognize(&img).unwrap_or_else(|e| e);
            let _ = proxy.send_event(AppEvent::OcrDone(text, translate));
        });
    }

    fn toggle_pins(&mut self) {
        self.pins_hidden = !self.pins_hidden;
        for p in &self.pins {
            if self.pins_hidden {
                p.hide();
            } else if *self.group_vis.get(&p.group).unwrap_or(&true) {
                p.show();
            }
        }
    }

    fn toggle_group(&mut self, name: String) {
        let vis = !*self.group_vis.get(&name).unwrap_or(&true);
        self.group_vis.insert(name.clone(), vis);
        for p in &self.pins {
            if p.group.eq_ignore_ascii_case(&name) {
                if vis && !self.pins_hidden {
                    p.show();
                } else {
                    p.hide();
                }
            }
        }
        self.rebuild_tray();
    }

    fn restore_closed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(c) = self.closed.pop() {
            if let Some(img) = decode_png(&c.png) {
                self.spawn_pin(event_loop, img, Some(c.pos), Some(c.group));
            }
        }
    }

    fn selection_translate(&self) {
        #[cfg(not(windows))]
        {
            let msg = tr("Select-text translate is only available here.");
            crate::result_win::open(msg, false);
            return;
        }
        #[cfg(windows)]
        {
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let backup = clipboard::read_best();
            if crate::result_win::send_ctrl_c().is_err() {
                let _ = proxy.send_event(AppEvent::OcrDone(String::new(), true));
                return;
            }
            std::thread::sleep(Duration::from_millis(80));
            let text = match clipboard::read_best() {
                ClipContent::Text(t) => t,
                _ => String::new(),
            };
            // restore image if we stole it
            if let ClipContent::Image(img) = backup {
                let _ = clipboard::write_image(&img);
            }
            let _ = proxy.send_event(AppEvent::OcrDone(text, true));
        });
        }
    }

    fn persist_session(&self) {
        let dir = data_dir();
        let mut snaps = Vec::new();
        for p in &self.pins {
            if p.closed {
                continue;
            }
            let file = next_pin_filename(&snaps);
            let png = encode_png(&p.rendered());
            if png.is_empty() {
                continue;
            }
            let path = pin_png_path(&dir, &file);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::write(&path, &png).is_err() {
                continue;
            }
            let (x, y) = p.pos();
            snaps.push(PinSnapshot {
                file,
                x,
                y,
                scale: p.scale,
                opacity: p.opacity,
                group: p.group.clone(),
            });
        }
        let sess = SessionFile {
            groups: self.cfg.groups.clone(),
            pins: snaps,
        };
        let _ = sess.save(&dir);
    }

    fn restore_session(&mut self, event_loop: &ActiveEventLoop) {
        let dir = data_dir();
        let sess = SessionFile::load(&dir).unwrap_or_default();
        if !sess.groups.is_empty() {
            self.cfg.groups = sess.groups;
        }
        for snap in sess.pins {
            let path = pin_png_path(&dir, &snap.file);
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let Some(img) = decode_png(&bytes) else {
                continue;
            };
            self.spawn_pin(event_loop, img, Some((snap.x, snap.y)), Some(snap.group));
            if let Some(p) = self.pins.last_mut() {
                p.set_scale(snap.scale);
                p.set_opacity(snap.opacity);
            }
        }
    }

    fn reap_pins(&mut self) {
        let mut i = 0;
        while i < self.pins.len() {
            if self.pins[i].closed {
                let p = self.pins.remove(i);
                self.closed.push(
                    ClosedPin {
                        png: encode_png(&p.rendered()),
                        pos: p.pos(),
                        scale: p.scale,
                        opacity: p.opacity,
                        group: p.group.clone(),
                    },
                    self.cfg.closed_pins_max,
                );
            } else {
                i += 1;
            }
        }
    }
}

fn encode_png(img: &Raster) -> Vec<u8> {
    let rgba = image::RgbaImage::from_raw(img.width, img.height, img.pixels.clone());
    let Some(rgba) = rgba else {
        return Vec::new();
    };
    let mut buf = Vec::new();
    let dynimg = image::DynamicImage::ImageRgba8(rgba);
    let _ = dynimg.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png);
    buf
}

fn decode_png(bytes: &[u8]) -> Option<Raster> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    Raster::from_rgba(img.width(), img.height(), img.into_raw())
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.hotkeys.is_none() {
            match native::install_hotkeys(self.proxy.clone(), &self.cfg) {
                Ok(h) => self.hotkeys = Some(h),
                Err(e) => crate::logutil::log(&format!("hotkeys: {e}")),
            }
        }
        if self.tray.is_none() {
            self.rebuild_tray();
        }
        self.ensure_fallback(event_loop);
        #[cfg(unix)]
        {
            std::thread::spawn(crate::detect::warm_element_backend);
        }
        if !self.session_restored {
            self.session_restored = true;
            self.restore_session(event_loop);
        }
        if self.start_prefs {
            self.start_prefs = false;
            prefs::open(self.cfg.clone(), self.proxy.clone());
        }
        if self.start_snip {
            self.start_snip = false;
            self.request_snip(event_loop, false);
        }
        if self.start_translate {
            self.start_translate = false;
            self.request_snip(event_loop, true);
        }
        if self.start_pin {
            self.start_pin = false;
            self.pin_clipboard(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Hotkey(Command::Snip) => self.request_snip(event_loop, false),
            AppEvent::Hotkey(Command::PinClipboard) => {
                if self.overlay.is_none() {
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
            AppEvent::Hotkey(Command::Quit) => {
                self.persist_session();
                event_loop.exit();
            }
            AppEvent::Hotkey(Command::OpenSettings) => {
                prefs::open(self.cfg.clone(), self.proxy.clone());
            }
            AppEvent::Hotkey(Command::CaptureTranslate) => self.request_snip(event_loop, true),
            AppEvent::Hotkey(Command::SelectionTranslate) => self.selection_translate(),
            AppEvent::Tray(TrayCmd::Snip) => self.request_snip(event_loop, false),
            AppEvent::Tray(TrayCmd::Pin) => self.pin_clipboard(event_loop),
            AppEvent::Tray(TrayCmd::HideAll) => self.toggle_pins(),
            AppEvent::Tray(TrayCmd::Settings) => prefs::open(self.cfg.clone(), self.proxy.clone()),
            AppEvent::Tray(TrayCmd::Quit) => {
                self.persist_session();
                event_loop.exit();
            }
            AppEvent::Tray(TrayCmd::Translate) => self.request_snip(event_loop, true),
            AppEvent::Tray(TrayCmd::Restore) => self.restore_closed(event_loop),
            AppEvent::Tray(TrayCmd::ToggleGroup(name)) => self.toggle_group(name),
            AppEvent::PrefsApplied(cfg) => self.apply_cfg(cfg),
            AppEvent::RaisePrefs => {}
            AppEvent::Ipc(cmd) if cmd == "raise" => {
                if let Some(fb) = &self.fallback {
                    fb.show();
                }
            }
            AppEvent::Ipc(_) => {}
            AppEvent::OcrDone(text, translate) => crate::result_win::open(text, translate),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let WindowEvent::ModifiersChanged(m) = &event {
            self.mods = m.state();
        }

        if let Some(cd) = self.countdown.as_mut() {
            if cd.window_id() == id {
                cd.handle(&event);
                return;
            }
        }
        if let Some(fb) = self.fallback.as_mut() {
            if fb.window_id() == id {
                if let Some(cmd) = fb.handle(&event) {
                    let _ = self.proxy.send_event(AppEvent::Tray(cmd));
                }
                return;
            }
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
                if let WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } = &event
                {
                    let now = Instant::now();
                    if let Some((wid, t)) = self.last_click {
                        if wid == id && now.duration_since(t).as_millis() < 350 {
                            p.on_double_click(&mut self.cfg);
                            self.last_click = None;
                        } else {
                            self.last_click = Some((id, now));
                        }
                    } else {
                        self.last_click = Some((id, now));
                    }
                }
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
        if self.hotkeys.is_none() {
            if let Ok(h) = native::install_hotkeys(self.proxy.clone(), &self.cfg) {
                self.hotkeys = Some(h);
            }
        }
        if let Some(at) = self.snip_at {
            if Instant::now() >= at {
                if self.countdown.is_some() {
                    if self.countdown_left > 1 {
                        self.countdown_left -= 1;
                        if let Some(cd) = &mut self.countdown {
                            cd.set_left(self.countdown_left);
                        }
                        self.snip_at = Some(Instant::now() + Duration::from_secs(1));
                    } else {
                        self.countdown = None;
                        self.prepare_capture_hide();
                        self.snip_at = Some(Instant::now() + Duration::from_millis(self.hide_wait_ms()));
                    }
                } else {
                    self.snip_at = None;
                    self.begin_snip(event_loop);
                    if self.overlay.is_none() {
                        self.restore_fallback_if_needed();
                    }
                }
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(at));
            }
        }
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
        let before = self.pins.len();
        self.reap_pins();
        if self.pins.len() != before {
            self.persist_session();
        }
        crate::result_win::raise_above_pins();
    }
}

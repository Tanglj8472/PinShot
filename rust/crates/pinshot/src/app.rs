//! Application shell: tray, hotkeys, overlay, pins, prefs, session, OCR.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use pinshot_core::config::{self, data_dir, Config};
use pinshot_core::history::{HistoryEntry, HistoryFile, SnipHistory};
use pinshot_core::hotkey::Command;
use pinshot_core::i18n::{set_lang, tr};
use pinshot_core::raster::Raster;
use pinshot_core::session::{
    next_pin_filename, pin_png_path, ClosedPin, ClosedStack, PinSnapshot, SessionFile,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::WindowId;

use crate::autostart;
use crate::capture::Capture;
use crate::clipboard::{self, ClipContent};
use crate::countdown::CountdownWin;
use crate::event::{AppEvent, PinMenuAction, TrayCmd};
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
    let start_pin = args
        .iter()
        .any(|a| a == "--pin" || a == "--pin-clipboard" || a == "pin");
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
        prefs: None,
        pin_menu: None,
        menu_pin: None,
        menu_esc_until: None,
        #[cfg(unix)]
        input_hold: None,
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
    prefs: Option<crate::prefs_win::PrefsWin>,
    pin_menu: Option<crate::pin_menu::PinMenuWin>,
    menu_pin: Option<WindowId>,
    /// Swallow Escape on pins after the menu closes so the still-held Esc
    /// does not close the pin (and persist/reap must not drop the others).
    menu_esc_until: Option<Instant>,
    #[cfg(unix)]
    input_hold: Option<crate::x11util::InputHold>,
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
                Ok(mut f) => {
                    f.apply_theme(&self.cfg.theme);
                    f.retranslate();
                    self.fallback = Some(f);
                }
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
        if let Some(fb) = &mut self.fallback {
            fb.retranslate();
            fb.apply_theme(&self.cfg.theme);
        }
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
            150
        } else {
            150
        }
    }

    fn prepare_capture_hide(&mut self) {
        // Python `_snip_now`: grab pointer+keyboard *before* hiding the
        // fallback so the first drag after F1 is not delivered to Chrome.
        #[cfg(unix)]
        if self.input_hold.is_none() {
            self.input_hold = Some(crate::x11util::InputHold::grab());
        }
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
        #[cfg(unix)]
        if self.input_hold.is_none() {
            self.input_hold = Some(crate::x11util::InputHold::grab());
        }
        #[cfg(unix)]
        let grabbed = Capture::grab_while_held(self.dump_capture);
        #[cfg(not(unix))]
        let grabbed = Capture::grab(self.dump_capture);
        match grabbed {
            Ok(cap) => match Overlay::start(
                event_loop,
                cap,
                self.history.clone(),
                self.cfg.history_max,
                translate,
                self.proxy.clone(),
                #[cfg(unix)]
                self.input_hold.take(),
            ) {
                Ok(ov) => self.overlay = Some(ov),
                Err(e) => {
                    #[cfg(unix)]
                    {
                        self.input_hold = None;
                    }
                    eprintln!("PinShot overlay: {e}");
                }
            },
            Err(e) => {
                #[cfg(unix)]
                {
                    self.input_hold = None;
                }
                eprintln!("PinShot capture: {e}");
            }
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
        self.spawn_pin_ex(event_loop, img, pos, group, true);
    }

    fn spawn_pin_ex(
        &mut self,
        event_loop: &ActiveEventLoop,
        img: Raster,
        pos: Option<(i32, i32)>,
        group: Option<String>,
        persist: bool,
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
                if persist {
                    self.persist_session();
                }
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
        self.persist_history();
        self.overlay = None;
        let resnip = matches!(res, OverlayResult::Resnip);
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
            OverlayResult::Resnip => {}
        }
        if resnip {
            self.request_snip(event_loop, false);
        } else {
            self.restore_fallback_if_needed();
        }
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
                if let Some(p) = self.pins.last_mut() {
                    p.shadow = c.shadow && p.can_shadow;
                    p.thumb = c.thumb;
                    p.set_scale(c.scale);
                    p.set_opacity_ex(c.opacity, false);
                    p.persist_needed = false;
                }
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
            // Persist the original pixels + annotation shapes (Python bakes
            // composed() into the PNG; we keep them separate so restore still
            // lets the user undo/edit after a restart).
            let png = encode_png(&p.img);
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
                shapes: p.annot.shapes.clone(),
                shadow: p.shadow,
                thumb: p.thumb,
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
            self.spawn_pin_ex(
                event_loop,
                img,
                Some((snap.x, snap.y)),
                Some(snap.group),
                false,
            );
            if let Some(p) = self.pins.last_mut() {
                p.shadow = snap.shadow && p.can_shadow;
                p.thumb = snap.thumb;
                p.set_scale(snap.scale);
                p.set_opacity_ex(snap.opacity, false);
                if !snap.shapes.is_empty() {
                    p.restore_shapes(snap.shapes);
                }
                p.persist_needed = false;
            }
        }
    }

    fn persist_history(&self) {
        let dir = HistoryFile::dir(&data_dir());
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let mut names = Vec::new();
        let mut kept = SnipHistory::new();
        for e in &self.history.entries {
            let name = format!("hist-{:03}.png", names.len());
            let png = encode_png(&e.image);
            if png.is_empty() {
                continue;
            }
            if std::fs::write(dir.join(&name), png).is_err() {
                continue;
            }
            names.push(name);
            kept.entries.push(e.clone());
        }
        let file = kept.to_file(&names);
        if let Ok(text) = file.to_toml() {
            let _ = std::fs::write(HistoryFile::path(&data_dir()), text);
        }
    }

    fn restore_history(&mut self) {
        let dir = data_dir();
        let Ok(text) = std::fs::read_to_string(HistoryFile::path(&dir)) else {
            return;
        };
        let Ok(file) = HistoryFile::from_toml(&text) else {
            return;
        };
        let hist_dir = HistoryFile::dir(&dir);
        let mut hist = SnipHistory::new();
        for meta in file.entries {
            let Ok(bytes) = std::fs::read(hist_dir.join(&meta.file)) else {
                continue;
            };
            let Some(img) = decode_png(&bytes) else {
                continue;
            };
            hist.entries.push(HistoryEntry {
                rect: meta.rect(),
                image: img,
            });
        }
        hist.idx = hist.entries.len();
        if !hist.entries.is_empty() {
            crate::logutil::log(&format!(
                "restored {} snip history entries",
                hist.entries.len()
            ));
        }
        self.history = hist;
    }

    fn reap_pins(&mut self) {
        let mut i = 0;
        while i < self.pins.len() {
            if self.pins[i].closed {
                let p = self.pins.remove(i);
                if !p.destroy {
                    self.closed.push(
                        ClosedPin {
                            png: encode_png(&p.rendered()),
                            pos: p.pos(),
                            scale: p.scale,
                            opacity: p.opacity,
                            group: p.group.clone(),
                            shadow: p.shadow,
                            thumb: p.thumb,
                        },
                        self.cfg.closed_pins_max,
                    );
                }
            } else {
                i += 1;
            }
        }
    }

    fn open_prefs_win(&mut self, event_loop: &ActiveEventLoop) {
        if self.prefs.is_some() {
            let _ = self.proxy.send_event(AppEvent::RaisePrefs);
            return;
        }
        match crate::prefs_win::PrefsWin::create(event_loop, self.cfg.clone(), self.proxy.clone()) {
            Ok(w) => self.prefs = Some(w),
            Err(e) => crate::logutil::log(&format!("prefs window: {e}")),
        }
    }

    fn open_pin_menu(&mut self, event_loop: &ActiveEventLoop, pid: WindowId) {
        let Some(p) = self.pins.iter().find(|p| p.window_id() == pid) else {
            return;
        };
        let (px, py) = p.pos();
        let state = crate::pin_menu::PinMenuState {
            annotating: p.annotating,
            click_through: p.click_through,
            shadow: p.shadow,
            scale: p.scale,
            opacity: p.opacity,
            group: p.group.clone(),
            groups: self.cfg.groups.clone(),
            img_w: p.img.width,
            img_h: p.img.height,
        };
        let (mw, mh) = crate::pin_menu::menu_pixel_size(&state);
        let (mx, my) = crate::pin_menu::clamp_menu_pos(
            px + p.last_cursor_i32().0,
            py + p.last_cursor_i32().1,
            mw as i32,
            mh as i32,
            1920,
            1200,
        );
        match crate::pin_menu::PinMenuWin::create(event_loop, (mx, my), state) {
            Ok(m) => {
                self.menu_pin = Some(pid);
                self.pin_menu = Some(m);
                self.menu_esc_until = None;
            }
            Err(e) => crate::logutil::log(&format!("pin menu: {e}")),
        }
    }

    fn dismiss_pin_menu(&mut self) {
        if let Some(m) = self.pin_menu.as_mut() {
            m.close();
        }
        self.pin_menu = None;
        self.menu_esc_until = Some(Instant::now() + Duration::from_millis(450));
    }

    fn pin_esc_guarded(&self) -> bool {
        self.pin_menu.is_some()
            || self
                .menu_esc_until
                .map(|t| Instant::now() < t)
                .unwrap_or(false)
    }

    fn apply_pin_menu(&mut self, action: PinMenuAction) {
        let Some(pid) = self.menu_pin else {
            return;
        };
        if matches!(action, PinMenuAction::NewGroup) {
            let name = next_group_name(&self.cfg.groups);
            let name = pinshot_core::groups::ensure(&mut self.cfg.groups, &name);
            self.group_vis.entry(name.clone()).or_insert(true);
            if let Some(p) = self.pins.iter_mut().find(|p| p.window_id() == pid) {
                p.group = name;
                p.persist_needed = true;
            }
            let _ = pinshot_core::config::save(&self.cfg);
            self.rebuild_tray();
            return;
        }
        if let PinMenuAction::MoveGroup(ref g) = action {
            let vis = *self.group_vis.get(g).unwrap_or(&true);
            if let Some(p) = self.pins.iter_mut().find(|p| p.window_id() == pid) {
                p.apply_menu(action.clone());
                if !vis {
                    p.hide();
                }
            }
            return;
        }
        if let Some(p) = self.pins.iter_mut().find(|p| p.window_id() == pid) {
            p.apply_menu(action);
        }
    }
}

fn next_group_name(groups: &[String]) -> String {
    for i in 1..99 {
        let n = format!("Group {i}");
        if pinshot_core::groups::find(groups, &n).is_none() {
            return n;
        }
    }
    "Group".into()
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
            self.restore_history();
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
            AppEvent::RaisePrefs => {
                if let Some(w) = &self.prefs {
                    w.raise();
                } else {
                    self.open_prefs_win(event_loop);
                }
            }
            AppEvent::OpenPrefs => self.open_prefs_win(event_loop),
            AppEvent::X11Key {
                keysym,
                mods,
                pressed,
            } => {
                if let Some(ov) = self.overlay.as_mut() {
                    ov.handle_x11_key(keysym, mods, pressed);
                    self.finish_overlay(event_loop);
                }
            }
            AppEvent::PinMenu(action) => self.apply_pin_menu(action),
            AppEvent::Ipc(cmd) if cmd == "reload-settings" => {
                match config::load() {
                    Ok(cfg) => self.apply_cfg(cfg),
                    Err(e) => crate::logutil::log(&format!("reload settings: {e}")),
                }
            }
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

        if self
            .prefs
            .as_ref()
            .map(|w| w.window_id() == id)
            .unwrap_or(false)
        {
            if let Some(pw) = self.prefs.as_mut() {
                pw.handle(&event);
            }
            if self.prefs.as_ref().map(|w| w.closed).unwrap_or(false) {
                self.prefs = None;
            }
            return;
        }
        if self
            .pin_menu
            .as_ref()
            .map(|m| m.window_id() == id)
            .unwrap_or(false)
        {
            let action = self.pin_menu.as_mut().and_then(|m| m.handle(&event));
            let closed = self.pin_menu.as_ref().map(|m| m.closed).unwrap_or(false);
            if action.is_some() || closed {
                self.dismiss_pin_menu();
            }
            if let Some(action) = action {
                let _ = self.proxy.send_event(AppEvent::PinMenu(action));
            }
            return;
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
        let mut open_menu = None;
        let mut close_menu = false;
        let swallow_keys = self.pin_esc_guarded();
        let menu_open = self.pin_menu.is_some();
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
                            p.on_double_click(&mut self.cfg, self.mods.shift_key());
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
                if swallow_keys && matches!(&event, WindowEvent::KeyboardInput { .. }) {
                    if let WindowEvent::KeyboardInput { event: kev, .. } = &event {
                        if kev.state == ElementState::Pressed
                            && matches!(kev.logical_key, Key::Named(NamedKey::Escape))
                            && menu_open
                        {
                            close_menu = true;
                        }
                    }
                } else {
                    p.handle(&event);
                }
                if p.pending_menu {
                    p.pending_menu = false;
                    open_menu = Some(id);
                }
                break;
            }
        }
        if close_menu {
            self.dismiss_pin_menu();
        }
        if let Some(pid) = open_menu {
            self.open_pin_menu(event_loop, pid);
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
                        self.snip_at =
                            Some(Instant::now() + Duration::from_millis(self.hide_wait_ms()));
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
        if self.overlay.is_some() {
            // Poll while the overlay is up so QueryPointer can adopt a
            // Button1 that XTEST / a pre-map drag never delivered to winit.
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            let mut wake = self.snip_at;
            for p in &self.pins {
                if let Some(t) = p.next_wake() {
                    wake = Some(wake.map(|w| w.min(t)).unwrap_or(t));
                }
            }
            if let Some(at) = wake {
                if Instant::now() < at {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(at));
                }
            }
        }
        let mut persist = false;
        for p in &mut self.pins {
            p.tick_glow();
            p.pump_redraw();
            if p.persist_needed {
                p.persist_needed = false;
                persist = true;
            }
        }
        if self.pin_menu.as_ref().map(|m| m.closed).unwrap_or(false) {
            self.dismiss_pin_menu();
        }
        if self.prefs.as_ref().map(|w| w.closed).unwrap_or(false) {
            self.prefs = None;
        }
        let before = self.pins.len();
        self.reap_pins();
        if persist || self.pins.len() != before {
            self.persist_session();
        }
        crate::result_win::raise_above_pins();
    }
}

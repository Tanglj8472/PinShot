//! Main-loop settings window for Linux. eframe on a second thread after `gtk::init`
//! fails silently, so Preferences is a winit window on the same event loop.

use std::num::NonZeroU32;
use std::rc::Rc;

use pinshot_core::config::{self, Config};
use pinshot_core::geom::{Point, Rect};
use pinshot_core::hotkey::{bindings_from_config, find_conflicts, normalize_seq};
use pinshot_core::i18n::{self, tr};
use softbuffer::{Context, Surface};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId, WindowLevel};

use crate::draw::{fill_rect, fill_rounded_rect, present, stroke_rect, stroke_rounded_rect, UiTheme};
use crate::event::AppEvent;
use pinshot_core::color::Rgba;

const W: f64 = 520.0;
const H: f64 = 600.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Lang,
    Theme,
    Login,
    Delay,
    Confirm,
    Dbl,
    HistMinus,
    HistPlus,
    ClosedMinus,
    ClosedPlus,
    Browse,
    QualMinus,
    QualPlus,
    Apply,
    Ok,
    Cancel,
    Hk(u8),
    Tab(u8),
}

struct Hit {
    rect: Rect,
    field: Field,
}

pub struct PrefsWin {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    draft: Config,
    proxy: EventLoopProxy<AppEvent>,
    tab: usize,
    hits: Vec<Hit>,
    cursor: Point,
    dirty: bool,
    pub closed: bool,
    armed: Option<u8>,
    mods: ModifiersState,
    status: String,
}

impl PrefsWin {
    pub fn create(
        event_loop: &ActiveEventLoop,
        cfg: Config,
        proxy: EventLoopProxy<AppEvent>,
    ) -> Result<Self, String> {
        crate::logutil::log("prefs: opening settings window");
        let attrs = Window::default_attributes()
            .with_title(tr("PinShot Preferences"))
            .with_inner_size(LogicalSize::new(W, H))
            .with_min_inner_size(LogicalSize::new(440.0, 420.0))
            .with_resizable(true)
            .with_window_level(WindowLevel::AlwaysOnTop);
        let window = Rc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        crate::native::raise_on_top(&window);
        crate::native::focus_window(&window);
        crate::logutil::log("prefs raise above");
        let context = Context::new(window.clone()).map_err(|e| format!("{e:?}"))?;
        let surface = Surface::new(&context, window.clone()).map_err(|e| format!("{e:?}"))?;
        let mut w = Self {
            window,
            surface,
            draft: cfg,
            proxy,
            tab: 0,
            hits: Vec::new(),
            cursor: Point::ZERO,
            dirty: true,
            closed: false,
            armed: None,
            mods: ModifiersState::empty(),
            status: String::new(),
        };
        w.relayout();
        w.redraw();
        Ok(w)
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn raise(&self) {
        self.window.set_visible(true);
        crate::native::raise_on_top(&self.window);
        crate::native::focus_window(&self.window);
        crate::logutil::log("prefs raise above");
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.window.set_visible(false);
        crate::prefs::mark_closed();
    }

    pub fn handle(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Point::new(position.x as f32, position.y as f32);
                self.dirty = true;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                crate::native::focus_window(&self.window);
                if let Some(f) = self.hit() {
                    self.dispatch_tab_or_click(f);
                } else {
                    self.armed = None;
                }
                self.relayout();
                self.dirty = true;
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(i) = self.armed {
                    if let Some(seq) = key_to_seq(&event.logical_key, self.mods) {
                        self.set_hk(i, seq);
                        self.armed = None;
                        self.dirty = true;
                        self.relayout();
                        return;
                    }
                }
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => self.close(),
                    Key::Named(NamedKey::Enter) => {
                        self.apply();
                        self.close();
                    }
                    _ => {}
                }
            }
            WindowEvent::CloseRequested => self.close(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Resized(_) => {
                self.relayout();
                self.dirty = true;
            }
            _ => {}
        }
        if self.dirty {
            self.window.request_redraw();
        }
    }

    fn hit(&self) -> Option<Field> {
        self.hits
            .iter()
            .find(|h| h.rect.contains(self.cursor))
            .map(|h| h.field)
    }

    fn click(&mut self, f: Field) {
        match f {
            Field::Lang => {
                self.draft.language = if self.draft.language == "en" {
                    "zh_CN".into()
                } else {
                    "en".into()
                };
                i18n::set_lang(self.draft.lang());
                self.window.set_title(&tr("PinShot Preferences"));
            }
            Field::Theme => {
                self.draft.theme = match self.draft.theme.as_str() {
                    "system" => "light".into(),
                    "light" => "dark".into(),
                    _ => "system".into(),
                };
            }
            Field::Login => self.draft.run_at_login = !self.draft.run_at_login,
            Field::Delay => {
                self.draft.snip_delay = match self.draft.snip_delay {
                    0 => 1,
                    1 => 2,
                    2 => 3,
                    3 => 5,
                    _ => 0,
                };
            }
            Field::Confirm => self.draft.skip_abort_confirm = !self.draft.skip_abort_confirm,
            Field::Dbl => {
                self.draft.dblclick_pin_action = match self.draft.dblclick_pin_action.as_str() {
                    "close" => "hide".into(),
                    "hide" => "none".into(),
                    _ => "close".into(),
                };
            }
            Field::HistMinus => self.draft.history_max = self.draft.history_max.saturating_sub(1).max(1),
            Field::HistPlus => self.draft.history_max = (self.draft.history_max + 1).min(50),
            Field::ClosedMinus => self.draft.closed_pins_max = self.draft.closed_pins_max.saturating_sub(1),
            Field::ClosedPlus => self.draft.closed_pins_max = (self.draft.closed_pins_max + 1).min(50),
            Field::Browse => {
                if let Some(p) = crate::dialog::open_folder() {
                    self.draft.quick_save_folder = p;
                }
            }
            Field::QualMinus => self.draft.image_quality = self.draft.image_quality.saturating_sub(1).max(1),
            Field::QualPlus => self.draft.image_quality = (self.draft.image_quality + 1).min(100),
            Field::Apply => self.apply(),
            Field::Ok => {
                self.apply();
                self.close();
            }
            Field::Cancel => self.close(),
            Field::Hk(i) => self.armed = Some(i),
            Field::Tab(_) => {}
        }
        // tab buttons encoded as Hk 20/21/22? use dedicated — see relayout
    }

    fn set_hk(&mut self, i: u8, seq: String) {
        let slot = match i {
            0 => &mut self.draft.hotkey_snip,
            1 => &mut self.draft.hotkey_pin,
            2 => &mut self.draft.hotkey_hide_all,
            3 => &mut self.draft.hotkey_restore,
            4 => &mut self.draft.hotkey_selection_translate,
            5 => &mut self.draft.hotkey_capture_translate,
            6 => &mut self.draft.hotkey_clickthrough,
            _ => return,
        };
        *slot = seq;
    }

    fn apply(&mut self) {
        self.draft.hotkey_snip = normalize_seq(&self.draft.hotkey_snip);
        self.draft.hotkey_pin = normalize_seq(&self.draft.hotkey_pin);
        self.draft.hotkey_hide_all = normalize_seq(&self.draft.hotkey_hide_all);
        self.draft.hotkey_restore = normalize_seq(&self.draft.hotkey_restore);
        self.draft.hotkey_clickthrough = normalize_seq(&self.draft.hotkey_clickthrough);
        self.draft.hotkey_selection_translate = normalize_seq(&self.draft.hotkey_selection_translate);
        self.draft.hotkey_capture_translate = normalize_seq(&self.draft.hotkey_capture_translate);
        let conflicts = find_conflicts(&bindings_from_config(&self.draft));
        if !conflicts.is_empty() {
            self.status = format!(
                "{}: {} / {}",
                tr("Conflicting hotkeys"),
                conflicts[0].0,
                conflicts[0].1
            );
            return;
        }
        self.draft = self.draft.clone().validate();
        if let Err(e) = config::save(&self.draft) {
            self.status = e.to_string();
            return;
        }
        i18n::set_lang(self.draft.lang());
        self.window.set_title(&tr("PinShot Preferences"));
        let _ = self.proxy.send_event(AppEvent::PrefsApplied(self.draft.clone()));
        self.status = tr("Settings applied.");
        crate::logutil::log("prefs: applied");
    }

    fn relayout(&mut self) {
        let size = self.window.inner_size();
        let w = size.width.max(8) as f32;
        let h = size.height.max(8) as f32;
        self.hits.clear();
        let mut y = 52.0;
        self.hits.push(Hit {
            rect: Rect::new(16.0, 12.0, 100.0, 28.0),
            field: Field::Tab(0),
        });
        self.hits.push(Hit {
            rect: Rect::new(126.0, 12.0, 100.0, 28.0),
            field: Field::Tab(1),
        });
        self.hits.push(Hit {
            rect: Rect::new(236.0, 12.0, 100.0, 28.0),
            field: Field::Tab(2),
        });
        let row_w = (w - 32.0).max(200.0);
        match self.tab {
            0 => {
                push(self, 16.0, y, row_w, Field::Lang);
                y += 36.0;
                push(self, 16.0, y, row_w, Field::Theme);
                y += 36.0;
                push(self, 16.0, y, row_w, Field::Login);
                y += 36.0;
                push(self, 16.0, y, row_w, Field::Delay);
                y += 36.0;
                push(self, 16.0, y, row_w, Field::Confirm);
                y += 36.0;
                push(self, 16.0, y, row_w, Field::Dbl);
                y += 40.0;
                push(self, 180.0, y, 36.0, Field::HistMinus);
                push(self, 260.0, y, 36.0, Field::HistPlus);
                y += 36.0;
                push(self, 180.0, y, 36.0, Field::ClosedMinus);
                push(self, 260.0, y, 36.0, Field::ClosedPlus);
            }
            1 => {
                for i in 0..7u8 {
                    push(self, 200.0, y, 280.0, Field::Hk(i));
                    y += 36.0;
                }
            }
            _ => {
                push(self, 16.0, y, 360.0, Field::Browse);
                y += 40.0;
                push(self, 180.0, y, 36.0, Field::QualMinus);
                push(self, 260.0, y, 36.0, Field::QualPlus);
            }
        }
        let by = h - 48.0;
        push(self, 16.0, by, 80.0, Field::Apply);
        push(self, 106.0, by, 80.0, Field::Ok);
        push(self, 196.0, by, 80.0, Field::Cancel);
        let _ = w;
    }

    fn redraw(&mut self) {
        self.relayout();
        let size = self.window.inner_size();
        let w = size.width.max(8);
        let h = size.height.max(8);
        let Some(nw) = NonZeroU32::new(w) else {
            return;
        };
        let Some(nh) = NonZeroU32::new(h) else {
            return;
        };
        if self.surface.resize(nw, nh).is_err() {
            return;
        }
        let Some(mut pm) = tiny_skia::Pixmap::new(w, h) else {
            return;
        };
        let thm = UiTheme::resolve(&self.draft.theme);
        fill_rect(
            &mut pm,
            Rect::new(0.0, 0.0, w as f32, h as f32),
            thm.bg,
        );
        for (i, name) in [tr("General"), tr("Hotkeys"), tr("Output")].iter().enumerate() {
            let r = Rect::new(16.0 + i as f32 * 110.0, 12.0, 100.0, 28.0);
            fill_rect(
                &mut pm,
                r,
                if self.tab == i {
                    thm.accent
                } else {
                    thm.panel
                },
            );
            stroke_rect(&mut pm, r, thm.border, 1.0);
            crate::text::blit_into_pm(&mut pm, name, r.x + 10.0, r.y + 6.0, 12, thm.text, true);
        }
        let mut y = 52.0;
        match self.tab {
            0 => {
                y = row_val(&mut pm, y, &tr("Language:"), self.draft.lang().display_name(), thm);
                let tname = match self.draft.theme.as_str() {
                    "light" => tr("Light"),
                    "dark" => tr("Dark"),
                    _ => tr("System"),
                };
                y = row_val(&mut pm, y, &tr("Theme:"), &tname, thm);
                y = row_val(
                    &mut pm,
                    y,
                    &tr("Run at login"),
                    if self.draft.run_at_login { "ON" } else { "OFF" },
                    thm,
                );
                let delay = if self.draft.snip_delay == 0 {
                    tr("None (0 s)")
                } else {
                    tr(&format!("{} s", self.draft.snip_delay))
                };
                y = row_val(&mut pm, y, &tr("Snip delay:"), &delay, thm);
                y = row_val(
                    &mut pm,
                    y,
                    &tr("Confirm before abort (Esc)"),
                    if self.draft.skip_abort_confirm { "OFF" } else { "ON" },
                    thm,
                );
                let dbl = match self.draft.dblclick_pin_action.as_str() {
                    "hide" => tr("Hide pin"),
                    "none" => tr("Do nothing"),
                    _ => tr("Close pin"),
                };
                y = row_val(&mut pm, y, &tr("Double-click pin:"), &dbl, thm);
                y = row_step(
                    &mut pm,
                    y,
                    &tr("Snip history:"),
                    &format!("{}", self.draft.history_max),
                    thm,
                );
                y = row_step(
                    &mut pm,
                    y,
                    &tr("Closed pins:"),
                    &format!("{}", self.draft.closed_pins_max),
                    thm,
                );
            }
            1 => {
                let hks = [
                    (tr("Snip:"), self.draft.hotkey_snip.clone()),
                    (tr("Pin clipboard:"), self.draft.hotkey_pin.clone()),
                    (tr("Hide/show pins:"), self.draft.hotkey_hide_all.clone()),
                    (tr("Restore last pin:"), self.draft.hotkey_restore.clone()),
                    (tr("Select-text translate:"), self.draft.hotkey_selection_translate.clone()),
                    (tr("Snip and translate:"), self.draft.hotkey_capture_translate.clone()),
                    (tr("Click-through:"), self.draft.hotkey_clickthrough.clone()),
                ];
                for (i, (lab, val)) in hks.iter().enumerate() {
                    let armed = self.armed == Some(i as u8);
                    y = row_hk(&mut pm, y, lab, val, armed, thm);
                }
            }
            _ => {
                let folder = if self.draft.quick_save_folder.is_empty() {
                    tr("Browse…")
                } else {
                    self.draft.quick_save_folder.clone()
                };
                y = row_val(&mut pm, y, &tr("Save folder:"), &folder, thm);
                y = row_step(
                    &mut pm,
                    y,
                    &tr("Quality:"),
                    &format!("{}", self.draft.image_quality),
                    thm,
                );
            }
        }
        let _ = y;
        let by = h as f32 - 48.0;
        for (lab, x, _field) in [
            (tr("Apply"), 16.0, Field::Apply),
            (tr("OK"), 106.0, Field::Ok),
            (tr("Cancel"), 196.0, Field::Cancel),
        ] {
            let r = Rect::new(x, by, 80.0, 32.0);
            let hover = r.contains(self.cursor);
            fill_rounded_rect(&mut pm, r, 5.0, if hover { thm.hover } else { thm.panel });
            stroke_rounded_rect(&mut pm, r, 5.0, thm.border, 1.0);
            crate::text::blit_into_pm(&mut pm, &lab, r.x + 12.0, r.y + 7.0, 13, thm.text, true);
        }
        if !self.status.is_empty() {
            crate::text::blit_into_pm(
                &mut pm,
                &self.status,
                16.0,
                by - 22.0,
                11,
                thm.muted,
                false,
            );
        }
        if let Ok(mut buf) = self.surface.buffer_mut() {
            present(&mut buf, &pm);
            let _ = buf.present();
        }
        self.dirty = false;
    }
}

fn push(w: &mut PrefsWin, x: f32, y: f32, ww: f32, field: Field) {
    w.hits.push(Hit {
        rect: Rect::new(x, y, ww, 30.0),
        field,
    });
}

fn row_val(pm: &mut tiny_skia::Pixmap, y: f32, lab: &str, val: &str, thm: UiTheme) -> f32 {
    crate::text::blit_into_pm(pm, lab, 16.0, y + 6.0, 13, thm.text, true);
    let r = Rect::new(200.0, y, 280.0, 30.0);
    fill_rounded_rect(pm, r, 5.0, thm.panel);
    stroke_rounded_rect(pm, r, 5.0, thm.border, 1.0);
    crate::text::blit_into_pm(pm, val, r.x + 8.0, r.y + 6.0, 13, thm.text, false);
    y + 36.0
}

fn row_step(pm: &mut tiny_skia::Pixmap, y: f32, lab: &str, val: &str, thm: UiTheme) -> f32 {
    crate::text::blit_into_pm(pm, lab, 16.0, y + 6.0, 13, thm.text, true);
    btn(pm, Rect::new(180.0, y, 36.0, 30.0), "−", thm);
    crate::text::blit_into_pm(pm, val, 226.0, y + 6.0, 13, thm.text, true);
    btn(pm, Rect::new(260.0, y, 36.0, 30.0), "+", thm);
    y + 36.0
}

fn row_hk(pm: &mut tiny_skia::Pixmap, y: f32, lab: &str, val: &str, armed: bool, thm: UiTheme) -> f32 {
    crate::text::blit_into_pm(pm, lab, 16.0, y + 6.0, 13, thm.text, true);
    let r = Rect::new(200.0, y, 280.0, 30.0);
    fill_rounded_rect(pm, r, 5.0, if armed { Rgba::rgb(255, 236, 179) } else { thm.panel });
    stroke_rounded_rect(pm, r, 5.0, thm.border, 1.0);
    let shown = if val.is_empty() { "—" } else { val };
    crate::text::blit_into_pm(pm, shown, r.x + 8.0, r.y + 6.0, 13, thm.text, false);
    y + 36.0
}

fn btn(pm: &mut tiny_skia::Pixmap, r: Rect, t: &str, thm: UiTheme) {
    fill_rounded_rect(pm, r, 5.0, thm.panel);
    stroke_rounded_rect(pm, r, 5.0, thm.border, 1.0);
    crate::text::blit_into_pm(pm, t, r.x + 10.0, r.y + 5.0, 14, thm.text, true);
}

fn key_to_seq(key: &Key, mods: ModifiersState) -> Option<String> {
    if matches!(
        key,
        Key::Named(NamedKey::Shift | NamedKey::Control | NamedKey::Alt | NamedKey::Super)
    ) {
        return None;
    }
    let k = match key {
        Key::Named(NamedKey::F1) => "F1",
        Key::Named(NamedKey::F2) => "F2",
        Key::Named(NamedKey::F3) => "F3",
        Key::Named(NamedKey::F4) => "F4",
        Key::Named(NamedKey::F5) => "F5",
        Key::Named(NamedKey::F6) => "F6",
        Key::Named(NamedKey::F7) => "F7",
        Key::Named(NamedKey::F8) => "F8",
        Key::Named(NamedKey::F9) => "F9",
        Key::Named(NamedKey::F10) => "F10",
        Key::Named(NamedKey::F11) => "F11",
        Key::Named(NamedKey::F12) => "F12",
        Key::Named(NamedKey::Escape) => "Esc",
        Key::Named(NamedKey::Space) => "Space",
        Key::Named(NamedKey::Tab) => "Tab",
        Key::Character(c) => {
            let s = c.to_ascii_uppercase();
            if s.is_empty() {
                return None;
            }
            let mut parts = Vec::new();
            if mods.control_key() {
                parts.push("Ctrl".into());
            }
            if mods.shift_key() {
                parts.push("Shift".into());
            }
            if mods.alt_key() {
                parts.push("Alt".into());
            }
            parts.push(s);
            return Some(parts.join("+"));
        }
        _ => return None,
    };
    let mut parts = Vec::new();
    if mods.control_key() {
        parts.push("Ctrl");
    }
    if mods.shift_key() {
        parts.push("Shift");
    }
    if mods.alt_key() {
        parts.push("Alt");
    }
    parts.push(k);
    Some(parts.join("+"))
}

impl PrefsWin {
    pub fn click_tab(&mut self, i: usize) {
        self.tab = i.min(2);
        self.armed = None;
        self.relayout();
        self.dirty = true;
    }
}

// Tab hits use Hk(20/21/22) — intercept in click.
impl PrefsWin {
    fn dispatch_tab_or_click(&mut self, f: Field) {
        match f {
            Field::Tab(i) => self.click_tab(i as usize),
            other => self.click(other),
        }
    }
}

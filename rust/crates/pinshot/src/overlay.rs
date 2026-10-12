//! Full-screen snip overlay session (one winit window covering the virtual desktop).

use std::rc::Rc;
use std::time::{Duration, Instant};

use pinshot_core::annotation::{Annotator, Tool};
use pinshot_core::color::Rgba;
use pinshot_core::colorpicker::{color_picker_layout, ColorPicker, PickerAction};
use pinshot_core::confirm::{confirm_abort_layout, ConfirmAbortHit};
use pinshot_core::cursor::cursor_for;
use pinshot_core::geom::{hit_sel_handle, hover_rect, Point, Rect};
use pinshot_core::history::{HistoryEntry, HistoryView, SnipHistory};
use pinshot_core::i18n::tr;
use pinshot_core::magnifier::magnifier_visible;
use pinshot_core::raster::Raster;
use pinshot_core::render::compose;
use pinshot_core::selection::{DetectMode, Nudge, PressKind, SelState, Selection};
use pinshot_core::snap::SnapGuides;
use pinshot_core::wheel::WheelGate;
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
    Ocr(Raster, bool, (i32, i32)),
    Cancel,
    /// Esc then F1 (or F1 while the overlay still holds the grab): start a new snip.
    Resnip,
}

struct ConfirmAbort {
    dont: bool,
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
    elements_rx: Option<std::sync::mpsc::Receiver<Vec<Rect>>>,
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
    held_btn1: bool,
    /// Consecutive QueryPointer Button1-up samples. XGrab often reports a
    /// single false `!down`; two in a row is a real release.
    btn1_up_streak: u8,
    /// QueryPointer sample from the Button1 rising edge. A late winit press
    /// (XGrab) must not start a text box at the drop point.
    pending_press: Option<Point>,
    wheel: WheelGate,
    confirm: Option<ConfirmAbort>,
    color_picker: Option<ColorPicker>,
    sel_move0: Option<Rect>,
    #[cfg(unix)]
    proxy: winit::event_loop::EventLoopProxy<crate::event::AppEvent>,
    #[cfg(unix)]
    key_grab: Option<crate::x11util::OverlayKeyGrab>,
    #[cfg(not(windows))]
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    ime_preedit: bool,
}

impl Overlay {
    pub fn start(
        event_loop: &ActiveEventLoop,
        capture: Capture,
        history: SnipHistory,
        history_max: u32,
        translate_on_select: bool,
        proxy: winit::event_loop::EventLoopProxy<crate::event::AppEvent>,
        #[cfg(unix)] pre_hold: Option<crate::x11util::InputHold>,
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
        let window = Rc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        native::apply_overlay_style(&window);
        window.set_cursor(CursorIcon::Crosshair);
        // IME only while a text editor is open; a standing IME grab lets Tab
        // leak to Chrome before XGrabKeyboard wins.
        window.set_ime_allowed(false);
        #[cfg(not(windows))]
        let surface = {
            let ctx = softbuffer::Context::new(window.clone()).map_err(|e| format!("{e:?}"))?;
            Some(softbuffer::Surface::new(&ctx, window.clone()).map_err(|e| format!("{e:?}"))?)
        };
        let front = OverlayFront::new(&capture.composed, 110)?;
        let windows = translated_windows(capture.virtual_origin);
        crate::logutil::log(&format!("detect windows={}", windows.len()));
        let (elem_tx, elem_rx) = std::sync::mpsc::channel();
        let origin = capture.virtual_origin;
        std::thread::spawn(move || {
            let elems = translated_elements(origin);
            let _ = elem_tx.send(elems);
        });
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
            elements_rx: Some(elem_rx),
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
            held_btn1: false,
            btn1_up_streak: 0,
            pending_press: None,
            wheel: WheelGate::new(),
            confirm: None,
            color_picker: None,
            sel_move0: None,
            #[cfg(unix)]
            proxy: proxy.clone(),
            #[cfg(unix)]
            key_grab: None,
            #[cfg(not(windows))]
            surface,
            ime_preedit: false,
        };
        s.window.set_visible(true);
        native::focus_window(&s.window);
        if s.translate_on_select {
            s.show_toast(tr("Select text, then release"));
        }
        #[cfg(unix)]
        {
            // Release InputHold *before* the exclusive overlay grab, otherwise
            // the first XGrabKeyboard fails (AlreadyGrabbed) and Tab leaks to
            // Chrome for ~80 ms.
            drop(pre_hold);
            s.key_grab = native::grab_overlay_keys(&s.window, proxy);
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
        self.mark_dirty();
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.window.request_redraw();
    }

    fn sync_cursor(&mut self) {
        let over_chrome = self.confirm.is_some()
            || self.color_picker.is_some()
            || self
                .toolbar
                .as_ref()
                .is_some_and(|t| t.contains(self.cursor));
        let mut hint = self.annot.cursor_hint(self.img_pt(self.cursor));
        hint.over_chrome = over_chrome;
        hint.sel_state = Some(self.sel.state);
        hint.hover_sel_handle = self.sel.resize_handle().or_else(|| {
            if self.annot.tool.is_none() && self.sel.has_sel() {
                hit_sel_handle(self.sel.normalized(), self.cursor)
            } else {
                None
            }
        });
        hint.over_selection = self.sel.has_sel() && self.sel.normalized().contains(self.cursor);
        crate::cursor::apply(&self.window, cursor_for(hint));
    }

    fn release_grab(&mut self) {
        #[cfg(unix)]
        if let Some(mut g) = self.key_grab.take() {
            g.release();
        }
    }

    fn expire_toast(&mut self) {
        if let Some(t) = self.toast_until {
            if Instant::now() >= t {
                self.toast = None;
                self.toast_until = None;
                self.mark_dirty();
            }
        }
    }

    fn set_ime_pause(&mut self, pause: bool) {
        #[cfg(unix)]
        if let Some(g) = &self.key_grab {
            g.set_pause_keys(pause);
        }
        #[cfg(not(unix))]
        let _ = pause;
    }

    fn sync_ime(&mut self) {
        let editing = self.annot.text_edit.is_some();
        self.window.set_ime_allowed(editing);
        // Keep XGrabKeyboard while typing so xdotool / Unicode keysyms reach
        // the overlay. Pause only during IME preedit so XIM can compose.
        self.set_ime_pause(editing && self.ime_preedit);
        if let Some(r) = self.annot.text_edit_rect() {
            let dpr = self.dpr.max(0.01);
            self.window.set_ime_cursor_area(
                PhysicalPosition::new((r.x / dpr) as f64, (r.y / dpr) as f64),
                PhysicalSize::new((r.w / dpr).max(8.0) as f64, (r.h / dpr).max(8.0) as f64),
            );
        }
    }

    fn commit_annot_release(&mut self, pt: Point) {
        let before = self.annot.shapes.len();
        self.annot.release(self.img_pt(pt));
        self.sync_ime();
        if self.annot.shapes.len() > before {
            if let Some(sh) = self.annot.shapes.last() {
                let b = sh.bounds();
                crate::logutil::log(&format!(
                    "annot {:?} {:.0}x{:.0} selected={:?}",
                    sh.kind, b.w, b.h, self.annot.selected
                ));
            }
        }
        self.relayout_toolbar();
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn cursor(&self) -> Point {
        self.cursor
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

    fn poll_elements(&mut self) {
        let Some(rx) = self.elements_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(elems) => {
                crate::logutil::log(&format!(
                    "detect elements={} ({})",
                    elems.len(),
                    element_source()
                ));
                self.elements = elems;
                self.elements_rx = None;
                self.update_hover();
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.elements_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    fn snap_guides(&self) -> SnapGuides {
        let mut rects = self.windows.clone();
        rects.extend(self.elements.iter().copied());
        rects.push(self.bounds());
        SnapGuides::from_rects(rects)
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
                self.annot.option_style(),
            );
            tb.set_width_value(self.annot.option_width());
            self.toolbar = Some(tb);
        } else {
            self.toolbar = None;
        }
    }

    fn log_tb_chrome(&self) {
        let Some(tb) = &self.toolbar else {
            return;
        };
        crate::logutil::log(&format!(
            "tb bar {:.0},{:.0} {:.0}x{:.0}",
            tb.bar.x, tb.bar.y, tb.bar.w, tb.bar.h
        ));
        for (name, act) in [
            ("Rect", TbAction::Tool(Tool::Rect)),
            ("Line", TbAction::Tool(Tool::Line)),
            ("Pen", TbAction::Tool(Tool::Pen)),
            ("Mosaic", TbAction::Tool(Tool::Mosaic)),
            ("Text", TbAction::Tool(Tool::Text)),
            ("Eraser", TbAction::Tool(Tool::Eraser)),
            ("Ocr", TbAction::Ocr),
            ("Pin", TbAction::Pin),
        ] {
            if let Some(c) = tb.button_center(act) {
                crate::logutil::log(&format!("tb {name} at {:.0},{:.0}", c.x, c.y));
            }
        }
        if let Some(pr) = tb.panel {
            crate::logutil::log(&format!(
                "tb panel {:.0},{:.0} {:.0}x{:.0}",
                pr.x, pr.y, pr.w, pr.h
            ));
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
            crate::logutil::log(&format!("overlay copy {}x{}", img.width, img.height));
            self.release_grab();
            self.result = Some(OverlayResult::Copy(img, self.sel.normalized()));
            self.window.set_visible(false);
        }
    }

    fn finish_pin(&mut self) {
        if let Some(img) = self.crop() {
            self.remember();
            let (vx, vy) = self.capture.virtual_origin;
            let r = self.sel.normalized();
            self.release_grab();
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
            self.release_grab();
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
        let (vx, vy) = self.capture.virtual_origin;
        let r = self.sel.normalized();
        self.release_grab();
        self.result = Some(OverlayResult::Ocr(
            img,
            translate,
            (vx + r.x as i32, vy + r.y as i32),
        ));
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
            // R reapplies the last selection rect on the *current* capture.
            // `,` / `.` still replace the shot via apply_replay.
            self.annot.clear();
            self.sel.set_rect(e.rect, self.bounds());
            self.relayout_toolbar();
            crate::logutil::log(&format!(
                "R last rect {:.0}x{:.0} on live capture {}x{}",
                e.rect.w, e.rect.h, self.capture.composed.width, self.capture.composed.height
            ));
            self.show_toast(tr("Last snip area"));
            self.mark_dirty();
        }
    }

    fn apply_replay(&mut self, img: &Raster, rect: Option<Rect>) {
        self.capture.composed = img.clone();
        let _ = self.front.replace_shot(img, hwnd_of(&self.window));
        self.annot.clear();
        if let Some(r) = rect {
            self.sel.set_rect(r, self.bounds());
            self.relayout_toolbar();
        }
        // Live (no rect): Python keeps the current selection.
        self.mark_dirty();
    }

    fn load_elements(&mut self) {
        self.poll_elements();
        if self.elements.is_empty() {
            self.elements = translated_elements(self.capture.virtual_origin);
            crate::logutil::log(&format!(
                "detect elements={} ({})",
                self.elements.len(),
                element_source()
            ));
        }
    }

    fn cancel(&mut self) {
        crate::logutil::log("overlay cancel");
        self.release_grab();
        self.result = Some(OverlayResult::Cancel);
        self.window.set_visible(false);
    }

    fn resnip(&mut self) {
        crate::logutil::log("overlay F1 resnip");
        self.release_grab();
        self.result = Some(OverlayResult::Resnip);
        self.window.set_visible(false);
    }

    fn apply_tb(&mut self, a: TbAction) {
        match a {
            TbAction::Tool(_) | TbAction::Arrow => {
                crate::toolbar::toggle_tool(&mut self.annot, &mut self.arrow_on, a)
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
            TbAction::Check => {}
            TbAction::Width(d) => {
                self.annot.adjust_width(d);
            }
            TbAction::WidthSet(v) => {
                self.annot.set_option_width(v);
            }
            TbAction::Color(i) => {
                if let Some(tool) = self.annot.tool.filter(|t| *t != Tool::Select) {
                    let c = pinshot_core::palette_colors()[i as usize];
                    self.annot.set_style_value(tool, |s| s.color = c);
                }
            }
            TbAction::CustomColor => self.pick_custom_color(),
            TbAction::Fill(on) => self.style_tool(|s| s.filled = on),
            TbAction::LineStyle(ls) => self.style_tool(|s| s.line_style = ls),
            TbAction::SetMode(mode) => self.style_tool(|s| s.mode = mode),
            TbAction::SetEffect(effect) => self.style_tool(|s| s.effect = effect),
            TbAction::Bold(on) => {
                self.style_tool(|s| s.bold = on);
                self.log_font_resolve();
            }
            TbAction::Italic(on) => {
                self.style_tool(|s| s.italic = on);
                self.log_font_resolve();
            }
            TbAction::Outline(on) => self.style_tool(|s| s.outline = on),
            TbAction::Family(fam) => {
                self.style_tool(|s| s.family = fam.to_string());
                self.log_font_resolve();
            }
        }
        crate::logutil::log(&format!("tb {a:?}"));
        self.relayout_toolbar();
        self.held_btn1 = true;
        self.mark_dirty();
    }

    fn style_tool(&mut self, setter: impl Fn(&mut pinshot_core::annotation::Style)) {
        if let Some(tool) = self.annot.option_tool().filter(|t| *t != Tool::Select) {
            self.annot.set_style_value(tool, setter);
        }
    }

    fn log_font_resolve(&self) {
        let st = self.annot.option_style();
        let fam = st.map(|s| s.family.as_str()).unwrap_or("");
        let bold = st.map(|s| s.bold).unwrap_or(false);
        let italic = st.map(|s| s.italic).unwrap_or(false);
        if let Some(p) = pinshot_core::font::resolve_family_path(fam, bold, italic) {
            crate::logutil::log(&format!(
                "font resolve {fam:?} bold={bold} italic={italic} -> {}",
                p.display()
            ));
        }
    }

    fn pick_custom_color(&mut self) {
        let Some(tool) = self.annot.option_tool().filter(|t| *t != Tool::Select) else {
            return;
        };
        let cur = self
            .annot
            .option_style()
            .map(|s| s.color)
            .unwrap_or_else(|| self.annot.style(tool).color);
        self.color_picker = Some(ColorPicker::new(cur));
        crate::logutil::log(&format!("color picker open {}", cur.hex()));
        self.mark_dirty();
    }

    fn picker_layout(&self) -> pinshot_core::ColorPickerLayout {
        let b = self.bounds();
        color_picker_layout(b.w, b.h)
    }

    fn picker_press(&mut self, pt: Point) {
        let lay = self.picker_layout();
        let Some(picker) = self.color_picker.as_mut() else {
            return;
        };
        let Some(hit) = lay.hit(pt) else {
            return;
        };
        match picker.press(hit, pt, &lay) {
            PickerAction::Accept => self.accept_picker(),
            PickerAction::Cancel => self.close_picker(),
            PickerAction::Continue => self.mark_dirty(),
        }
    }

    fn picker_drag(&mut self, pt: Point) {
        let lay = self.picker_layout();
        if let Some(picker) = self.color_picker.as_mut() {
            if picker.drag.is_some() {
                picker.drag_to(pt, &lay);
                self.dirty = true;
            }
        }
    }

    fn accept_picker(&mut self) {
        let Some(picker) = self.color_picker.take() else {
            return;
        };
        if let Some(tool) = self.annot.option_tool().filter(|t| *t != Tool::Select) {
            let c = picker.current;
            self.annot.set_style_value(tool, |s| s.color = c);
            crate::logutil::log(&format!("custom color {}", c.hex()));
        }
        self.relayout_toolbar();
        self.mark_dirty();
    }

    fn close_picker(&mut self) {
        if self.color_picker.take().is_some() {
            crate::logutil::log("color picker cancel");
            self.mark_dirty();
        }
    }

    fn finish_sel_drag(&mut self, pt: Point) {
        let guides = self.snap_guides();
        self.sel
            .release_snapped(pt, self.hover, self.bounds(), Some(&guides));
        self.log_sel_translate();
        self.relayout_toolbar();
        self.log_tb_chrome();
    }

    fn apply_sel_motion(&mut self, pt: Point) {
        let moving = self.sel.state == SelState::Moving;
        let before = self.sel.normalized();
        let guides = self.snap_guides();
        self.sel.motion_snapped(pt, self.bounds(), Some(&guides));
        if moving {
            let after = self.sel.normalized();
            let dx = (after.x - before.x) * self.dpr;
            let dy = (after.y - before.y) * self.dpr;
            if dx != 0.0 || dy != 0.0 {
                self.annot.translate_all(dx, dy);
            }
        }
    }

    fn log_sel_translate(&mut self) {
        if let Some(o) = self.sel_move0.take() {
            let n = self.sel.normalized();
            let dx = (n.x - o.x) * self.dpr;
            let dy = (n.y - o.y) * self.dpr;
            if dx != 0.0 || dy != 0.0 {
                crate::logutil::log(&format!("annot translate {dx:.0},{dy:.0}"));
            }
        }
    }

    fn sync_pointer(&mut self) {
        if let Some((x, y, _)) = native::pointer_in_window(&self.window) {
            let (vx, vy) = self.capture.virtual_origin;
            self.cursor = Point::new((x - vx) as f32, (y - vy) as f32);
        }
    }

    fn request_abort(&mut self) {
        if !self.sel.has_sel() {
            self.cancel();
            return;
        }
        let skip = pinshot_core::config::load()
            .map(|c| c.skip_abort_confirm)
            .unwrap_or(false);
        if skip {
            self.cancel();
            return;
        }
        if self.confirm.is_some() {
            return;
        }
        self.annot.finish();
        self.confirm = Some(ConfirmAbort { dont: false });
        crate::logutil::log("confirm abort shown");
        self.mark_dirty();
    }

    fn confirm_yes(&mut self) {
        let dont = self.confirm.as_ref().map(|c| c.dont).unwrap_or(false);
        self.confirm = None;
        if dont {
            if let Ok(mut cfg) = pinshot_core::config::load() {
                cfg.skip_abort_confirm = true;
                let _ = pinshot_core::config::save(&cfg);
            }
            crate::logutil::log("confirm abort yes (do not show again)");
        } else {
            crate::logutil::log("confirm abort yes");
        }
        self.cancel();
    }

    fn confirm_no(&mut self) {
        self.confirm = None;
        crate::logutil::log("confirm abort no");
        self.mark_dirty();
    }

    fn hit_confirm(&mut self, pt: Point) -> bool {
        if self.confirm.is_none() {
            return false;
        }
        let b = self.bounds();
        let lay = confirm_abort_layout(b.w, b.h);
        match lay.hit(pt) {
            Some(ConfirmAbortHit::Yes) => self.confirm_yes(),
            Some(ConfirmAbortHit::No) => self.confirm_no(),
            Some(ConfirmAbortHit::Dont) => {
                if let Some(c) = self.confirm.as_mut() {
                    c.dont = !c.dont;
                    crate::logutil::log(&format!("confirm abort dont={}", c.dont));
                }
                self.mark_dirty();
            }
            None => {}
        }
        true
    }

    pub fn handle(&mut self, event: &WindowEvent) {
        self.poll_elements();
        if self.color_picker.is_some() {
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    self.cursor = Point::new(position.x as f32, position.y as f32);
                    self.picker_drag(self.cursor);
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if *state == ElementState::Pressed {
                        self.picker_press(self.cursor);
                    } else if let Some(p) = self.color_picker.as_mut() {
                        p.release();
                    }
                }
                WindowEvent::KeyboardInput { event, .. }
                    if event.state == ElementState::Pressed =>
                {
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => self.close_picker(),
                        Key::Named(NamedKey::Enter) => self.accept_picker(),
                        _ => {}
                    }
                }
                WindowEvent::RedrawRequested => self.redraw(),
                WindowEvent::CloseRequested => {
                    crate::logutil::log("overlay CloseRequested ignored");
                }
                _ => {}
            }
            if self.dirty {
                self.window.request_redraw();
            }
            self.sync_cursor();
            return;
        }
        match event {
            WindowEvent::CursorMoved { position, .. } if self.confirm.is_some() => {
                self.cursor = Point::new(position.x as f32, position.y as f32);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let next = Point::new(position.x as f32, position.y as f32);
                let same_px = next.x.round() == self.cursor.x.round()
                    && next.y.round() == self.cursor.y.round();
                self.cursor = next;
                if self.annot.is_brush_live()
                    || self.annot.is_selecting()
                    || self.annot.is_dragging_text()
                    || self.annot.live.is_some()
                {
                    self.annot.move_to(self.img_pt(self.cursor), false);
                    self.dirty = true;
                } else if self
                    .toolbar
                    .as_ref()
                    .is_some_and(|t| t.contains(self.cursor))
                {
                    self.dirty = true;
                } else if matches!(
                    self.sel.state,
                    SelState::Creating | SelState::Moving | SelState::Resizing
                ) {
                    self.apply_sel_motion(self.cursor);
                    self.dirty = true;
                } else {
                    let prev_hover = self.hover;
                    self.update_hover();
                    if !same_px || prev_hover != self.hover {
                        self.dirty = true;
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pending = self.pending_press.take();
                // Rising-edge snapshot is only for a *new* rectangle: XGrab
                // drops CursorMoved, so Create must start at the real press.
                // Handle / move presses use the current point — otherwise a
                // stale origin (~13px off) misses the handle and becomes Move.
                let pt = if *state == ElementState::Pressed {
                    if self.sel.has_sel() || pending.is_none() {
                        self.sync_pointer();
                    }
                    press_from_pending(self.sel.has_sel(), pending, self.cursor)
                } else {
                    self.sync_pointer();
                    self.cursor
                };
                self.cursor = pt;
                if *state == ElementState::Pressed && self.hit_confirm(pt) {
                    return;
                }
                if *state == ElementState::Pressed {
                    native::focus_window(&self.window);
                }
                if *state == ElementState::Pressed && *button == MouseButton::Left {
                    self.held_btn1 = true;
                    if let Some(tb) = &self.toolbar {
                        if let Some(a) = tb.hit(pt) {
                            self.apply_tb(a);
                            return;
                        }
                    }
                    if self.sel.has_sel() && self.annot.press(self.img_pt(pt), false) {
                        self.sync_ime();
                        self.relayout_toolbar();
                        self.mark_dirty();
                        return;
                    }
                    match self.sel.press(pt, self.bounds(), self.annot.tool.is_some()) {
                        PressKind::Ignore => {}
                        PressKind::Move => {
                            self.sel_move0 = Some(self.sel.normalized());
                            self.toolbar = None;
                        }
                        _ => self.toolbar = None,
                    }
                    self.dirty = true;
                } else if *state == ElementState::Pressed && *button == MouseButton::Middle {
                    if self.annot.text_edit.is_some() {
                        if let Ok(s) = crate::clipboard::read_text() {
                            self.annot.insert_str(&s);
                            self.sync_ime();
                            self.mark_dirty();
                        }
                    }
                } else if *state == ElementState::Pressed && *button == MouseButton::Right {
                    // Snipaste: right-click ends the current annotation (incl. text).
                    if self.annot.right_click() {
                        self.sync_ime();
                        self.relayout_toolbar();
                        self.mark_dirty();
                        return;
                    }
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
                    self.held_btn1 = false;
                    if self.annot.live.is_some()
                        || self.annot.is_selecting()
                        || self.annot.is_dragging_text()
                        || self.annot.is_swallowing_text_click()
                        || self.annot.is_editing_text()
                    {
                        self.commit_annot_release(pt);
                    } else {
                        self.finish_sel_drag(pt);
                        if self.translate_on_select && self.sel.has_sel() {
                            self.finish_ocr(true);
                        }
                    }
                    self.dirty = true;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.confirm.is_some() {
                    return;
                }
                self.sync_pointer();
                let raw = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (*y * 120.0) as i32,
                    MouseScrollDelta::PixelDelta(p) => p.y as i32,
                };
                let Some(angle) = self.wheel.take(raw) else {
                    return;
                };
                if self.annot.wheel_at(self.img_pt(self.cursor), angle) {
                    crate::logutil::log(&format!(
                        "overlay wheel {angle:+} width={}",
                        self.annot.option_width()
                    ));
                    self.relayout_toolbar();
                    self.mark_dirty();
                }
            }
            WindowEvent::Ime(ime) => match ime {
                winit::event::Ime::Commit(s) => {
                    crate::logutil::log(&format!("text ime commit {:?}", s));
                    self.annot.insert_str(s);
                    self.ime_preedit = false;
                    self.sync_ime();
                    self.mark_dirty();
                }
                winit::event::Ime::Preedit(s, _) => {
                    self.ime_preedit = !s.is_empty();
                    self.sync_ime();
                }
                winit::event::Ime::Enabled => {
                    self.sync_ime();
                }
                winit::event::Ime::Disabled => {
                    self.ime_preedit = false;
                    self.sync_ime();
                }
            },
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
                    self.sync_ime();
                } else if self.result.is_none() {
                    native::focus_window(&self.window);
                    #[cfg(unix)]
                    if let Some(g) = &self.key_grab {
                        g.kick();
                    }
                    self.sync_ime();
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                self.mag_forced = self.mods.contains(ModifiersState::ALT);
                self.dirty = true;
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::CloseRequested => {
                crate::logutil::log("overlay CloseRequested ignored");
            }
            _ => {}
        }
        if self.dirty {
            self.window.request_redraw();
        }
        self.sync_cursor();
    }

    pub fn handle_x11_key(&mut self, keysym: u32, mods: u16, pressed: bool) {
        self.mods = x11_mods_to_winit(mods);
        self.mag_forced = self.mods.contains(ModifiersState::ALT);
        if !pressed {
            self.mark_dirty();
            return;
        }
        let Some((key, text)) = keysym_to_winit(keysym).or_else(|| {
            // Direct UCS keysym without the 0x01000000 prefix (some xdotool setups).
            let ch = char::from_u32(keysym)?;
            if ch.is_control() || (keysym >= 0xff00 && keysym <= 0xffff) {
                return None;
            }
            let s = ch.to_string();
            Some((Key::Character(s.clone().into()), Some(s)))
        }) else {
            return;
        };
        self.key(&key, text.as_deref());
        self.sync_ime();
        self.sync_cursor();
        self.window.request_redraw();
    }

    fn key(&mut self, key: &Key, text: Option<&str>) {
        if self.color_picker.is_some() {
            match key {
                Key::Named(NamedKey::Enter) => self.accept_picker(),
                Key::Named(NamedKey::Escape) => self.close_picker(),
                _ => {}
            }
            return;
        }
        if self.confirm.is_some() {
            match key {
                Key::Named(NamedKey::Enter) => self.confirm_yes(),
                Key::Named(NamedKey::Escape) => self.confirm_no(),
                Key::Character(c) if c.eq_ignore_ascii_case("y") => self.confirm_yes(),
                Key::Character(c) if c.eq_ignore_ascii_case("n") => self.confirm_no(),
                _ => {}
            }
            return;
        }
        if self.annot.text_edit.is_some() {
            let ctrl = self.mods.control_key();
            let shift = self.mods.shift_key();
            match key {
                Key::Named(NamedKey::Escape) => self.annot.commit_text(true),
                Key::Named(NamedKey::Enter) => {
                    if shift {
                        self.annot.type_text('\n');
                    } else {
                        if let Some(ed) = &self.annot.text_edit {
                            crate::logutil::log(&format!("text commit {:?}", ed.text));
                        }
                        self.annot.commit_text(false);
                    }
                }
                Key::Named(NamedKey::Backspace) => self.annot.type_text('\u{8}'),
                Key::Named(NamedKey::Delete) => self.annot.type_text('\u{7f}'),
                Key::Named(NamedKey::ArrowLeft) => self.annot.move_caret(-1),
                Key::Named(NamedKey::ArrowRight) => self.annot.move_caret(1),
                Key::Named(NamedKey::Home) => self.annot.move_caret(-2),
                Key::Named(NamedKey::End) => self.annot.move_caret(2),
                Key::Named(NamedKey::F1) => {
                    self.annot.commit_text(true);
                    self.resnip();
                    return;
                }
                Key::Character(c) if c == "v" && ctrl => {
                    if let Ok(s) = crate::clipboard::read_text() {
                        self.annot.insert_str(&s);
                    }
                }
                Key::Character(c) if (c == "1" || c == "[") && !ctrl => {
                    self.annot.adjust_width(-1);
                    self.relayout_toolbar();
                }
                Key::Character(c) if (c == "2" || c == "]") && !ctrl => {
                    self.annot.adjust_width(1);
                    self.relayout_toolbar();
                }
                _ => {
                    if ctrl {
                        // Ctrl+Enter already handled; do not insert 'c' from Ctrl+C etc.
                    } else if let Some(t) = text {
                        self.annot.insert_str(t);
                    }
                }
            }
            self.sync_ime();
            self.mark_dirty();
            return;
        }
        let ctrl = self.mods.control_key();
        let _shift = self.mods.shift_key();
        match key {
            Key::Named(NamedKey::Escape) => self.request_abort(),
            Key::Named(NamedKey::F1) => self.resnip(),
            Key::Named(NamedKey::F5) => {
                if self.sel.has_sel() {
                    self.finish_ocr(true);
                }
            }
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
            Key::Character(c) if c.eq_ignore_ascii_case("t") && self.sel.has_sel() => {
                self.annot.set_tool(Some(Tool::Text));
                self.relayout_toolbar();
                self.dirty = true;
            }
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
                crate::logutil::log("overlay Tab swallowed");
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
            Key::Character(c) if (c == "1" || c == "[") && self.annot.tool.is_some() => {
                self.annot.adjust_width(-1);
                self.dirty = true;
            }
            Key::Character(c) if (c == "2" || c == "]") && self.annot.tool.is_some() => {
                self.annot.adjust_width(1);
                self.dirty = true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("a") && !ctrl => {
                self.move_cursor(-1.0, 0.0)
            }
            Key::Character(c) if c.eq_ignore_ascii_case("d") && !ctrl => self.move_cursor(1.0, 0.0),
            Key::Character(c) if c.eq_ignore_ascii_case("w") && !ctrl => {
                self.move_cursor(0.0, -1.0)
            }
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
        let _ = self.window.set_cursor_position(PhysicalPosition::new(
            self.cursor.x as f64,
            self.cursor.y as f64,
        ));
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
        let before = self.sel.normalized();
        self.sel.nudge(dx, dy, mode, self.bounds());
        if mode == Nudge::Move {
            let after = self.sel.normalized();
            let adx = (after.x - before.x) * self.dpr;
            let ady = (after.y - before.y) * self.dpr;
            if adx != 0.0 || ady != 0.0 {
                self.annot.translate_all(adx, ady);
                crate::logutil::log(&format!("annot translate {adx:.0},{ady:.0}"));
            }
        }
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
                    self.sync_ime();
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
            &self.annot,
            self.toolbar.as_ref(),
            self.toast.as_deref(),
            self.show_cursor,
            &self.capture.composed,
            self.confirm.as_ref().map(|c| c.dont),
            self.color_picker.as_ref(),
        );
        #[cfg(not(windows))]
        if let Some(surface) = &mut self.surface {
            let pm = self.front.pixmap();
            if let (Some(nw), Some(nh)) = (
                std::num::NonZeroU32::new(pm.width()),
                std::num::NonZeroU32::new(pm.height()),
            ) {
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
        self.poll_elements();
        self.recover_pointer_drag();
        self.expire_toast();
        if self.annot.text_edit.is_some() {
            self.dirty = true;
        }
        if self
            .toolbar
            .as_ref()
            .is_some_and(|t| t.contains(self.cursor))
        {
            // Keep tooltips live even when winit skipped CursorMoved (XGrab).
            self.dirty = true;
        }
        if self.dirty {
            self.window.request_redraw();
        }
    }

    /// If ButtonPress landed on the window underneath before the overlay
    /// mapped, adopt the still-held Button1 as a selection drag.
    ///
    /// Recover must **not** start `annot.press`. A QueryPointer rising edge
    /// plus the real winit press (or a same-point recover release) `finish()`es
    /// an empty live shape, after which the physical drag has `held_btn1` and
    /// no live — rect/ellipse/arrow draw nothing. MouseInput owns press;
    /// recover only continues or commits a stroke that already has extent.
    ///
    /// QueryPointer often reports a single false Button1-up during XGrab.
    /// Finishing Creating on that sample looks like a click and adopts the
    /// hovered window (Snipaste only does that on a real click). Debounce
    /// two consecutive ups, and never adopt hover from recover — only a
    /// drag that already produced a rectangle, or a winit release.
    fn recover_pointer_drag(&mut self) {
        if self.confirm.is_some() || self.color_picker.is_some() {
            return;
        }
        let Some((x, y, down)) = native::pointer_in_window(&self.window) else {
            return;
        };
        let vx = self.capture.virtual_origin.0;
        let vy = self.capture.virtual_origin.1;
        let pt = Point::new((x - vx) as f32, (y - vy) as f32);
        if down {
            self.btn1_up_streak = 0;
        } else {
            self.btn1_up_streak = self.btn1_up_streak.saturating_add(1);
        }
        let drawing = self.annot.live.is_some()
            || self.annot.is_selecting()
            || self.annot.is_dragging_text()
            || self.annot.is_brush_live();
        // Snapshot the first QueryPointer sample after Button1 went down.
        // Later samples are the drag; a late winit press must use this origin.
        if down && !self.held_btn1 && self.pending_press.is_none() {
            self.pending_press = Some(pt);
        }
        if down && !drawing {
            if let Some(p) = self.pending_press {
                self.cursor = p;
            } else {
                self.cursor = pt;
            }
        } else {
            self.cursor = pt;
        }
        let over_tb = self
            .toolbar
            .as_ref()
            .map(|t| t.contains(self.cursor))
            .unwrap_or(false);
        if self.sel.has_sel() && self.annot.tool.is_some() && !over_tb {
            match recover_stroke_action(down, drawing, annot_stroke_extent(&self.annot)) {
                RecoverStroke::Move => {
                    self.annot.move_to(self.img_pt(pt), false);
                    self.dirty = true;
                }
                RecoverStroke::Release => {
                    self.commit_annot_release(pt);
                    self.dirty = true;
                }
                RecoverStroke::Idle => {}
            }
        } else if down
            && self.sel.state == SelState::Idle
            && !self.sel.has_sel()
            && self.annot.tool.is_none()
        {
            let origin = self.pending_press.unwrap_or(pt);
            let _ = self.sel.press(origin, self.bounds(), false);
            crate::logutil::log(&format!(
                "recover Button1 drag ({:.0},{:.0})",
                origin.x, origin.y
            ));
            self.dirty = true;
        } else if down
            && matches!(
                self.sel.state,
                SelState::Creating | SelState::Moving | SelState::Resizing
            )
        {
            self.apply_sel_motion(pt);
            self.dirty = true;
        } else if recover_should_commit_sel(
            down,
            self.btn1_up_streak,
            self.sel.state,
            self.sel.has_sel(),
        ) {
            self.finish_sel_drag(pt);
            if self.sel.has_sel() {
                let r = self.sel.normalized();
                crate::logutil::log(&format!("recover release sel {:.0}x{:.0}", r.w, r.h));
            }
            self.dirty = true;
        }
        if !down {
            self.pending_press = None;
        }
        self.held_btn1 = down;
    }

    pub fn next_wake(&self) -> Option<Instant> {
        let ms = if self.annot.is_brush_live() { 8 } else { 16 };
        let poll = Instant::now() + Duration::from_millis(ms);
        match self.toast_until {
            Some(t) => Some(if t < poll { t } else { poll }),
            None => Some(poll),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoverStroke {
    Idle,
    Move,
    Release,
}

/// Prefer the Button1 rising-edge snapshot. QueryPointer is only the fallback
/// when recover has not seen the press yet (timely winit ButtonPress).
/// Once a selection exists, ignore the snapshot so handle/move clicks use the
/// real press point (a stale origin ~13px off misses the handle).
fn press_from_pending(has_sel: bool, pending: Option<Point>, fallback: Point) -> Point {
    if has_sel {
        fallback
    } else {
        pending.unwrap_or(fallback)
    }
}

/// Recover must not treat a click-sized Creating as a window adopt: Snipaste
/// window-detect is click-only, and a false QueryPointer up would snap the
/// whole window. Two consecutive ups, and only a drag that already has a rect
/// (or move/resize), may commit from recover.
fn recover_should_commit_sel(down: bool, up_streak: u8, state: SelState, has_sel: bool) -> bool {
    if down || up_streak < 2 {
        return false;
    }
    match state {
        SelState::Moving | SelState::Resizing => true,
        SelState::Creating => has_sel,
        _ => false,
    }
}

/// Recover continues a live stroke; it never starts one (see `recover_pointer_drag`).
fn recover_stroke_action(down: bool, drawing: bool, live_has_extent: bool) -> RecoverStroke {
    if !drawing {
        return RecoverStroke::Idle;
    }
    if down {
        RecoverStroke::Move
    } else if live_has_extent {
        RecoverStroke::Release
    } else {
        RecoverStroke::Idle
    }
}

fn annot_stroke_extent(annot: &Annotator) -> bool {
    if annot.is_selecting() || annot.is_dragging_text() {
        return true;
    }
    let Some(sh) = annot.live.as_ref() else {
        return false;
    };
    if sh.kind.is_brush(&sh.style) {
        return sh.pts.len() > 1;
    }
    sh.pts.len() > 1 && sh.pts[0].manhattan(*sh.pts.last().unwrap()) > 2.0
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
        0xffe3 | 0xffe4 => Some(NamedKey::Control),
        0xffe9 | 0xffea => Some(NamedKey::Alt),
        0xffeb | 0xffec => Some(NamedKey::Super),
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
    // X11 function-key range (0xff00–0xffff): never treat as Unicode text.
    // Control_L is 0xffe3; char::from_u32 maps that to '￣' and used to leak into text.
    if keysym >= 0xff00 && keysym <= 0xffff {
        return None;
    }
    // Unicode keysym: 0x01000000 | codepoint (xdotool CJK, X11 UTF-8).
    let cp = if keysym & 0xff000000 == 0x01000000 {
        keysym & 0x00ffffff
    } else {
        keysym
    };
    let ch = char::from_u32(cp)?;
    if ch.is_control() {
        return None;
    }
    let s = ch.to_string();
    Some((Key::Character(s.clone().into()), Some(s)))
}

#[cfg(test)]
mod keysym_tests {
    use super::keysym_to_winit;
    use super::{
        annot_stroke_extent, press_from_pending, recover_should_commit_sel, recover_stroke_action,
        RecoverStroke,
    };
    use pinshot_core::annotation::{Annotator, Tool};
    use pinshot_core::geom::Point;
    use pinshot_core::selection::SelState;
    use winit::keyboard::{Key, NamedKey};

    #[test]
    fn control_l_is_modifier_not_macron() {
        match keysym_to_winit(0xffe3) {
            Some((Key::Named(NamedKey::Control), None)) => {}
            other => panic!("Control_L became {other:?}"),
        }
        assert!(keysym_to_winit(0xffe5).is_none()); // Caps_Lock
    }

    #[test]
    fn latin_letters_keep_text() {
        let (k, t) = keysym_to_winit(b'h' as u32).unwrap();
        assert!(matches!(k, Key::Character(c) if c.as_str() == "h"));
        assert_eq!(t.as_deref(), Some("h"));
    }

    #[test]
    fn unicode_keysym_inserts_cjk() {
        let (k, t) = keysym_to_winit(0x01004f60).unwrap();
        assert!(matches!(k, Key::Character(c) if c.as_str() == "你"));
        assert_eq!(t.as_deref(), Some("你"));
    }

    #[test]
    fn recover_never_starts_a_press() {
        assert_eq!(
            recover_stroke_action(true, false, false),
            RecoverStroke::Idle
        );
        assert_eq!(
            recover_stroke_action(true, true, false),
            RecoverStroke::Move
        );
        assert_eq!(
            recover_stroke_action(false, true, false),
            RecoverStroke::Idle
        );
        assert_eq!(
            recover_stroke_action(false, true, true),
            RecoverStroke::Release
        );
    }

    #[test]
    fn same_point_live_has_no_extent() {
        let mut a = Annotator::new();
        a.set_tool(Some(Tool::Rect));
        a.press(Point::new(10.0, 10.0), false);
        assert!(!annot_stroke_extent(&a));
        a.move_to(Point::new(40.0, 30.0), false);
        assert!(annot_stroke_extent(&a));
    }

    #[test]
    fn late_press_uses_rising_edge_not_drop_point() {
        let origin = Point::new(40.0, 80.0);
        let drop = Point::new(120.0, 140.0);
        assert_eq!(press_from_pending(false, Some(origin), drop), origin);
        assert_eq!(press_from_pending(false, None, drop), drop);
    }

    #[test]
    fn handle_press_ignores_pending_origin() {
        let origin = Point::new(40.0, 80.0);
        let handle = Point::new(1900.0, 1060.0);
        assert_eq!(press_from_pending(true, Some(origin), handle), handle);
    }

    #[test]
    fn recover_does_not_commit_click_or_single_up() {
        assert!(!recover_should_commit_sel(
            false,
            1,
            SelState::Creating,
            false
        ));
        assert!(!recover_should_commit_sel(
            false,
            2,
            SelState::Creating,
            false
        ));
        assert!(recover_should_commit_sel(
            false,
            2,
            SelState::Creating,
            true
        ));
        assert!(recover_should_commit_sel(
            false,
            2,
            SelState::Resizing,
            true
        ));
        assert!(!recover_should_commit_sel(
            true,
            2,
            SelState::Creating,
            true
        ));
    }
}

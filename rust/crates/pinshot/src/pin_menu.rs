//! In-process pin context menu (Python `PinWindow.build_menu`). Zenity is not required.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use pinshot_core::geom::{clamp_f32, Point, Rect};
use pinshot_core::groups;
use pinshot_core::i18n::tr;
use softbuffer::{Context, Surface};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::draw::{
    fill_rect, fill_rounded_rect, paint_check_tick, present, stroke_rect, stroke_rounded_rect,
    UiTheme,
};
use crate::event::PinMenuAction;
use pinshot_core::color::Rgba;
use tiny_skia::{FillRule, Paint, PathBuilder, Transform};

const ROW_H: f32 = 22.0;
const MENU_W: f32 = 220.0;
const SUB_W: f32 = 210.0;
const PAD: f32 = 6.0;
const PROMPT_H: u32 = 112;

#[derive(Clone, Copy)]
enum PromptKind {
    New,
    Rename,
}

struct Prompt {
    kind: PromptKind,
    buf: String,
}

/// Overlay-style insert: KeyboardInput text when not composing; skip controls.
fn prompt_insert(buf: &mut String, ime_preedit: bool, text: &str) -> bool {
    if ime_preedit {
        return false;
    }
    let mut changed = false;
    for ch in text.chars() {
        if !ch.is_control() {
            buf.push(ch);
            changed = true;
        }
    }
    changed
}

fn prompt_submit_name(buf: &str) -> Option<String> {
    let name = buf.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SubKind {
    Zoom,
    ImageProc,
    Groups,
    Info,
}

enum Row {
    Sep,
    Head(String),
    Item {
        label: String,
        action: Option<PinMenuAction>,
        checked: bool,
        checkable: bool,
        indent: bool,
        submenu: Option<SubKind>,
    },
}

pub struct PinMenuState {
    pub annotating: bool,
    pub click_through: bool,
    pub shadow: bool,
    pub scale: f32,
    pub opacity: f32,
    pub group: String,
    pub groups: Vec<String>,
    pub group_counts: Vec<(String, usize)>,
    pub img_w: u32,
    pub img_h: u32,
    pub smooth: bool,
    pub grayscale: bool,
    pub invert: bool,
    pub rotation: i32,
    pub mirror_x: bool,
    pub mirror_y: bool,
}

pub struct PinMenuWin {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    rows: Vec<(Rect, Row)>,
    sub_rows: Vec<(Rect, Row)>,
    open_sub: Option<SubKind>,
    state: PinMenuState,
    cursor: Point,
    dirty: bool,
    pub closed: bool,
    born: Instant,
    prompt: Option<Prompt>,
    group: String,
    ime_preedit: bool,
}

impl PinMenuWin {
    pub fn create(
        event_loop: &ActiveEventLoop,
        pos: (i32, i32),
        state: PinMenuState,
    ) -> Result<Self, String> {
        let rows_src = build_rows(&state);
        let (w, h) = menu_pixel_size(&state);
        crate::logutil::log(&format!("pin menu {w}x{h}"));
        if let Some((cx, cy)) = menu_action_center(&state, &PinMenuAction::NewGroup) {
            crate::logutil::log(&format!("pin menu new-group at {cx:.0},{cy:.0}"));
        }
        if let Some((cx, cy)) = menu_action_center(&state, &PinMenuAction::RenameGroup) {
            crate::logutil::log(&format!("pin menu rename-group at {cx:.0},{cy:.0}"));
        }
        let attrs = Window::default_attributes()
            .with_title("PinShot")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(true)
            .with_position(PhysicalPosition::new(pos.0, pos.1))
            .with_inner_size(PhysicalSize::new(w, h))
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
        let window = Rc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        crate::native::raise_on_top(&window);
        crate::native::focus_window(&window);
        let context = Context::new(window.clone()).map_err(|e| format!("{e:?}"))?;
        let surface = Surface::new(&context, window.clone()).map_err(|e| format!("{e:?}"))?;
        let mut y = PAD;
        let mut rows = Vec::new();
        for row in rows_src {
            let hh = match &row {
                Row::Sep => 8.0,
                _ => ROW_H,
            };
            rows.push((Rect::new(0.0, y, MENU_W, hh), row));
            y += hh;
        }
        let group = state.group.clone();
        let mut m = Self {
            window,
            surface,
            rows,
            sub_rows: Vec::new(),
            open_sub: None,
            state,
            cursor: Point::ZERO,
            dirty: true,
            closed: false,
            born: Instant::now(),
            prompt: None,
            group,
            ime_preedit: false,
        };
        m.redraw();
        Ok(m)
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.window.set_visible(false);
    }

    pub fn handle(&mut self, event: &WindowEvent) -> Option<PinMenuAction> {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Point::new(position.x as f32, position.y as f32);
                self.update_submenu();
                self.dirty = true;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                crate::native::focus_window(&self.window);
                if self.prompt.is_some() {
                    // clicks stay in the name field
                } else if let Some(a) = self.hit() {
                    match a {
                        PinMenuAction::NewGroup => self.begin_prompt(PromptKind::New),
                        PinMenuAction::RenameGroup => self.begin_prompt(PromptKind::Rename),
                        other => {
                            self.close();
                            return Some(other);
                        }
                    }
                } else if self.hit_submenu_parent().is_some() {
                    // keep flyout open
                }
            }
            WindowEvent::Ime(ime) => {
                if self.prompt.is_some() {
                    match ime {
                        winit::event::Ime::Commit(s) => {
                            if let Some(p) = self.prompt.as_mut() {
                                p.buf.push_str(s);
                            }
                            self.ime_preedit = false;
                            self.dirty = true;
                        }
                        // Enabled alone must not block KeyboardInput (no IME daemon).
                        // Only a non-empty Preedit is composing CJK.
                        winit::event::Ime::Preedit(s, _) => {
                            self.ime_preedit = !s.is_empty();
                        }
                        winit::event::Ime::Enabled => {}
                        winit::event::Ime::Disabled => self.ime_preedit = false,
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if self.prompt.is_some() {
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            if self.ime_preedit {
                                self.ime_preedit = false;
                            } else {
                                self.close();
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            if !self.ime_preedit {
                                return self.submit_prompt();
                            }
                        }
                        Key::Named(NamedKey::Backspace) => {
                            if let Some(p) = self.prompt.as_mut() {
                                p.buf.pop();
                            }
                            self.dirty = true;
                        }
                        _ => {
                            // Same as overlay text: accept KeyboardInput when not
                            // in IME preedit; Ime::Commit still handles CJK.
                            if let Some(t) = event.text.as_deref() {
                                if prompt_insert(
                                    &mut self.prompt.as_mut().unwrap().buf,
                                    self.ime_preedit,
                                    t,
                                ) {
                                    if let Some(p) = &self.prompt {
                                        crate::logutil::log(&format!(
                                            "pin menu prompt text {:?}",
                                            p.buf
                                        ));
                                    }
                                    self.dirty = true;
                                }
                            }
                        }
                    }
                } else {
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.close();
                        }
                        Key::Named(NamedKey::Enter) => {
                            if let Some(a) = self.hit() {
                                match a {
                                    PinMenuAction::NewGroup => self.begin_prompt(PromptKind::New),
                                    PinMenuAction::RenameGroup => {
                                        self.begin_prompt(PromptKind::Rename)
                                    }
                                    other => {
                                        self.close();
                                        return Some(other);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::Focused(false)
                if self.prompt.is_none() && self.born.elapsed().as_millis() > 250 =>
            {
                self.close()
            }
            WindowEvent::CloseRequested => self.close(),
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
        if self.dirty {
            self.window.request_redraw();
        }
        None
    }

    fn begin_prompt(&mut self, kind: PromptKind) {
        let buf = match kind {
            PromptKind::New => String::new(),
            PromptKind::Rename => groups::label(&self.group),
        };
        self.prompt = Some(Prompt { kind, buf });
        self.window.set_ime_allowed(true);
        let _ = self
            .window
            .request_inner_size(PhysicalSize::new(MENU_W as u32, PROMPT_H));
        self.dirty = true;
        crate::logutil::log(match kind {
            PromptKind::New => "pin menu prompt new group",
            PromptKind::Rename => "pin menu prompt rename group",
        });
    }

    fn submit_prompt(&mut self) -> Option<PinMenuAction> {
        let Some(p) = self.prompt.take() else {
            return None;
        };
        let Some(name) = prompt_submit_name(&p.buf) else {
            crate::logutil::log("pin menu prompt empty ignored");
            self.close();
            return None;
        };
        self.close();
        crate::logutil::log(&format!("pin menu prompt submit {name:?}"));
        Some(match p.kind {
            PromptKind::New => PinMenuAction::NewGroupNamed(name),
            PromptKind::Rename => PinMenuAction::RenameGroupNamed(name),
        })
    }

    fn hit(&self) -> Option<PinMenuAction> {
        for (r, row) in &self.sub_rows {
            if r.contains(self.cursor) {
                if let Row::Item {
                    action: Some(action),
                    ..
                } = row
                {
                    return Some(action.clone());
                }
            }
        }
        for (r, row) in &self.rows {
            if r.contains(self.cursor) {
                if let Row::Item {
                    action: Some(action),
                    submenu: None,
                    ..
                } = row
                {
                    return Some(action.clone());
                }
            }
        }
        None
    }

    fn hit_submenu_parent(&self) -> Option<SubKind> {
        for (r, row) in &self.rows {
            if r.contains(self.cursor) {
                if let Row::Item {
                    submenu: Some(k), ..
                } = row
                {
                    return Some(*k);
                }
            }
        }
        None
    }

    fn update_submenu(&mut self) {
        let mut kind = self.hit_submenu_parent();
        if kind.is_none() {
            if self.sub_rows.iter().any(|(r, _)| r.contains(self.cursor)) {
                kind = self.open_sub;
            }
        }
        if kind == self.open_sub {
            return;
        }
        self.open_sub = kind;
        self.layout_submenu();
        let (w, h) = self.current_size();
        let _ = self.window.request_inner_size(PhysicalSize::new(w, h));
    }

    fn layout_submenu(&mut self) {
        self.sub_rows.clear();
        let Some(kind) = self.open_sub else {
            return;
        };
        let parent_y = self
            .rows
            .iter()
            .find_map(|(r, row)| match row {
                Row::Item {
                    submenu: Some(k), ..
                } if *k == kind => Some(r.y),
                _ => None,
            })
            .unwrap_or(PAD);
        let mut y = parent_y;
        for row in build_sub_rows(&self.state, kind) {
            let hh = row_h(&row);
            self.sub_rows.push((Rect::new(MENU_W, y, SUB_W, hh), row));
            y += hh;
        }
    }

    fn current_size(&self) -> (u32, u32) {
        let main_h = (PAD * 2.0 + self.rows.iter().map(|(r, _)| r.h).sum::<f32>()).ceil() as u32;
        if self.open_sub.is_none() {
            return (MENU_W as u32, main_h.max(40));
        }
        let sub_bottom = self.sub_rows.last().map(|(r, _)| r.bottom()).unwrap_or(0.0);
        let h = main_h.max((sub_bottom + PAD).ceil() as u32).max(40);
        ((MENU_W + SUB_W) as u32, h)
    }

    fn redraw(&mut self) {
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
        let thm = UiTheme::resolve("light");
        fill_rect(&mut pm, Rect::new(0.0, 0.0, w as f32, h as f32), thm.bg);
        stroke_rect(
            &mut pm,
            Rect::new(0.5, 0.5, w as f32 - 1.0, h as f32 - 1.0),
            thm.border,
            1.0,
        );
        if let Some(p) = &self.prompt {
            let title = match p.kind {
                PromptKind::New => tr("New group"),
                PromptKind::Rename => tr("Rename group"),
            };
            crate::text::blit_into_pm(&mut pm, &title, 14.0, 12.0, 14, thm.text, true);
            crate::text::blit_into_pm(&mut pm, &tr("Name:"), 14.0, 40.0, 12, thm.muted, false);
            let field = Rect::new(14.0, 62.0, MENU_W - 28.0, 28.0);
            fill_rounded_rect(&mut pm, field, 5.0, Rgba::WHITE);
            stroke_rounded_rect(&mut pm, field, 5.0, thm.accent, 1.5);
            let shown = if p.buf.is_empty() {
                String::new()
            } else {
                p.buf.clone()
            };
            crate::text::blit_into_pm(
                &mut pm,
                &format!("{shown}|"),
                20.0,
                67.0,
                13,
                thm.text,
                false,
            );
            if let Ok(mut buf) = self.surface.buffer_mut() {
                present(&mut buf, &pm);
                let _ = buf.present();
            }
            self.dirty = false;
            return;
        }
        for (r, row) in &self.rows {
            match row {
                Row::Sep => {
                    fill_rect(
                        &mut pm,
                        Rect::new(10.0, r.y + 3.5, MENU_W - 20.0, 1.0),
                        thm.border,
                    );
                }
                Row::Head(s) => {
                    crate::text::blit_into_pm(
                        &mut pm,
                        s,
                        r.x + 12.0,
                        r.y + 4.0,
                        11,
                        thm.muted,
                        true,
                    );
                }
                Row::Item {
                    label,
                    checked,
                    checkable,
                    indent,
                    submenu,
                    ..
                } => {
                    let hover = r.contains(self.cursor);
                    if hover {
                        fill_rounded_rect(
                            &mut pm,
                            Rect::new(r.x + 4.0, r.y + 1.0, r.w - 8.0, r.h - 2.0),
                            4.0,
                            thm.hover,
                        );
                    }
                    let mut x = r.x + if *indent { 28.0 } else { 12.0 };
                    if *checkable || *checked {
                        paint_check_tick(&mut pm, x, r.y + 5.0, *checked);
                        x += 18.0;
                    }
                    crate::text::blit_into_pm(&mut pm, label, x, r.y + 3.0, 12, thm.text, false);
                    if submenu.is_some() {
                        paint_flyout_arrow(&mut pm, r.right() - 14.0, r.y + 7.0, thm.muted);
                    }
                }
            }
        }
        if !self.sub_rows.is_empty() {
            let y0 = self.sub_rows[0].0.y - 4.0;
            let y1 = self.sub_rows.last().map(|(r, _)| r.bottom()).unwrap_or(y0) + 4.0;
            let card = Rect::new(MENU_W + 2.0, y0, SUB_W - 6.0, (y1 - y0).max(20.0));
            fill_rounded_rect(&mut pm, card, 4.0, thm.bg);
            stroke_rounded_rect(&mut pm, card, 4.0, thm.border, 1.0);
        }
        for (r, row) in &self.sub_rows {
            if let Row::Item {
                label,
                checked,
                checkable,
                indent,
                ..
            } = row
            {
                let hover = r.contains(self.cursor);
                if hover {
                    fill_rounded_rect(
                        &mut pm,
                        Rect::new(r.x + 4.0, r.y + 1.0, r.w - 8.0, r.h - 2.0),
                        4.0,
                        thm.hover,
                    );
                }
                let mut x = r.x + if *indent { 16.0 } else { 10.0 };
                if *checkable || *checked {
                    paint_check_tick(&mut pm, x, r.y + 5.0, *checked);
                    x += 18.0;
                }
                crate::text::blit_into_pm(&mut pm, label, x, r.y + 3.0, 12, thm.text, false);
            } else if let Row::Sep = row {
                fill_rect(
                    &mut pm,
                    Rect::new(r.x + 8.0, r.y + 3.5, r.w - 16.0, 1.0),
                    thm.border,
                );
            }
        }
        if let Ok(mut buf) = self.surface.buffer_mut() {
            present(&mut buf, &pm);
            let _ = buf.present();
        }
        self.dirty = false;
    }
}

fn paint_flyout_arrow(pm: &mut tiny_skia::Pixmap, x: f32, y: f32, color: Rgba) {
    let mut pb = PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + 5.0, y + 4.0);
    pb.line_to(x, y + 8.0);
    pb.close();
    if let Some(path) = pb.finish() {
        let mut p = Paint::default();
        p.set_color(color.to_skia());
        p.anti_alias = true;
        pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
    }
}

fn build_rows(state: &PinMenuState) -> Vec<Row> {
    vec![
        item(tr("Copy image"), PinMenuAction::Copy, false, false),
        item(tr("Save image as…"), PinMenuAction::Save, false, false),
        Row::Sep,
        check_item(
            tr("Show toolbar"),
            PinMenuAction::ToggleToolbar,
            state.annotating,
            false,
        ),
        item(tr("Recognize text"), PinMenuAction::Ocr, false, false),
        item(tr("Translate"), PinMenuAction::Translate, false, false),
        check_item(
            tr("Shadow"),
            PinMenuAction::ToggleShadow,
            state.shadow,
            false,
        ),
        check_item(
            tr("Click-through"),
            PinMenuAction::ToggleClickThrough,
            state.click_through,
            false,
        ),
        Row::Sep,
        flyout(tr("Zoom"), SubKind::Zoom),
        flyout(tr("Image processing"), SubKind::ImageProc),
        Row::Sep,
        item(tr("Paste"), PinMenuAction::Paste, false, false),
        item(
            tr("Replace by file..."),
            PinMenuAction::ReplaceFile,
            false,
            false,
        ),
        flyout(tr("Move to group"), SubKind::Groups),
        item(
            tr("View in folder"),
            PinMenuAction::ViewInFolder,
            false,
            false,
        ),
        Row::Sep,
        item(tr("Close"), PinMenuAction::Close, false, false),
        item(tr("Destroy"), PinMenuAction::Destroy, false, false),
        Row::Sep,
        flyout(format!("{} × {}", state.img_w, state.img_h), SubKind::Info),
    ]
}

fn build_sub_rows(state: &PinMenuState, kind: SubKind) -> Vec<Row> {
    match kind {
        SubKind::Zoom => {
            let mut rows = Vec::new();
            for (label, pct) in [("33.3%", 33), ("50%", 50), ("100%", 100), ("200%", 200)] {
                let on = (state.scale * 100.0 - pct as f32).abs() < 1.0
                    || (pct == 33 && (state.scale - 1.0 / 3.0).abs() < 0.02);
                rows.push(check_item(label, PinMenuAction::Zoom(pct), on, false));
            }
            rows.push(Row::Sep);
            let cur = format!("{}%", (state.scale * 100.0).round() as i32);
            rows.push(check_item(
                format!("{}  {}", cur, tr("Current")),
                PinMenuAction::Zoom((state.scale * 100.0).round() as i32),
                true,
                false,
            ));
            rows.push(check_item(
                tr("Smoothing"),
                PinMenuAction::ToggleSmooth,
                state.smooth,
                false,
            ));
            rows
        }
        SubKind::ImageProc => vec![
            item(tr("Rotate left"), PinMenuAction::Rotate(-1), false, false),
            item(tr("Rotate right"), PinMenuAction::Rotate(1), false, false),
            item(tr("Horizontal flip"), PinMenuAction::FlipH, false, false),
            item(tr("Vertical flip"), PinMenuAction::FlipV, false, false),
        ],
        SubKind::Groups => {
            let mut rows = Vec::new();
            for g in &state.groups {
                let on = g.eq_ignore_ascii_case(&state.group);
                let n = state
                    .group_counts
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(g))
                    .map(|(_, c)| *c)
                    .unwrap_or(0);
                let label = if n > 0 {
                    format!("{:<10} {n}", groups::label(g))
                } else {
                    groups::label(g)
                };
                rows.push(check_item(
                    label,
                    PinMenuAction::MoveGroup(g.clone()),
                    on,
                    false,
                ));
            }
            rows.push(item(
                tr("New group…"),
                PinMenuAction::NewGroup,
                false,
                false,
            ));
            rows.push(item(
                tr("Rename group…"),
                PinMenuAction::RenameGroup,
                false,
                false,
            ));
            rows
        }
        SubKind::Info => {
            let yes = |v: bool| if v { tr("Yes") } else { tr("No") };
            vec![
                info_row(format!("{}: {}", tr("Group"), groups::label(&state.group))),
                info_row(format!(
                    "{}: {}%",
                    tr("Zoom"),
                    (state.scale * 100.0).round() as i32
                )),
                info_row(format!("{}: {}°", tr("Rotation"), state.rotation)),
                info_row(format!(
                    "{}: {}%",
                    tr("Opacity"),
                    (state.opacity * 100.0).round() as i32
                )),
                info_row(format!("{}: {}", tr("X-mirror"), yes(state.mirror_x))),
                info_row(format!("{}: {}", tr("Y-mirror"), yes(state.mirror_y))),
                check_item(
                    format!("{}: {}", tr("Grayscale"), yes(state.grayscale)),
                    PinMenuAction::ToggleGrayscale,
                    state.grayscale,
                    false,
                ),
                check_item(
                    format!("{}: {}", tr("Color inverted"), yes(state.invert)),
                    PinMenuAction::ToggleInvert,
                    state.invert,
                    false,
                ),
                Row::Sep,
                item(tr("Copy size"), PinMenuAction::CopySize, false, false),
                item(tr("Reset zoom"), PinMenuAction::ResetZoom, false, false),
            ]
        }
    }
}

fn item(label: impl Into<String>, action: PinMenuAction, checked: bool, indent: bool) -> Row {
    item_ex(label, Some(action), checked, false, indent, None)
}

fn check_item(label: impl Into<String>, action: PinMenuAction, checked: bool, indent: bool) -> Row {
    item_ex(label, Some(action), checked, true, indent, None)
}

fn flyout(label: impl Into<String>, kind: SubKind) -> Row {
    item_ex(label, None, false, false, false, Some(kind))
}

fn info_row(label: impl Into<String>) -> Row {
    item_ex(label, None, false, false, false, None)
}

fn item_ex(
    label: impl Into<String>,
    action: Option<PinMenuAction>,
    checked: bool,
    checkable: bool,
    indent: bool,
    submenu: Option<SubKind>,
) -> Row {
    Row::Item {
        label: label.into(),
        action,
        checked,
        checkable,
        indent,
        submenu,
    }
}

fn row_h(row: &Row) -> f32 {
    match row {
        Row::Sep => 8.0,
        _ => ROW_H,
    }
}

/// Client-pixel centre of a menu action row (for Xvfb clicks).
pub fn menu_action_center(state: &PinMenuState, want: &PinMenuAction) -> Option<(f32, f32)> {
    let rows = build_rows(state);
    let mut y = PAD;
    for row in &rows {
        let hh = row_h(row);
        if let Row::Item {
            action: Some(action),
            ..
        } = row
        {
            if action == want {
                return Some((MENU_W / 2.0, y + hh / 2.0));
            }
        }
        y += hh;
    }
    let parent_y = |kind: SubKind| {
        let mut y = PAD;
        for row in &rows {
            let hh = row_h(row);
            if let Row::Item {
                submenu: Some(k), ..
            } = row
            {
                if *k == kind {
                    return y;
                }
            }
            y += hh;
        }
        PAD
    };
    for kind in [
        SubKind::Zoom,
        SubKind::ImageProc,
        SubKind::Groups,
        SubKind::Info,
    ] {
        let mut y = parent_y(kind);
        for row in build_sub_rows(state, kind) {
            let hh = row_h(&row);
            if let Row::Item {
                action: Some(action),
                ..
            } = &row
            {
                if action == want {
                    return Some((MENU_W + SUB_W / 2.0, y + hh / 2.0));
                }
            }
            y += hh;
        }
    }
    None
}

/// Pixel size of the in-process pin menu (separators are 8px, not `ROW_H`).
pub fn menu_pixel_size(state: &PinMenuState) -> (u32, u32) {
    let rows = build_rows(state);
    let h = (PAD * 2.0 + rows.iter().map(row_h).sum::<f32>())
        .ceil()
        .max(40.0) as u32;
    (MENU_W as u32, h)
}

/// Keep the menu on-screen when the pin is near the edge or tiny.
pub fn clamp_menu_pos(x: i32, y: i32, mw: i32, mh: i32, sw: i32, sh: i32) -> (i32, i32) {
    let max_x = (sw - mw).max(0) as f32;
    let max_y = (sh - mh).max(0) as f32;
    (
        clamp_f32(x as f32, 0.0, max_x).round() as i32,
        clamp_f32(y as f32, 0.0, max_y).round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_pin_menu_pos_does_not_panic() {
        let (x, y) = clamp_menu_pos(10, 10, 268, 800, 40, 20);
        assert_eq!((x, y), (0, 0));
    }

    #[test]
    fn click_through_row_is_checkable_and_checked() {
        let state = sample_state();
        let mut state = state;
        state.click_through = true;
        state.shadow = true;
        let rows = build_rows(&state);
        let ct = rows.iter().find_map(|r| match r {
            Row::Item {
                action: Some(PinMenuAction::ToggleClickThrough),
                checked,
                checkable,
                ..
            } => Some((*checked, *checkable)),
            _ => None,
        });
        assert_eq!(ct, Some((true, true)));
        let sh = rows.iter().find_map(|r| match r {
            Row::Item {
                action: Some(PinMenuAction::ToggleShadow),
                checked,
                checkable,
                ..
            } => Some((*checked, *checkable)),
            _ => None,
        });
        assert_eq!(sh, Some((true, true)));
        assert!(rows.iter().any(|r| matches!(
            r,
            Row::Item {
                action: Some(PinMenuAction::Ocr),
                ..
            }
        )));
        assert!(rows.iter().any(|r| matches!(
            r,
            Row::Item {
                action: Some(PinMenuAction::Translate),
                ..
            }
        )));
        assert!(rows.iter().any(|r| matches!(
            r,
            Row::Item {
                submenu: Some(SubKind::Zoom),
                ..
            }
        )));
        let zoom = build_sub_rows(&state, SubKind::Zoom);
        assert!(zoom.iter().any(|r| matches!(
            r,
            Row::Item {
                action: Some(PinMenuAction::ToggleSmooth),
                ..
            }
        )));
        assert!(zoom.iter().any(|r| matches!(
            r,
            Row::Item {
                label,
                ..
            } if label.starts_with("33.3")
        )));
    }

    #[test]
    fn menu_height_uses_sep_row_size() {
        let mut state = sample_state();
        state.groups = vec![
            "Default".into(),
            "Group 1".into(),
            "Group 2".into(),
            "Group 3".into(),
        ];
        let rows = build_rows(&state);
        let seps = rows.iter().filter(|r| matches!(r, Row::Sep)).count();
        assert!(seps >= 4, "expected several separators, got {seps}");
        let (_, h) = menu_pixel_size(&state);
        let naive = (PAD * 2.0 + rows.len() as f32 * ROW_H).ceil() as u32;
        assert!(
            h < naive,
            "content height {h} should be shorter than len*ROW_H {naive}"
        );
        let expected = (PAD * 2.0 + rows.iter().map(row_h).sum::<f32>())
            .ceil()
            .max(40.0) as u32;
        assert_eq!(h, expected);
        let groups = build_sub_rows(&state, SubKind::Groups);
        assert!(groups.iter().any(|r| matches!(
            r,
            Row::Item {
                action: Some(PinMenuAction::NewGroup),
                ..
            }
        )));
        assert!(groups.iter().any(|r| matches!(
            r,
            Row::Item {
                action: Some(PinMenuAction::RenameGroup),
                ..
            }
        )));
        assert!(menu_action_center(&state, &PinMenuAction::NewGroup).is_some());
        let info = build_sub_rows(&state, SubKind::Info);
        assert!(info.iter().any(|r| matches!(
            r,
            Row::Item {
                action: Some(PinMenuAction::ToggleGrayscale),
                ..
            }
        )));
    }

    fn sample_state() -> PinMenuState {
        PinMenuState {
            annotating: false,
            click_through: false,
            shadow: false,
            scale: 1.0,
            opacity: 1.0,
            group: "Default".into(),
            groups: vec!["Default".into()],
            group_counts: vec![("Default".into(), 1)],
            img_w: 100,
            img_h: 80,
            smooth: true,
            grayscale: false,
            invert: false,
            rotation: 0,
            mirror_x: false,
            mirror_y: false,
        }
    }

    #[test]
    fn prompt_accepts_keys_when_not_preedit_and_rejects_empty() {
        let mut buf = String::new();
        assert!(prompt_insert(&mut buf, false, "Work"));
        assert_eq!(buf, "Work");
        assert!(!prompt_insert(&mut buf, true, "x"));
        assert_eq!(buf, "Work");
        assert_eq!(prompt_submit_name("Work"), Some("Work".into()));
        assert_eq!(prompt_submit_name("   "), None);
        assert_eq!(prompt_submit_name(""), None);
    }
}

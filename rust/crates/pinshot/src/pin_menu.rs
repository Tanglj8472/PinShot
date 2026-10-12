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

use crate::draw::{fill_rect, present, stroke_rect};
use crate::event::PinMenuAction;
use pinshot_core::color::Rgba;

const ROW_H: f32 = 22.0;
const MENU_W: f32 = 268.0;
const PAD: f32 = 6.0;
const PROMPT_H: u32 = 96;

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

#[derive(Clone)]
enum Row {
    Sep,
    Head(String),
    Item {
        label: String,
        action: PinMenuAction,
        checked: bool,
        checkable: bool,
        indent: bool,
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
    pub img_w: u32,
    pub img_h: u32,
}

pub struct PinMenuWin {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    rows: Vec<(Rect, Row)>,
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
        let mut m = Self {
            window,
            surface,
            rows,
            cursor: Point::ZERO,
            dirty: true,
            closed: false,
            born: Instant::now(),
            prompt: None,
            group: state.group.clone(),
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
                        Key::Named(NamedKey::Escape) => self.close(),
                        Key::Named(NamedKey::Enter) => return self.submit_prompt(),
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
        for (r, row) in &self.rows {
            if r.contains(self.cursor) {
                if let Row::Item { action, .. } = row {
                    return Some(action.clone());
                }
            }
        }
        None
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
        fill_rect(
            &mut pm,
            Rect::new(0.0, 0.0, w as f32, h as f32),
            Rgba::rgb(247, 247, 247),
        );
        stroke_rect(
            &mut pm,
            Rect::new(0.5, 0.5, w as f32 - 1.0, h as f32 - 1.0),
            Rgba::rgb(154, 154, 154),
            1.0,
        );
        if let Some(p) = &self.prompt {
            let title = match p.kind {
                PromptKind::New => tr("New group"),
                PromptKind::Rename => tr("Rename group"),
            };
            crate::text::blit_into_pm(&mut pm, &title, 12.0, 10.0, 13, Rgba::rgb(20, 20, 20), true);
            crate::text::blit_into_pm(
                &mut pm,
                &tr("Name:"),
                12.0,
                36.0,
                12,
                Rgba::rgb(90, 90, 90),
                false,
            );
            fill_rect(&mut pm, Rect::new(12.0, 56.0, MENU_W - 24.0, 26.0), Rgba::WHITE);
            stroke_rect(
                &mut pm,
                Rect::new(12.0, 56.0, MENU_W - 24.0, 26.0),
                Rgba::rgb(20, 20, 20),
                1.0,
            );
            let shown = if p.buf.is_empty() {
                String::new()
            } else {
                p.buf.clone()
            };
            crate::text::blit_into_pm(
                &mut pm,
                &format!("{shown}|"),
                16.0,
                60.0,
                13,
                Rgba::rgb(20, 20, 20),
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
                        Rect::new(8.0, r.y + 3.0, MENU_W - 16.0, 1.0),
                        Rgba::rgb(200, 200, 200),
                    );
                }
                Row::Head(s) => {
                    crate::text::blit_into_pm(
                        &mut pm,
                        s,
                        r.x + 10.0,
                        r.y + 4.0,
                        11,
                        Rgba::rgb(90, 90, 90),
                        true,
                    );
                }
                Row::Item {
                    label,
                    checked,
                    checkable,
                    indent,
                    ..
                } => {
                    let hover = r.contains(self.cursor);
                    if hover {
                        fill_rect(&mut pm, *r, Rgba::rgb(196, 217, 242));
                    }
                    let mut x = if *indent { 28.0 } else { 12.0 };
                    if *checkable || *checked {
                        let boxr = Rect::new(x, r.y + 5.0, 12.0, 12.0);
                        fill_rect(
                            &mut pm,
                            boxr,
                            if *checked {
                                Rgba::rgb(40, 140, 80)
                            } else {
                                Rgba::rgb(247, 247, 247)
                            },
                        );
                        stroke_rect(&mut pm, boxr, Rgba::rgb(20, 20, 20), 1.0);
                        x += 16.0;
                    }
                    crate::text::blit_into_pm(
                        &mut pm,
                        label,
                        x,
                        r.y + 3.0,
                        12,
                        Rgba::rgb(20, 20, 20),
                        false,
                    );
                }
            }
        }
        if let Ok(mut buf) = self.surface.buffer_mut() {
            present(&mut buf, &pm);
            let _ = buf.present();
        }
        self.dirty = false;
    }
}

fn build_rows(state: &PinMenuState) -> Vec<Row> {
    let mut rows = vec![
        item(tr("Copy image"), PinMenuAction::Copy, false, false),
        item(tr("Save image as…"), PinMenuAction::Save, false, false),
        Row::Sep,
        check_item(
            tr("Show toolbar"),
            PinMenuAction::ToggleToolbar,
            state.annotating,
            false,
        ),
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
        Row::Head(tr("Zoom")),
    ];
    for pct in [25, 50, 75, 100, 150, 200, 400] {
        let on = (state.scale * 100.0 - pct as f32).abs() < 0.6;
        rows.push(check_item(
            format!("{pct}%"),
            PinMenuAction::Zoom(pct),
            on,
            true,
        ));
    }
    rows.push(Row::Head(tr("Image")));
    rows.push(item(
        tr("Rotate left"),
        PinMenuAction::Rotate(-1),
        false,
        true,
    ));
    rows.push(item(
        tr("Rotate right"),
        PinMenuAction::Rotate(1),
        false,
        true,
    ));
    rows.push(item(
        tr("Flip horizontal"),
        PinMenuAction::FlipH,
        false,
        true,
    ));
    rows.push(item(tr("Flip vertical"), PinMenuAction::FlipV, false, true));
    rows.push(Row::Sep);
    rows.push(item(tr("Paste"), PinMenuAction::Paste, false, false));
    rows.push(item(
        tr("Replace from file…"),
        PinMenuAction::ReplaceFile,
        false,
        false,
    ));
    rows.push(Row::Sep);
    rows.push(Row::Head(tr("Move to group")));
    for g in &state.groups {
        let on = g.eq_ignore_ascii_case(&state.group);
        rows.push(check_item(
            groups::label(g),
            PinMenuAction::MoveGroup(g.clone()),
            on,
            true,
        ));
    }
    rows.push(item(tr("New group…"), PinMenuAction::NewGroup, false, true));
    rows.push(item(
        tr("Rename group…"),
        PinMenuAction::RenameGroup,
        false,
        true,
    ));
    rows.push(Row::Sep);
    rows.push(item(
        tr("View in folder"),
        PinMenuAction::ViewInFolder,
        false,
        false,
    ));
    rows.push(Row::Sep);
    rows.push(item(tr("Close"), PinMenuAction::Close, false, false));
    rows.push(item(tr("Destroy"), PinMenuAction::Destroy, false, false));
    rows.push(Row::Sep);
    rows.push(Row::Head(format!("{} × {}", state.img_w, state.img_h)));
    rows.push(item(tr("Copy size"), PinMenuAction::CopySize, false, false));
    rows.push(item(
        tr("Reset zoom"),
        PinMenuAction::ResetZoom,
        false,
        false,
    ));
    let info = format!(
        "Zoom {}%  ·  Opacity {}%",
        (state.scale * 100.0).round() as i32,
        (state.opacity * 100.0).round() as i32
    );
    rows.push(Row::Head(tr(&info)));
    rows
}

fn item(label: impl Into<String>, action: PinMenuAction, checked: bool, indent: bool) -> Row {
    item_ex(label, action, checked, false, indent)
}

fn check_item(label: impl Into<String>, action: PinMenuAction, checked: bool, indent: bool) -> Row {
    item_ex(label, action, checked, true, indent)
}

fn item_ex(
    label: impl Into<String>,
    action: PinMenuAction,
    checked: bool,
    checkable: bool,
    indent: bool,
) -> Row {
    Row::Item {
        label: label.into(),
        action,
        checked,
        checkable,
        indent,
    }
}

fn row_h(row: &Row) -> f32 {
    match row {
        Row::Sep => 8.0,
        _ => ROW_H,
    }
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
        let state = PinMenuState {
            annotating: false,
            click_through: true,
            shadow: true,
            scale: 0.68,
            opacity: 1.0,
            group: "Default".into(),
            groups: vec!["Default".into()],
            img_w: 100,
            img_h: 80,
        };
        let rows = build_rows(&state);
        let ct = rows.iter().find_map(|r| match r {
            Row::Item {
                action: PinMenuAction::ToggleClickThrough,
                checked,
                checkable,
                ..
            } => Some((*checked, *checkable)),
            _ => None,
        });
        assert_eq!(ct, Some((true, true)));
        let sh = rows.iter().find_map(|r| match r {
            Row::Item {
                action: PinMenuAction::ToggleShadow,
                checked,
                checkable,
                ..
            } => Some((*checked, *checkable)),
            _ => None,
        });
        assert_eq!(sh, Some((true, true)));
        let footer = rows.iter().rev().find_map(|r| match r {
            Row::Head(s) if s.contains('%') => Some(s.clone()),
            _ => None,
        });
        assert!(footer.is_some());
    }

    #[test]
    fn menu_height_uses_sep_row_size() {
        let state = PinMenuState {
            annotating: false,
            click_through: false,
            shadow: false,
            scale: 1.0,
            opacity: 1.0,
            group: "Default".into(),
            groups: vec![
                "Default".into(),
                "Group 1".into(),
                "Group 2".into(),
                "Group 3".into(),
            ],
            img_w: 100,
            img_h: 80,
        };
        let rows = build_rows(&state);
        let seps = rows.iter().filter(|r| matches!(r, Row::Sep)).count();
        assert!(seps >= 6, "expected several separators, got {seps}");
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
        assert!(rows.iter().any(|r| matches!(
            r,
            Row::Item {
                action: PinMenuAction::NewGroup,
                ..
            }
        )));
        assert!(rows.iter().any(|r| matches!(
            r,
            Row::Item {
                action: PinMenuAction::RenameGroup,
                ..
            }
        )));
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

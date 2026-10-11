//! X11 helpers: override-redirect overlay, EWMH pin chrome, XShape click-through, XGrabKey.

use std::thread::JoinHandle;

use pinshot_core::hotkey::{parse_hotkey, Command, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
use pinshot_core::Config;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::event_loop::EventLoopProxy;
use winit::window::Window;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt, EventMask, GrabMode,
    InputFocus, KeyPressEvent, ModMask, PropMode,
};
use x11rb::protocol::Event;
use x11rb::COPY_DEPTH_FROM_PARENT;

use crate::event::AppEvent;

const SHIFT: u16 = 1;
const LOCK: u16 = 2;
const CONTROL: u16 = 4;
const MOD1: u16 = 8;
const MOD2: u16 = 16;
const MOD4: u16 = 64;
const IGNORE: [u16; 4] = [0, LOCK, MOD2, LOCK | MOD2];

pub fn xid_of(window: &Window) -> Option<u32> {
    let h = window.window_handle().ok()?;
    match h.as_raw() {
        RawWindowHandle::Xlib(w) => Some(w.window as u32),
        RawWindowHandle::Xcb(w) => Some(w.window.get()),
        _ => None,
    }
}

fn conn() -> Result<(x11rb::rust_connection::RustConnection, usize), String> {
    x11rb::connect(None).map_err(|e| e.to_string())
}

/// Unmap + round-trip so a subsequent root GetImage does not include this window.
pub fn unmap_sync(window: &Window) {
    let Some(xid) = xid_of(window) else {
        return;
    };
    let Ok((c, _)) = conn() else {
        return;
    };
    let _ = c.unmap_window(xid);
    let _ = c.get_input_focus().ok().and_then(|c| c.reply().ok());
    let _ = c.flush();
}

pub fn apply_overlay_style(window: &Window) {
    let Some(xid) = xid_of(window) else {
        return;
    };
    let Ok((c, _)) = conn() else {
        return;
    };
    let aux = ChangeWindowAttributesAux::new().override_redirect(1);
    let _ = c.change_window_attributes(xid, &aux);
    let _ = c.flush();
    let _ = window;
}

pub fn apply_pin_style(window: &Window) {
    // Managed window (never override-redirect) so the WM can give keyboard
    // focus on click. Stay above + skip the taskbar like Python.
    ewmh_state(window, true, &["_NET_WM_STATE_ABOVE", "_NET_WM_STATE_SKIP_TASKBAR", "_NET_WM_STATE_SKIP_PAGER"]);
    set_window_type(window, b"_NET_WM_WINDOW_TYPE_NORMAL");
    set_wm_input_hint(window, true);
}

pub fn raise_on_top(window: &Window) {
    ewmh_state(window, true, &["_NET_WM_STATE_ABOVE"]);
    let Some(xid) = xid_of(window) else {
        return;
    };
    let Ok((c, screen)) = conn() else {
        return;
    };
    let root = c.setup().roots[screen].root;
    let _ = c.configure_window(
        xid,
        &x11rb::protocol::xproto::ConfigureWindowAux::new().stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
    );
    if let Ok(active) = c.intern_atom(false, b"_NET_ACTIVE_WINDOW") {
        if let Ok(reply) = active.reply() {
            let ev = ClientMessageEvent::new(
                32,
                xid,
                reply.atom,
                [2, x11rb::CURRENT_TIME, 0, 0, 0],
            );
            let _ = c.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                ev,
            );
        }
    }
    let _ = c.flush();
}

fn set_window_type(window: &Window, typ: &[u8]) {
    let Some(xid) = xid_of(window) else {
        return;
    };
    let Ok((c, _)) = conn() else {
        return;
    };
    let Ok(ty) = c.intern_atom(false, b"_NET_WM_WINDOW_TYPE") else {
        return;
    };
    let Ok(ty) = ty.reply() else {
        return;
    };
    let Ok(val) = c.intern_atom(false, typ) else {
        return;
    };
    let Ok(val) = val.reply() else {
        return;
    };
    let bytes = val.atom.to_ne_bytes();
    let _ = c.change_property(
        PropMode::REPLACE,
        xid,
        ty.atom,
        AtomEnum::ATOM,
        32,
        1,
        &bytes,
    );
    let _ = c.flush();
}

fn set_wm_input_hint(window: &Window, input: bool) {
    let Some(xid) = xid_of(window) else {
        return;
    };
    let Ok((c, _)) = conn() else {
        return;
    };
    let Ok(atom) = c.intern_atom(false, b"WM_HINTS") else {
        return;
    };
    let Ok(atom) = atom.reply() else {
        return;
    };
    // ICCCM XWMHints: flags (InputHint|StateHint), input, initial_state=Normal
    let flags: u32 = 1 | 2;
    let input_u: u32 = if input { 1 } else { 0 };
    let mut bytes = Vec::with_capacity(36);
    for v in [flags, input_u, 1, 0, 0, 0, 0, 0, 0] {
        bytes.extend_from_slice(&v.to_ne_bytes());
    }
    let _ = c.change_property(
        PropMode::REPLACE,
        xid,
        atom.atom,
        atom.atom,
        32,
        9,
        &bytes,
    );
    let _ = c.flush();
}

fn ewmh_state(window: &Window, add: bool, atoms: &[&str]) {
    let Some(xid) = xid_of(window) else {
        return;
    };
    let Ok((c, screen)) = conn() else {
        return;
    };
    let root = c.setup().roots[screen].root;
    let Ok(state) = c.intern_atom(false, b"_NET_WM_STATE") else {
        return;
    };
    let Ok(state) = state.reply() else {
        return;
    };
    // _NET_WM_STATE carries two atoms per client message; send the rest in extra packets.
    for chunk in atoms.chunks(2) {
        let mut ids = [0u32; 2];
        for (i, name) in chunk.iter().enumerate() {
            if let Ok(a) = c.intern_atom(false, name.as_bytes()) {
                if let Ok(r) = a.reply() {
                    ids[i] = r.atom;
                }
            }
        }
        let ev = ClientMessageEvent::new(
            32,
            xid,
            state.atom,
            [if add { 1 } else { 0 }, ids[0], ids[1], 2, 0],
        );
        let _ = c.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
    }
    let _ = c.flush();
}

pub fn set_opacity(window: &Window, opacity: f32) {
    let Some(xid) = xid_of(window) else {
        return;
    };
    let Ok((c, _)) = conn() else {
        return;
    };
    let Ok(atom) = c.intern_atom(false, b"_NET_WM_WINDOW_OPACITY") else {
        return;
    };
    let Ok(atom) = atom.reply() else {
        return;
    };
    let v = ((opacity.clamp(0.1, 1.0) as f64) * 0xffff_ffff_u32 as f64).round() as u32;
    let bytes = v.to_ne_bytes();
    let _ = c.change_property(
        PropMode::REPLACE,
        xid,
        atom.atom,
        AtomEnum::CARDINAL,
        32,
        1,
        &bytes,
    );
    let _ = c.flush();
}

pub fn focus_window(window: &Window) {
    let Some(xid) = xid_of(window) else {
        window.focus_window();
        return;
    };
    let Ok((c, _)) = conn() else {
        window.focus_window();
        return;
    };
    let _ = c.map_window(xid);
    let _ = c.set_input_focus(InputFocus::PARENT, xid, x11rb::CURRENT_TIME);
    let _ = c.configure_window(
        xid,
        &x11rb::protocol::xproto::ConfigureWindowAux::new()
            .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
    );
    let _ = c.flush();
    window.focus_window();
}

/// Active keyboard grab on the overlay (same idea as Qt `grabKeyboard`).
/// Events are forwarded through `proxy` because a second X client cannot
/// deliver KeyPress to winit's connection.
///
/// `owner_events=false` so the previously-focused client (Chrome) does not
/// also receive keys. Pointer grab uses `owner_events=true` so winit still
/// sees mouse events on the overlay window.
pub struct OverlayKeyGrab {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pause_keys: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl OverlayKeyGrab {
    pub fn start(xid: u32, proxy: winit::event_loop::EventLoopProxy<crate::event::AppEvent>) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pause_keys = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let pause = pause_keys.clone();
        let _ = std::thread::Builder::new()
            .name("pinshot-overlay-grab".into())
            .spawn(move || overlay_grab_loop(xid, proxy, flag, pause));
        Self { stop, pause_keys }
    }

    /// Ungrab the keyboard while a text editor is open so XIM / winit IME can work.
    pub fn set_pause_keys(&self, pause: bool) {
        self.pause_keys
            .store(pause, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn kick(&self) {
        self.pause_keys
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Drop for OverlayKeyGrab {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn overlay_grab_loop(
    xid: u32,
    proxy: winit::event_loop::EventLoopProxy<crate::event::AppEvent>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pause_keys: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let Ok((c, _)) = conn() else {
        return;
    };
    let mut key_grabbed = false;
    let mut ptr_grabbed = false;
    let mut last_try = std::time::Instant::now() - std::time::Duration::from_secs(1);
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        let want_keys = !pause_keys.load(std::sync::atomic::Ordering::Relaxed);
        let now = std::time::Instant::now();
        if now.duration_since(last_try) >= std::time::Duration::from_millis(80) {
            last_try = now;
            let _ = c.map_window(xid);
            let _ = c.set_input_focus(InputFocus::PARENT, xid, x11rb::CURRENT_TIME);
            if want_keys && !key_grabbed {
                match c.grab_keyboard(false, xid, x11rb::CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC)
                {
                    Ok(cookie) => match cookie.reply() {
                        Ok(r) if r.status == x11rb::protocol::xproto::GrabStatus::SUCCESS => {
                            key_grabbed = true;
                            crate::logutil::log("XGrabKeyboard ok");
                        }
                        Ok(r) => {
                            crate::logutil::log(&format!("XGrabKeyboard status={:?}", r.status));
                        }
                        Err(e) => crate::logutil::log(&format!("XGrabKeyboard: {e}")),
                    },
                    Err(e) => crate::logutil::log(&format!("XGrabKeyboard: {e}")),
                }
            } else if !want_keys && key_grabbed {
                let _ = c.ungrab_keyboard(x11rb::CURRENT_TIME);
                key_grabbed = false;
                crate::logutil::log("XUngrabKeyboard (text/IME)");
            }
            if !ptr_grabbed {
                let mask = EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::POINTER_MOTION;
                match c.grab_pointer(
                    true,
                    xid,
                    mask,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                    x11rb::NONE,
                    x11rb::NONE,
                    x11rb::CURRENT_TIME,
                ) {
                    Ok(cookie) => {
                        if let Ok(r) = cookie.reply() {
                            if r.status == x11rb::protocol::xproto::GrabStatus::SUCCESS {
                                ptr_grabbed = true;
                                crate::logutil::log("XGrabPointer ok");
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
            let _ = c.flush();
        }
        match c.poll_for_event() {
            Ok(Some(Event::KeyPress(ev))) => {
                let keysym = keycode_to_keysym(&c, ev.detail, u16::from(ev.state));
                let _ = proxy.send_event(crate::event::AppEvent::X11Key {
                    keysym,
                    mods: u16::from(ev.state),
                    pressed: true,
                });
            }
            Ok(Some(Event::KeyRelease(ev))) => {
                let keysym = keycode_to_keysym(&c, ev.detail, u16::from(ev.state));
                let _ = proxy.send_event(crate::event::AppEvent::X11Key {
                    keysym,
                    mods: u16::from(ev.state),
                    pressed: false,
                });
            }
            Ok(Some(_)) => {}
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(8)),
            Err(_) => break,
        }
    }
    let _ = c.ungrab_keyboard(x11rb::CURRENT_TIME);
    let _ = c.ungrab_pointer(x11rb::CURRENT_TIME);
    let _ = c.flush();
}

/// Overlay-local pointer position and Button1 state (recover a drag whose
/// ButtonPress landed on the window underneath before the overlay mapped).
pub fn pointer_in_window(xid: u32) -> Option<(i32, i32, bool)> {
    let (c, _) = conn().ok()?;
    let q = c.query_pointer(xid).ok()?.reply().ok()?;
    let btn1 = u16::from(q.mask) & 0x0100 != 0;
    Some((i32::from(q.win_x), i32::from(q.win_y), btn1))
}

fn keycode_to_keysym(c: &x11rb::rust_connection::RustConnection, code: u8, state: u16) -> u32 {
    let setup = c.setup();
    let min = setup.min_keycode;
    if code < min {
        return 0;
    }
    let count = setup.max_keycode.saturating_sub(min).saturating_add(1);
    let Ok(cookie) = c.get_keyboard_mapping(min, count) else {
        return 0;
    };
    let Ok(map) = cookie.reply() else {
        return 0;
    };
    let stride = map.keysyms_per_keycode as usize;
    let idx = (code - min) as usize * stride;
    if idx >= map.keysyms.len() {
        return 0;
    }
    let shift = state & SHIFT != 0;
    let slot = if shift && stride > 1 { idx + 1 } else { idx };
    map.keysyms.get(slot).copied().filter(|k| *k != 0).unwrap_or(map.keysyms[idx])
}

pub fn set_click_through(window: &Window, on: bool, _opacity: f32) {
    let Some(xid) = xid_of(window) else {
        return;
    };
    let Ok((c, _)) = conn() else {
        return;
    };
    use x11rb::protocol::shape::{self, ConnectionExt as _};
    if c.extension_information(shape::X11_EXTENSION_NAME)
        .ok()
        .flatten()
        .is_none()
    {
        return;
    }
    if on {
        let _ = c.shape_rectangles(
            shape::SO::SET,
            shape::SK::INPUT,
            x11rb::protocol::xproto::ClipOrdering::UNSORTED,
            xid,
            0,
            0,
            &[],
        );
    } else {
        let geom = c.get_geometry(xid).ok().and_then(|c| c.reply().ok());
        if let Some(g) = geom {
            let rect = x11rb::protocol::xproto::Rectangle {
                x: 0,
                y: 0,
                width: g.width.max(1),
                height: g.height.max(1),
            };
            let _ = c.shape_rectangles(
                shape::SO::SET,
                shape::SK::INPUT,
                x11rb::protocol::xproto::ClipOrdering::UNSORTED,
                xid,
                0,
                0,
                &[rect],
            );
        }
    }
    let _ = c.flush();
}

/// Grab pointer+keyboard on the root until the overlay maps (Python `InputHold`).
pub struct InputHold {
    conn: Option<x11rb::rust_connection::RustConnection>,
}

impl InputHold {
    pub fn grab() -> Self {
        let Ok((c, screen)) = conn() else {
            return Self { conn: None };
        };
        let root = c.setup().roots[screen].root;
        let mask = EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION;
        let _ = c.grab_pointer(
            false,
            root,
            mask,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            x11rb::NONE,
            x11rb::NONE,
            x11rb::CURRENT_TIME,
        );
        let _ = c.grab_keyboard(false, root, x11rb::CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC);
        let _ = c.flush();
        Self { conn: Some(c) }
    }

    pub fn release(&mut self) {
        if let Some(c) = self.conn.take() {
            let _ = c.ungrab_pointer(x11rb::CURRENT_TIME);
            let _ = c.ungrab_keyboard(x11rb::CURRENT_TIME);
            let _ = c.flush();
        }
    }
}

impl Drop for InputHold {
    fn drop(&mut self) {
        self.release();
    }
}

fn win_mods_to_x(mods: u32) -> u16 {
    let mut x = 0u16;
    if mods & MOD_SHIFT != 0 {
        x |= SHIFT;
    }
    if mods & MOD_CONTROL != 0 {
        x |= CONTROL;
    }
    if mods & MOD_ALT != 0 {
        x |= MOD1;
    }
    if mods & MOD_WIN != 0 {
        x |= MOD4;
    }
    x
}

fn vk_to_keysym(vk: u32) -> Option<u32> {
    Some(match vk {
        0x70..=0x87 => 0xffbe + (vk - 0x70), // F1..F24
        0x20 => 0x0020,
        0x1B => 0xff1b,
        0x09 => 0xff09,
        0x0D => 0xff0d,
        0x08 => 0xff08,
        0x2E => 0xffff,
        0x21 => 0xff55,
        0x22 => 0xff56,
        0x23 => 0xff57,
        0x24 => 0xff50,
        0x25 => 0xff51,
        0x26 => 0xff52,
        0x27 => 0xff53,
        0x28 => 0xff54,
        b if (b'A'..=b'Z').contains(&(b as u8)) => (b as u8 as char).to_ascii_lowercase() as u32,
        b if (b'0'..=b'9').contains(&(b as u8)) => b,
        _ => return None,
    })
}

fn keysym_to_keycode(c: &x11rb::rust_connection::RustConnection, keysym: u32) -> Option<u8> {
    let setup = c.setup();
    let min = setup.min_keycode;
    let count = setup.max_keycode.saturating_sub(min).saturating_add(1);
    let map = c.get_keyboard_mapping(min, count).ok()?.reply().ok()?;
    let stride = map.keysyms_per_keycode as usize;
    for (i, chunk) in map.keysyms.chunks(stride).enumerate() {
        if chunk.iter().take(2).any(|ks| *ks == keysym) {
            return Some(min.saturating_add(i as u8));
        }
    }
    None
}

pub struct HotkeyHost {
    _thread: JoinHandle<()>,
}

impl HotkeyHost {
    pub fn install(proxy: EventLoopProxy<AppEvent>, cfg: &Config) -> Result<Self, String> {
        let specs: Vec<(String, Command)> = vec![
            (cfg.hotkey_snip.clone(), Command::Snip),
            (cfg.hotkey_pin.clone(), Command::PinClipboard),
            (cfg.hotkey_hide_all.clone(), Command::HideAllPins),
            (cfg.hotkey_restore.clone(), Command::RestoreClosed),
            (cfg.hotkey_selection_translate.clone(), Command::SelectionTranslate),
            (cfg.hotkey_capture_translate.clone(), Command::CaptureTranslate),
        ]
        .into_iter()
        .chain(
            (!cfg.hotkey_clickthrough.trim().is_empty())
                .then(|| (cfg.hotkey_clickthrough.clone(), Command::ClickThrough)),
        )
        .collect();

        let thread = std::thread::Builder::new()
            .name("pinshot-x11-hotkeys".into())
            .spawn(move || {
                if let Err(e) = hotkey_loop(proxy, specs) {
                    crate::logutil::log(&format!("XGrabKey: {e}"));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { _thread: thread })
    }
}

fn hotkey_loop(proxy: EventLoopProxy<AppEvent>, specs: Vec<(String, Command)>) -> Result<(), String> {
    let (c, screen) = conn()?;
    let root = c.setup().roots[screen].root;
    let mut binds: Vec<(u16, u8, Command)> = Vec::new();
    for (seq, cmd) in specs {
        let Some(hk) = parse_hotkey(&seq) else {
            continue;
        };
        let Some(ks) = vk_to_keysym(hk.vk) else {
            continue;
        };
        let Some(code) = keysym_to_keycode(&c, ks) else {
            crate::logutil::log(&format!("no keycode for {seq}"));
            continue;
        };
        let mods = win_mods_to_x(hk.mods);
        for ig in IGNORE {
            let _ = c.grab_key(
                true,
                root,
                ModMask::from(mods | ig),
                code,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            );
        }
        binds.push((mods, code, cmd));
    }
    let _ = c.flush();
    loop {
        let ev = c.wait_for_event().map_err(|e| e.to_string())?;
        if let Event::KeyPress(KeyPressEvent { detail, state, .. }) = ev {
            let mods = state & !(LOCK | MOD2);
            let mods_u = u16::from(mods);
            if let Some((_, _, cmd)) = binds.iter().find(|(m, code, _)| *m == mods_u && *code == detail) {
                let _ = proxy.send_event(AppEvent::Hotkey(*cmd));
            }
        }
    }
}

#[allow(dead_code)]
fn _keep_atom(a: AtomEnum) {
    let _ = COPY_DEPTH_FROM_PARENT;
    let _ = a;
}

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

/// True when an X compositor owns `_NET_WM_CM_Sn` (Python `_composited`).
pub fn compositor_active() -> bool {
    let Ok((c, screen)) = conn() else {
        return false;
    };
    let name = format!("_NET_WM_CM_S{screen}");
    let Ok(atom) = c.intern_atom(false, name.as_bytes()) else {
        return false;
    };
    let Ok(atom) = atom.reply() else {
        return false;
    };
    let Ok(owner) = c.get_selection_owner(atom.atom) else {
        return false;
    };
    owner.reply().map(|r| r.owner != 0).unwrap_or(false)
}

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
    ewmh_state(
        window,
        true,
        &[
            "_NET_WM_STATE_ABOVE",
            "_NET_WM_STATE_SKIP_TASKBAR",
            "_NET_WM_STATE_SKIP_PAGER",
        ],
    );
    set_window_type(window, b"_NET_WM_WINDOW_TYPE_NORMAL");
    set_wm_input_hint(window, true);
}

fn intern(c: &x11rb::rust_connection::RustConnection, name: &[u8]) -> Option<u32> {
    c.intern_atom(false, name)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom)
}

fn send_client(
    c: &x11rb::rust_connection::RustConnection,
    root: u32,
    xid: u32,
    atom: u32,
    data: [u32; 5],
) {
    let ev = ClientMessageEvent::new(32, xid, atom, data);
    let _ = c.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        ev,
    );
}

/// Python `x11raise.raise_on_top`: lower `lower` via `_NET_RESTACK_WINDOW`
/// Below, then ABOVE + restack Above + `_NET_ACTIVE_WINDOW` on `xid`.
pub fn raise_xid_above(xid: u32, lower: &[u32]) {
    let Ok((c, screen)) = conn() else {
        return;
    };
    let root = c.setup().roots[screen].root;
    let restack = intern(&c, b"_NET_RESTACK_WINDOW");
    let state = intern(&c, b"_NET_WM_STATE");
    let above = intern(&c, b"_NET_WM_STATE_ABOVE");
    let active = intern(&c, b"_NET_ACTIVE_WINDOW");
    for &low in lower {
        if low == 0 || low == xid {
            continue;
        }
        if let Some(rs) = restack {
            // X.Below = 1
            send_client(&c, root, low, rs, [2, 0, 1, 0, 0]);
        }
        let _ = c.configure_window(
            low,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .stack_mode(x11rb::protocol::xproto::StackMode::BELOW),
        );
    }
    if let (Some(st), Some(ab)) = (state, above) {
        send_client(&c, root, xid, st, [1, ab, 0, 2, 0]);
    }
    if let Some(rs) = restack {
        // X.Above = 0
        send_client(&c, root, xid, rs, [2, 0, 0, 0, 0]);
    }
    if let Some(act) = active {
        send_client(&c, root, xid, act, [2, x11rb::CURRENT_TIME, 0, 0, 0]);
    }
    let _ = c.configure_window(
        xid,
        &x11rb::protocol::xproto::ConfigureWindowAux::new()
            .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
    );
    let _ = c.flush();
}

fn window_title(c: &x11rb::rust_connection::RustConnection, wid: u32) -> String {
    let name = intern(c, b"_NET_WM_NAME");
    let utf8 = intern(c, b"UTF8_STRING");
    if let (Some(na), Some(ut)) = (name, utf8) {
        if let Ok(p) = c.get_property(false, wid, na, ut, 0, 256) {
            if let Ok(r) = p.reply() {
                if !r.value.is_empty() {
                    return String::from_utf8_lossy(&r.value).into_owned();
                }
            }
        }
        if let Ok(p) = c.get_property(false, wid, na, AtomEnum::STRING, 0, 256) {
            if let Ok(r) = p.reply() {
                if !r.value.is_empty() {
                    return String::from_utf8_lossy(&r.value).into_owned();
                }
            }
        }
    }
    if let Ok(p) = c.get_property(false, wid, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256) {
        if let Ok(r) = p.reply() {
            return String::from_utf8_lossy(&r.value).into_owned();
        }
    }
    String::new()
}

fn client_list(c: &x11rb::rust_connection::RustConnection, root: u32) -> Vec<u32> {
    for atom_name in [b"_NET_CLIENT_LIST_STACKING".as_slice(), b"_NET_CLIENT_LIST".as_slice()] {
        if let Some(atom) = intern(c, atom_name) {
            if let Ok(p) = c.get_property(false, root, atom, AtomEnum::WINDOW, 0, 4096) {
                if let Ok(r) = p.reply() {
                    if !r.value.is_empty() {
                        return r
                            .value
                            .chunks_exact(4)
                            .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
                            .collect();
                    }
                }
            }
        }
    }
    Vec::new()
}

pub fn titled_top_levels() -> Vec<(u32, String)> {
    let Ok((c, screen)) = conn() else {
        return Vec::new();
    };
    let root = c.setup().roots[screen].root;
    client_list(&c, root)
        .into_iter()
        .filter_map(|wid| {
            let title = window_title(&c, wid);
            if title.is_empty() {
                None
            } else {
                Some((wid, title))
            }
        })
        .collect()
}

/// Push "PinShot Pin" windows below the OCR result (Python `windows_to_lower`).
pub fn raise_ocr_above_pins() {
    let wins = titled_top_levels();
    let pins: Vec<u32> = wins
        .iter()
        .filter(|(_, t)| t == "PinShot Pin")
        .map(|(id, _)| *id)
        .collect();
    let Some(&(ocr, _)) = wins
        .iter()
        .find(|(_, t)| t == "PinShot OCR" || t.contains("PinShot OCR"))
    else {
        return;
    };
    crate::logutil::log(&format!("ocr raise above {} pins xid={ocr:#x}", pins.len()));
    raise_xid_above(ocr, &pins);
}

pub fn raise_on_top(window: &Window) {
    if let Some(xid) = xid_of(window) {
        raise_xid_above(xid, &[]);
        return;
    }
    ewmh_state(window, true, &["_NET_WM_STATE_ABOVE"]);
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
    let _ = c.change_property(PropMode::REPLACE, xid, atom.atom, atom.atom, 32, 9, &bytes);
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
    let Ok((c, screen)) = conn() else {
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
    let write = |wid: u32| {
        let _ = c.change_property(
            PropMode::REPLACE,
            wid,
            atom.atom,
            AtomEnum::CARDINAL,
            32,
            1,
            &bytes,
        );
    };
    write(xid);
    // picom / xfwm4 often read the property on the WM frame, not the client.
    let root = c.setup().roots.get(screen).map(|s| s.root);
    let mut frame = xid;
    if let Ok(cookie) = c.query_tree(xid) {
        if let Ok(tree) = cookie.reply() {
            if root != Some(tree.parent) && tree.parent != 0 && tree.parent != xid {
                write(tree.parent);
                frame = tree.parent;
            }
        }
    }
    let _ = c.flush();
    crate::logutil::log(&format!(
        "opacity set {:.2} client={xid:#x} frame={frame:#x}",
        opacity.clamp(0.1, 1.0)
    ));
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
    join: Option<std::thread::JoinHandle<()>>,
}

impl OverlayKeyGrab {
    pub fn start(
        xid: u32,
        proxy: winit::event_loop::EventLoopProxy<crate::event::AppEvent>,
    ) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pause_keys = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let pause = pause_keys.clone();
        let join = std::thread::Builder::new()
            .name("pinshot-overlay-grab".into())
            .spawn(move || overlay_grab_loop(xid, proxy, flag, pause))
            .ok();
        Self {
            stop,
            pause_keys,
            join,
        }
    }

    /// Ungrab the keyboard while XIM / winit IME is composing (not Latin text).
    pub fn set_pause_keys(&self, pause: bool) {
        self.pause_keys
            .store(pause, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn kick(&self) {
        self.pause_keys
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    /// Only the grabbing connection can ungrab. Join so Esc→F1 is not eaten.
    pub fn release(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.join.take() {
            let _ = h.join();
        }
    }
}

impl Drop for OverlayKeyGrab {
    fn drop(&mut self) {
        self.release();
    }
}

/// Best-effort ungrab if a previous overlay/OCR helper left the pointer or
/// keyboard grabbed. Only the grabbing client can truly ungrab; this still
/// helps when that client has already disconnected.
pub fn release_stray_grabs() {
    let Ok((c, screen)) = conn() else {
        return;
    };
    let _ = screen;
    let _ = c.ungrab_keyboard(x11rb::CURRENT_TIME);
    let _ = c.ungrab_pointer(x11rb::CURRENT_TIME);
    let _ = c.flush();
}

struct KeyMap {
    min: u8,
    stride: usize,
    keysyms: Vec<u32>,
}

fn load_keymap(c: &x11rb::rust_connection::RustConnection) -> Option<KeyMap> {
    let setup = c.setup();
    let min = setup.min_keycode;
    let count = setup.max_keycode.saturating_sub(min).saturating_add(1);
    let map = c.get_keyboard_mapping(min, count).ok()?.reply().ok()?;
    Some(KeyMap {
        min,
        stride: map.keysyms_per_keycode as usize,
        keysyms: map.keysyms,
    })
}

fn keysym_from_map(map: &KeyMap, code: u8, state: u16) -> u32 {
    if code < map.min || map.stride == 0 {
        return 0;
    }
    let idx = (code - map.min) as usize * map.stride;
    if idx >= map.keysyms.len() {
        return 0;
    }
    let shift = state & SHIFT != 0;
    let slot = if shift && map.stride > 1 {
        idx + 1
    } else {
        idx
    };
    map.keysyms
        .get(slot)
        .copied()
        .filter(|k| *k != 0)
        .unwrap_or(map.keysyms[idx])
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
    let mut map = load_keymap(&c);
    let mut key_grabbed = false;
    let mut last_try = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let root = c.setup().roots.first().map(|s| s.root).unwrap_or(xid);
    grab_swallow_keys(&c, root);
    loop {
        let stopping = stop.load(std::sync::atomic::Ordering::Relaxed);
        let want_keys = !stopping && !pause_keys.load(std::sync::atomic::Ordering::Relaxed);
        if want_keys && !key_grabbed {
            let now = std::time::Instant::now();
            if now.duration_since(last_try) >= std::time::Duration::from_millis(1) {
                last_try = now;
                let _ = c.map_window(xid);
                let _ = c.set_input_focus(InputFocus::PARENT, xid, x11rb::CURRENT_TIME);
                match c.grab_keyboard(
                    false,
                    xid,
                    x11rb::CURRENT_TIME,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                ) {
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
                let _ = c.flush();
            }
        } else if !want_keys && key_grabbed {
            let _ = c.ungrab_keyboard(x11rb::CURRENT_TIME);
            let _ = c.flush();
            key_grabbed = false;
            crate::logutil::log(if stopping {
                "XUngrabKeyboard (overlay close)"
            } else {
                "XUngrabKeyboard (text/IME)"
            });
        }
        match c.poll_for_event() {
            Ok(Some(Event::KeyPress(ev))) => {
                let keysym = map
                    .as_ref()
                    .map(|m| keysym_from_map(m, ev.detail, u16::from(ev.state)))
                    .unwrap_or(0);
                if stopping && keysym == 0xffbe {
                    let _ = proxy.send_event(crate::event::AppEvent::Hotkey(
                        pinshot_core::hotkey::Command::Snip,
                    ));
                } else {
                    let _ = proxy.send_event(crate::event::AppEvent::X11Key {
                        keysym,
                        mods: u16::from(ev.state),
                        pressed: true,
                    });
                }
            }
            Ok(Some(Event::KeyRelease(ev))) => {
                if !stopping {
                    let keysym = map
                        .as_ref()
                        .map(|m| keysym_from_map(m, ev.detail, u16::from(ev.state)))
                        .unwrap_or(0);
                    let _ = proxy.send_event(crate::event::AppEvent::X11Key {
                        keysym,
                        mods: u16::from(ev.state),
                        pressed: false,
                    });
                }
            }
            Ok(Some(Event::MappingNotify(_))) => {
                map = load_keymap(&c);
            }
            Ok(Some(_)) => {}
            Ok(None) => {
                if stopping {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(_) => break,
        }
        if stopping {
            // Drain anything already queued, then ungrab so the next F1 reaches HotkeyHost.
            while let Ok(Some(ev)) = c.poll_for_event() {
                if let Event::KeyPress(kev) = ev {
                    let keysym = map
                        .as_ref()
                        .map(|m| keysym_from_map(m, kev.detail, u16::from(kev.state)))
                        .unwrap_or(0);
                    if keysym == 0xffbe {
                        let _ = proxy.send_event(crate::event::AppEvent::Hotkey(
                            pinshot_core::hotkey::Command::Snip,
                        ));
                    }
                }
            }
            break;
        }
    }
    if key_grabbed {
        let _ = c.ungrab_keyboard(x11rb::CURRENT_TIME);
    }
    ungrab_swallow_keys(&c, root);
    let _ = c.flush();
}

/// Passive grab for Tab / ISO_Left_Tab so Chrome cannot see them even when
/// `XGrabKeyboard` is still racing InputHold ungrab.
fn grab_swallow_keys(c: &x11rb::rust_connection::RustConnection, root: u32) {
    for ks in [0xff09u32, 0xfe20] {
        let Some(code) = keysym_to_keycode(c, ks) else {
            continue;
        };
        for ig in IGNORE {
            let _ = c.grab_key(
                false,
                root,
                ModMask::from(ig),
                code,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            );
        }
    }
    let _ = c.flush();
}

fn ungrab_swallow_keys(c: &x11rb::rust_connection::RustConnection, root: u32) {
    for ks in [0xff09u32, 0xfe20] {
        let Some(code) = keysym_to_keycode(c, ks) else {
            continue;
        };
        for ig in IGNORE {
            let _ = c.ungrab_key(code, root, ModMask::from(ig));
        }
    }
}

/// Root + window-relative pointer and Button1.
#[derive(Clone, Copy, Debug)]
pub struct PointerState {
    pub root_x: i32,
    pub root_y: i32,
    pub win_x: i32,
    pub win_y: i32,
    pub btn1: bool,
    pub ctrl: bool,
}

pub fn pointer_state(xid: u32) -> Option<PointerState> {
    let (c, screen) = conn().ok()?;
    let root = c.setup().roots.get(screen).map(|s| s.root).unwrap_or(xid);
    let q = c
        .query_pointer(xid)
        .ok()
        .and_then(|c| c.reply().ok())
        .or_else(|| c.query_pointer(root).ok().and_then(|c| c.reply().ok()))?;
    let mask = u16::from(q.mask);
    let btn1 = mask & 0x0100 != 0;
    let ctrl = mask & CONTROL != 0;
    Some(PointerState {
        root_x: i32::from(q.root_x),
        root_y: i32::from(q.root_y),
        win_x: i32::from(q.win_x),
        win_y: i32::from(q.win_y),
        btn1,
        ctrl,
    })
}

/// Control from the X pointer mask (works when the pin is unfocused).
pub fn pointer_ctrl_down() -> bool {
    let Ok((c, screen)) = conn() else {
        return false;
    };
    let Some(root) = c.setup().roots.get(screen).map(|s| s.root) else {
        return false;
    };
    pointer_state(root).map(|p| p.ctrl).unwrap_or(false)
}

/// Root-relative pointer and Button1. Overlay recover uses the virtual origin.
pub fn pointer_in_window(xid: u32) -> Option<(i32, i32, bool)> {
    let p = pointer_state(xid)?;
    Some((p.root_x, p.root_y, p.btn1))
}

/// Window-relative pointer (pin zoom anchor).
pub fn pointer_in_client(xid: u32) -> Option<(i32, i32, bool)> {
    let p = pointer_state(xid)?;
    Some((p.win_x, p.win_y, p.btn1))
}

/// Atomic x/y/w/h so xfwm4 cannot resize about the center before the move.
/// Managed pins go through `_NET_MOVERESIZE_WINDOW` (the WM owns ConfigureRequest).
pub fn configure_geom(xid: u32, x: i32, y: i32, w: u32, h: u32) {
    let Ok((c, screen)) = conn() else {
        return;
    };
    let w = w.max(1);
    let h = h.max(1);
    let root = c.setup().roots.get(screen).map(|s| s.root).unwrap_or(xid);
    if let Ok(atom) = c.intern_atom(false, b"_NET_MOVERESIZE_WINDOW") {
        if let Ok(atom) = atom.reply() {
            // StaticGravity | x,y,w,h present | source=pager
            let info: u32 = 10 | (1 << 8) | (1 << 9) | (1 << 10) | (1 << 11) | (2 << 12);
            let ev = ClientMessageEvent::new(32, xid, atom.atom, [info, x as u32, y as u32, w, h]);
            let _ = c.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                ev,
            );
        }
    }
    let aux = x11rb::protocol::xproto::ConfigureWindowAux::new()
        .x(x)
        .y(y)
        .width(w)
        .height(h);
    let _ = c.configure_window(xid, &aux);
    let _ = c.flush();
}

#[cfg(test)]
mod keymap_tests {
    use super::{keysym_from_map, KeyMap, SHIFT};

    #[test]
    fn cached_map_shift_slot() {
        let map = KeyMap {
            min: 8,
            stride: 2,
            keysyms: vec![0, 0, b'a' as u32, b'A' as u32],
        };
        assert_eq!(keysym_from_map(&map, 9, 0), b'a' as u32);
        assert_eq!(keysym_from_map(&map, 9, SHIFT), b'A' as u32);
    }
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
        let _ = c.grab_keyboard(
            false,
            root,
            x11rb::CURRENT_TIME,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        );
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
            (
                cfg.hotkey_selection_translate.clone(),
                Command::SelectionTranslate,
            ),
            (
                cfg.hotkey_capture_translate.clone(),
                Command::CaptureTranslate,
            ),
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

fn hotkey_loop(
    proxy: EventLoopProxy<AppEvent>,
    specs: Vec<(String, Command)>,
) -> Result<(), String> {
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
            if let Some((_, _, cmd)) = binds
                .iter()
                .find(|(m, code, _)| *m == mods_u && *code == detail)
            {
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

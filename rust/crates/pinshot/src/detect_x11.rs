//! X11 EWMH window list + child walk. AT-SPI is probed in a subprocess (Python
//! `winlist.atspi_available`) and skipped when the a11y bus is missing.

use pinshot_core::geom::Rect;
use std::sync::atomic::{AtomicU8, Ordering};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, QueryTreeReply};

static LAST_SOURCE: AtomicU8 = AtomicU8::new(0);

pub fn window_rects() -> Vec<(Rect, String)> {
    let Ok((c, screen)) = x11rb::connect(None) else {
        return Vec::new();
    };
    let root = c.setup().roots[screen].root;
    let Ok(stacking) = c.intern_atom(false, b"_NET_CLIENT_LIST_STACKING") else {
        return Vec::new();
    };
    let Ok(stacking) = stacking.reply() else {
        return Vec::new();
    };
    let Ok(list) = c.intern_atom(false, b"_NET_CLIENT_LIST") else {
        return Vec::new();
    };
    let Ok(list) = list.reply() else {
        return Vec::new();
    };
    let prop = c
        .get_property(false, root, stacking.atom, AtomEnum::WINDOW, 0, 4096)
        .ok()
        .and_then(|c| c.reply().ok())
        .filter(|p| !p.value.is_empty())
        .or_else(|| {
            c.get_property(false, root, list.atom, AtomEnum::WINDOW, 0, 4096)
                .ok()
                .and_then(|c| c.reply().ok())
        });
    let Some(prop) = prop else {
        return Vec::new();
    };
    let wids = parse_windows(&prop.value);
    let hidden = c
        .intern_atom(false, b"_NET_WM_STATE_HIDDEN")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom);
    let state_atom = c
        .intern_atom(false, b"_NET_WM_STATE")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom);
    let name_atom = c
        .intern_atom(false, b"_NET_WM_NAME")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom);
    let extents = c
        .intern_atom(false, b"_NET_FRAME_EXTENTS")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom);
    let pid_atom = c
        .intern_atom(false, b"_NET_WM_PID")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom);
    let self_pid = std::process::id();
    let mut out = Vec::new();
    for wid in wids.into_iter().rev() {
        if let Some(pid_atom) = pid_atom {
            if let Ok(p) = c.get_property(false, wid, pid_atom, AtomEnum::CARDINAL, 0, 1) {
                if let Ok(r) = p.reply() {
                    if let Some(pid) = r.value32().and_then(|mut it| it.next()) {
                        if pid == self_pid {
                            continue;
                        }
                    }
                }
            }
        }
        if let (Some(st), Some(hid)) = (state_atom, hidden) {
            if let Ok(p) = c.get_property(false, wid, st, AtomEnum::ATOM, 0, 32) {
                if let Ok(r) = p.reply() {
                    if r.value32().into_iter().flatten().any(|a| a == hid) {
                        continue;
                    }
                }
            }
        }
        let Ok(g) = c.get_geometry(wid) else {
            continue;
        };
        let Ok(g) = g.reply() else {
            continue;
        };
        let Ok(t) = c.translate_coordinates(wid, root, 0, 0) else {
            continue;
        };
        let Ok(t) = t.reply() else {
            continue;
        };
        let mut x = t.dst_x as i32;
        let mut y = t.dst_y as i32;
        let mut w = g.width as i32;
        let mut h = g.height as i32;
        if let Some(ext) = extents {
            if let Ok(p) = c.get_property(false, wid, ext, AtomEnum::CARDINAL, 0, 4) {
                if let Ok(r) = p.reply() {
                    let v: Vec<u32> = r.value32().into_iter().flatten().collect();
                    if v.len() >= 4 {
                        let (l, rgt, tp, b) = (v[0] as i32, v[1] as i32, v[2] as i32, v[3] as i32);
                        x -= l;
                        y -= tp;
                        w += l + rgt;
                        h += tp + b;
                    }
                }
            }
        }
        if w < 8 || h < 8 {
            continue;
        }
        let mut title = String::new();
        if let Some(na) = name_atom {
                    if let Ok(p) = c.get_property(false, wid, na, AtomEnum::STRING, 0, 256) {
                if let Ok(r) = p.reply() {
                    title = String::from_utf8_lossy(&r.value).into_owned();
                }
            }
        }
        out.push((Rect::from_i32(x, y, w, h), title));
    }
    LAST_SOURCE.store(0, Ordering::Relaxed);
    out
}

fn parse_windows(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn child_rects() -> Vec<(Rect, String)> {
    let Ok((c, screen)) = x11rb::connect(None) else {
        return Vec::new();
    };
    let root = c.setup().roots[screen].root;
    let mut out = Vec::new();
    walk_children(&c, root, root, 0, 4, &mut out);
    out
}

fn walk_children(
    c: &x11rb::rust_connection::RustConnection,
    root: u32,
    wid: u32,
    depth: u32,
    max: u32,
    out: &mut Vec<(Rect, String)>,
) {
    if depth > max {
        return;
    }
    let Ok(tree) = c.query_tree(wid) else {
        return;
    };
    let Ok(QueryTreeReply { children, .. }) = tree.reply() else {
        return;
    };
    for child in children.into_iter().rev() {
        if let (Ok(g), Ok(t)) = (c.get_geometry(child), c.translate_coordinates(child, root, 0, 0)) {
            if let (Ok(g), Ok(t)) = (g.reply(), t.reply()) {
                if g.width >= 8 && g.height >= 8 {
                    out.push((
                        Rect::from_i32(t.dst_x as i32, t.dst_y as i32, g.width as i32, g.height as i32),
                        String::new(),
                    ));
                }
            }
        }
        walk_children(c, root, child, depth + 1, max, out);
    }
}

pub fn collect_elements() -> Vec<(Rect, String)> {
    if atspi_available() {
        LAST_SOURCE.store(1, Ordering::Relaxed);
    } else {
        LAST_SOURCE.store(2, Ordering::Relaxed);
    }
    child_rects()
}

pub fn translated_windows(origin: (i32, i32)) -> Vec<Rect> {
    window_rects()
        .into_iter()
        .map(|(r, _)| r.translated(-origin.0 as f32, -origin.1 as f32))
        .collect()
}

pub fn translated_elements(origin: (i32, i32)) -> Vec<Rect> {
    collect_elements()
        .into_iter()
        .map(|(r, _)| r.translated(-origin.0 as f32, -origin.1 as f32))
        .collect()
}

pub fn element_source() -> &'static str {
    match LAST_SOURCE.load(Ordering::Relaxed) {
        1 => "AT-SPI",
        _ => "X11 children",
    }
}

/// Same safety rule as Python: Atspi.init() can abort, so probe in a subprocess.
pub fn atspi_available() -> bool {
    static CACHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        let py = r#"
import gi
gi.require_version('Atspi','2.0')
from gi.repository import Atspi
Atspi.init()
d=Atspi.get_desktop(0)
print('ATSPI_OK', d.get_child_count())
"#;
        let ok = std::process::Command::new("python3")
            .args(["-c", py])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("ATSPI_OK"))
            .unwrap_or(false);
        crate::logutil::log(&format!("AT-SPI probe: {ok}"));
        ok
    })
}

pub fn warm_element_backend() {
    let _ = atspi_available();
}

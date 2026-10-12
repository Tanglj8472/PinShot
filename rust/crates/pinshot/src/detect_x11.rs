//! X11 EWMH window list + child walk. AT-SPI is probed in a subprocess (Python
//! `winlist.atspi_available`) and skipped when the a11y bus is missing.

use pinshot_core::geom::Rect;
use std::sync::atomic::{AtomicU8, Ordering};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, QueryTreeReply};

static LAST_SOURCE: AtomicU8 = AtomicU8::new(0);

/// Python `winlist._walk_x(..., max_depth=4)` includes depths 0..=4.
const X11_ELEM_MAX_DEPTH: u32 = 4;

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
    let type_atom = c
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom);
    let skip_types = shell_type_atoms(&c);
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
        if is_shell_window(&c, wid, type_atom, &skip_types) {
            continue;
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

fn shell_type_atoms(c: &x11rb::rust_connection::RustConnection) -> Vec<u32> {
    const NAMES: &[&[u8]] = &[
        b"_NET_WM_WINDOW_TYPE_DOCK",
        b"_NET_WM_WINDOW_TYPE_DESKTOP",
        b"_NET_WM_WINDOW_TYPE_TOOLBAR",
        b"_NET_WM_WINDOW_TYPE_MENU",
        b"_NET_WM_WINDOW_TYPE_UTILITY",
        b"_NET_WM_WINDOW_TYPE_SPLASH",
        b"_NET_WM_WINDOW_TYPE_DROPDOWN_MENU",
        b"_NET_WM_WINDOW_TYPE_POPUP_MENU",
        b"_NET_WM_WINDOW_TYPE_TOOLTIP",
        b"_NET_WM_WINDOW_TYPE_NOTIFICATION",
    ];
    NAMES
        .iter()
        .filter_map(|n| {
            c.intern_atom(false, n)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|a| a.atom)
        })
        .collect()
}

fn is_shell_window(
    c: &x11rb::rust_connection::RustConnection,
    wid: u32,
    type_atom: Option<u32>,
    skip: &[u32],
) -> bool {
    let Some(ty) = type_atom else {
        return false;
    };
    if skip.is_empty() {
        return false;
    }
    let Ok(p) = c.get_property(false, wid, ty, AtomEnum::ATOM, 0, 16) else {
        return false;
    };
    let Ok(r) = p.reply() else {
        return false;
    };
    let types: Vec<u32> = r.value32().into_iter().flatten().collect();
    types.iter().any(|a| skip.iter().any(|s| s == a))
}

fn parse_windows(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn client_ids(c: &x11rb::rust_connection::RustConnection, root: u32) -> Vec<u32> {
    let stacking = c
        .intern_atom(false, b"_NET_CLIENT_LIST_STACKING")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom);
    let list = c
        .intern_atom(false, b"_NET_CLIENT_LIST")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom);
    for atom in [stacking, list].into_iter().flatten() {
        if let Ok(p) = c.get_property(false, root, atom, AtomEnum::WINDOW, 0, 4096) {
            if let Ok(r) = p.reply() {
                if !r.value.is_empty() {
                    return parse_windows(&r.value);
                }
            }
        }
    }
    Vec::new()
}

fn child_rects() -> Vec<(Rect, String)> {
    let Ok((c, screen)) = x11rb::connect(None) else {
        return Vec::new();
    };
    let root = c.setup().roots[screen].root;
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
    let pid_atom = c
        .intern_atom(false, b"_NET_WM_PID")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|a| a.atom);
    let self_pid = std::process::id();
    let mut out = Vec::new();
    for wid in client_ids(&c, root).into_iter().rev() {
        if let Some(pid_atom) = pid_atom {
            if let Ok(p) = c.get_property(false, wid, pid_atom, AtomEnum::CARDINAL, 0, 1) {
                if let Ok(r) = p.reply() {
                    if r.value32().and_then(|mut it| it.next()) == Some(self_pid) {
                        continue;
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
        walk_viewable(&c, root, wid, 0, X11_ELEM_MAX_DEPTH, &mut out);
    }
    out.sort_by(|a, b| {
        a.0.area()
            .partial_cmp(&b.0.area())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

fn walk_viewable(
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
    let Ok(attrs) = c.get_window_attributes(wid) else {
        return;
    };
    let Ok(attrs) = attrs.reply() else {
        return;
    };
    if attrs.map_state != x11rb::protocol::xproto::MapState::VIEWABLE {
        return;
    }
    if let (Ok(g), Ok(t)) = (
        c.get_geometry(wid),
        c.translate_coordinates(wid, root, 0, 0),
    ) {
        if let (Ok(g), Ok(t)) = (g.reply(), t.reply()) {
            if g.width >= 8 && g.height >= 8 && g.width < 20000 && g.height < 20000 {
                out.push((
                    Rect::from_i32(
                        t.dst_x as i32,
                        t.dst_y as i32,
                        g.width as i32,
                        g.height as i32,
                    ),
                    format!("x11:{wid:#x}"),
                ));
            }
        }
    }
    let Ok(tree) = c.query_tree(wid) else {
        return;
    };
    let Ok(QueryTreeReply { children, .. }) = tree.reply() else {
        return;
    };
    for child in children.into_iter().rev() {
        walk_viewable(c, root, child, depth + 1, max, out);
    }
}

fn parse_atspi_line(line: &str) -> Option<(Rect, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let (coords, label) = match line.split_once('\t') {
        Some((c, l)) => (c, l.to_string()),
        None => (line, "atspi".into()),
    };
    let parts: Vec<&str> = coords.split(',').collect();
    if parts.len() != 4 {
        return None;
    }
    let xs: Result<Vec<i32>, _> = parts.iter().map(|s| s.trim().parse::<i32>()).collect();
    let Ok(xs) = xs else {
        return None;
    };
    Some((Rect::from_i32(xs[0], xs[1], xs[2], xs[3]), label))
}

fn atspi_rects(exclude_pids: &[u32]) -> Vec<(Rect, String)> {
    let exclude = exclude_pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let py = format!(
        r#"
import time
EXCLUDE = {{int(x) for x in "{exclude}".split(",") if x.strip()}}
try:
    import gi
    gi.require_version('Atspi','2.0')
    from gi.repository import Atspi
    Atspi.init()
except Exception:
    raise SystemExit(0)
SKIP = {{'application','redundant object','unknown','invalid'}}
SHOWING = Atspi.StateType.SHOWING
t_end = time.monotonic() + 0.6
def walk(acc, depth):
    if acc is None or depth > 12 or time.monotonic() > t_end: return
    try:
        role = acc.get_role_name()
        if depth > 0 and not acc.get_state_set().contains(SHOWING): return
        if role not in SKIP:
            comp = acc.get_component_iface()
            if comp:
                e = comp.get_extents(Atspi.CoordType.SCREEN)
                if e.width > 3 and e.height > 3 and e.width < 20000:
                    name = (acc.get_name() or '').replace('\n',' ').replace('\t',' ')
                    print(f"{{e.x}},{{e.y}},{{e.width}},{{e.height}}\t{{role}}:{{name}}")
        for i in range(min(acc.get_child_count(), 400)):
            walk(acc.get_child_at_index(i), depth+1)
    except Exception:
        return
d = Atspi.get_desktop(0)
for i in range(max(0, d.get_child_count())):
    app = d.get_child_at_index(i)
    try:
        if app.get_process_id() in EXCLUDE: continue
    except Exception:
        pass
    walk(app, 0)
"#
    );
    let Ok(out) = std::process::Command::new("python3")
        .args(["-c", &py])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(parse_atspi_line)
        .collect()
}

pub fn collect_elements() -> Vec<(Rect, String)> {
    let x11 = child_rects();
    if atspi_available() {
        let acc = atspi_rects(&[std::process::id()]);
        if !acc.is_empty() {
            LAST_SOURCE.store(1, Ordering::Relaxed);
            let mut all = acc;
            all.extend(x11);
            return all;
        }
    }
    LAST_SOURCE.store(2, Ordering::Relaxed);
    x11
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_element_walk_matches_python_depth() {
        assert_eq!(X11_ELEM_MAX_DEPTH, 4);
    }

    #[test]
    fn atspi_line_keeps_role_name() {
        let (r, label) = parse_atspi_line("10,20,30,40\tpush button:OK").unwrap();
        assert_eq!(
            (r.x as i32, r.y as i32, r.w as i32, r.h as i32),
            (10, 20, 30, 40)
        );
        assert_eq!(label, "push button:OK");
        let (_, fallback) = parse_atspi_line("1,2,3,4").unwrap();
        assert_eq!(fallback, "atspi");
        assert!(parse_atspi_line("nope").is_none());
    }
}

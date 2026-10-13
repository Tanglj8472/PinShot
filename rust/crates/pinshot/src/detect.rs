//! Top-level windows (EnumWindows + DWM) and live element hit-test.
//!
//! Element mode matches public Snipaste behavior: find the HWND under the
//! cursor (skip our overlay), then go deeper in that process only
//! (UIA control-view walk, else nested child HWND). No full-desktop tree dump.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use pinshot_core::geom::{Point, Rect};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ChildWindowFromPointEx, EnumChildWindows, EnumWindows, GetTopWindow, GetWindow, GetWindowLongW,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, CWP_SKIPINVISIBLE, CWP_SKIPTRANSPARENT, GWL_EXSTYLE, GW_HWNDNEXT,
    WS_EX_TOOLWINDOW,
};

use crate::win32::{is_cloaked, window_frame};

pub struct PointHit {
    pub rect: Rect,
    pub exe: String,
    pub depth: u32,
    pub elapsed_ms: u128,
}

struct EnumState {
    exclude_pid: u32,
    rects: Vec<(Rect, String)>,
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = &mut *(lparam.0 as *mut EnumState);
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() || is_cloaked(hwnd) {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == state.exclude_pid {
            return BOOL(1);
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let title = window_title(hwnd);
        if (ex & WS_EX_TOOLWINDOW.0 != 0) && title.is_empty() {
            return BOOL(1);
        }
        if let Some((x, y, w, h)) = window_frame(hwnd) {
            if w >= 8 && h >= 8 {
                state.rects.push((Rect::from_i32(x, y, w, h), title));
            }
        }
    }
    BOOL(1)
}

fn window_title(hwnd: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(hwnd);
        if n <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; n as usize + 1];
        let got = GetWindowTextW(hwnd, &mut buf);
        if got <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..got as usize])
    }
}

pub fn window_rects() -> Vec<(Rect, String)> {
    let mut state = EnumState {
        exclude_pid: unsafe { GetCurrentProcessId() },
        rects: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut state as *mut _ as isize));
    }
    state.rects
}

pub fn translated_windows(origin: (i32, i32)) -> Vec<Rect> {
    window_rects()
        .into_iter()
        .map(|(r, _)| r.translated(-origin.0 as f32, -origin.1 as f32))
        .collect()
}

struct Tops {
    exclude: u32,
    hwnds: Vec<HWND>,
}

unsafe extern "system" fn enum_tops(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let st = &mut *(lparam.0 as *mut Tops);
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() || is_cloaked(hwnd) {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != st.exclude {
            st.hwnds.push(hwnd);
        }
    }
    BOOL(1)
}

unsafe extern "system" fn enum_kids(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = &mut *(lparam.0 as *mut Vec<(Rect, String)>);
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_ok() {
            let w = r.right - r.left;
            let h = r.bottom - r.top;
            if w >= 8 && h >= 8 {
                out.push((
                    Rect::from_i32(r.left, r.top, w, h),
                    format!("hwnd:{:#x}", hwnd.0 as usize),
                ));
            }
        }
    }
    BOOL(1)
}

fn child_rects() -> Vec<(Rect, String)> {
    let mut tops = Tops {
        exclude: unsafe { GetCurrentProcessId() },
        hwnds: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(enum_tops), LPARAM(&mut tops as *mut _ as isize));
    }
    let mut rects = Vec::new();
    unsafe {
        for hwnd in tops.hwnds {
            let _ = EnumChildWindows(hwnd, Some(enum_kids), LPARAM(&mut rects as *mut _ as isize));
        }
    }
    rects.sort_by_key(|(r, _): &(Rect, String)| (r.area() as i64).max(0));
    rects
}

/// UI Automation control-view walk (time-budgeted). `None` → child HWND fallback.
fn try_uia() -> Option<Vec<(Rect, String)>> {
    let uia = uia_instance()?;
    let walker = uia.get_control_view_walker().ok()?;
    let deadline = Instant::now() + Duration::from_millis(600);
    let mut out = Vec::new();
    let mut tops = Tops {
        exclude: unsafe { GetCurrentProcessId() },
        hwnds: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(enum_tops), LPARAM(&mut tops as *mut _ as isize));
    }
    for hwnd in tops.hwnds.into_iter().take(12) {
        if Instant::now() > deadline {
            break;
        }
        let el = uia.element_from_handle(hwnd.into()).ok();
        if let Some(el) = el {
            walk_uia(&walker, &el, 0, deadline, &mut out);
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn walk_uia(
    walker: &uiautomation::UITreeWalker,
    el: &uiautomation::UIElement,
    depth: u32,
    deadline: Instant,
    out: &mut Vec<(Rect, String)>,
) {
    if depth > 10 || Instant::now() > deadline {
        return;
    }
    if let Ok(off) = el.is_offscreen() {
        if !off {
            if let Ok(r) = el.get_bounding_rectangle() {
                let w = r.get_width();
                let h = r.get_height();
                if w > 3 && h > 3 {
                    let name = el.get_name().unwrap_or_default();
                    let kind = el.get_localized_control_type().unwrap_or_default();
                    out.push((
                        Rect::from_i32(r.get_left(), r.get_top(), w, h),
                        format!("{kind}:{name}"),
                    ));
                }
            }
        }
    }
    if let Ok(mut child) = walker.get_first_child(el) {
        loop {
            walk_uia(walker, &child, depth + 1, deadline, out);
            match walker.get_next_sibling(&child) {
                Ok(next) => child = next,
                Err(_) => break,
            }
            if Instant::now() > deadline {
                break;
            }
        }
    }
}

static LAST_SOURCE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn collect_elements() -> Vec<(Rect, String)> {
    let kids = child_rects();
    if let Some(mut r) = try_uia() {
        if !r.is_empty() {
            LAST_SOURCE.store(1, std::sync::atomic::Ordering::Relaxed);
            // Same as Python: UIA first (finer), child HWNDs fill apps without a11y.
            r.extend(kids);
            return r;
        }
    }
    LAST_SOURCE.store(2, std::sync::atomic::Ordering::Relaxed);
    kids
}

pub fn warm_element_backend() {
    let _ = uia_instance();
}

pub fn translated_elements(origin: (i32, i32)) -> Vec<Rect> {
    collect_elements()
        .into_iter()
        .map(|(r, _)| r.translated(-origin.0 as f32, -origin.1 as f32))
        .collect()
}

pub fn element_source() -> &'static str {
    match LAST_SOURCE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => "UI Automation",
        2 => "child windows",
        _ => "hit-test",
    }
}

fn uia_instance() -> Option<&'static uiautomation::UIAutomation> {
    static UIA: OnceLock<Option<uiautomation::UIAutomation>> = OnceLock::new();
    UIA.get_or_init(|| {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
        uiautomation::UIAutomation::new().ok()
    })
    .as_ref()
}

fn hwnd_nonzero(hwnd: HWND) -> bool {
    !hwnd.is_invalid()
}

fn exclude_hwnd(raw: Option<isize>) -> HWND {
    raw.map(|v| HWND(v as *mut _)).unwrap_or_default()
}

fn top_window_at(sx: i32, sy: i32, exclude: HWND, exclude_pid: u32) -> Option<HWND> {
    unsafe {
        let mut hwnd = GetTopWindow(HWND::default()).ok()?;
        loop {
            if hwnd != exclude
                && IsWindowVisible(hwnd).as_bool()
                && !IsIconic(hwnd).as_bool()
                && !is_cloaked(hwnd)
            {
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid != exclude_pid {
                    if let Some((x, y, w, h)) = window_frame(hwnd) {
                        if sx >= x && sy >= y && sx < x + w && sy < y + h {
                            return Some(hwnd);
                        }
                    }
                }
            }
            hwnd = match GetWindow(hwnd, GW_HWNDNEXT) {
                Ok(next) => next,
                Err(_) => break,
            };
        }
    }
    None
}

fn process_exe(pid: u32) -> String {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return format!("pid:{pid}");
        };
        let mut n = 260u32;
        let mut buf = vec![0u16; 260];
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut n,
        );
        let _ = windows::Win32::Foundation::CloseHandle(handle);
        if ok.is_ok() && n > 0 {
            let s = String::from_utf16_lossy(&buf[..n as usize]);
            return exe_stem(&s);
        }
        format!("pid:{pid}")
    }
}

fn exe_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.to_string())
}

fn rect_contains_i32(r: Rect, x: i32, y: i32) -> bool {
    r.contains(Point::from_i32(x, y))
}

fn deepest_child_hwnd(top: HWND, sx: i32, sy: i32) -> (HWND, u32) {
    let mut cur = top;
    let mut depth = 0u32;
    unsafe {
        loop {
            let mut pt = POINT { x: sx, y: sy };
            if !ScreenToClient(cur, &mut pt).as_bool() {
                break;
            }
            let child = ChildWindowFromPointEx(cur, pt, CWP_SKIPINVISIBLE | CWP_SKIPTRANSPARENT);
            if !hwnd_nonzero(child) || child == cur {
                break;
            }
            cur = child;
            depth += 1;
            if depth > 24 {
                break;
            }
        }
    }
    (cur, depth)
}

fn hwnd_rect(hwnd: HWND) -> Option<Rect> {
    if let Some((x, y, w, h)) = window_frame(hwnd) {
        if w >= 3 && h >= 3 {
            return Some(Rect::from_i32(x, y, w, h));
        }
    }
    unsafe {
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_ok() {
            let w = r.right - r.left;
            let h = r.bottom - r.top;
            if w >= 3 && h >= 3 {
                return Some(Rect::from_i32(r.left, r.top, w, h));
            }
        }
    }
    None
}

fn better_hit(best: Option<(Rect, u32)>, rect: Rect, depth: u32) -> bool {
    match best {
        None => true,
        Some((br, bd)) => depth > bd || (depth == bd && rect.area() < br.area()),
    }
}

fn deepest_uia_at(top: HWND, sx: i32, sy: i32, deadline: Instant) -> Option<(Rect, u32)> {
    let uia = uia_instance()?;
    let walker = uia.get_control_view_walker().ok()?;
    let root = uia.element_from_handle(top.into()).ok()?;
    let mut best: Option<(Rect, u32)> = None;
    walk_uia_at(&walker, &root, sx, sy, 0, deadline, &mut best);
    best
}

fn walk_uia_at(
    walker: &uiautomation::UITreeWalker,
    el: &uiautomation::UIElement,
    sx: i32,
    sy: i32,
    depth: u32,
    deadline: Instant,
    best: &mut Option<(Rect, u32)>,
) {
    if depth > 16 || Instant::now() > deadline {
        return;
    }
    let Ok(r) = el.get_bounding_rectangle() else {
        return;
    };
    let rect = Rect::from_i32(r.get_left(), r.get_top(), r.get_width(), r.get_height());
    if rect.w <= 3.0 || rect.h <= 3.0 || !rect_contains_i32(rect, sx, sy) {
        return;
    }
    if let Ok(off) = el.is_offscreen() {
        if off && depth > 0 {
            return;
        }
    }
    if better_hit(*best, rect, depth) {
        *best = Some((rect, depth));
    }
    if let Ok(mut child) = walker.get_first_child(el) {
        loop {
            walk_uia_at(walker, &child, sx, sy, depth + 1, deadline, best);
            match walker.get_next_sibling(&child) {
                Ok(next) => child = next,
                Err(_) => break,
            }
            if Instant::now() > deadline {
                break;
            }
        }
    }
}

/// Live hit-test at a virtual-desktop (screen) point. Skips `exclude_hwnd` (overlay).
pub fn element_at_point(sx: i32, sy: i32, exclude: Option<isize>) -> Option<PointHit> {
    let t0 = Instant::now();
    let self_pid = unsafe { GetCurrentProcessId() };
    let skip = exclude_hwnd(exclude);
    let top = top_window_at(sx, sy, skip, self_pid)?;
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(top, Some(&mut pid));
    }
    let exe = process_exe(pid);
    crate::logutil::log(&format!("detect cursor in window exe={exe}"));

    let deadline = Instant::now() + Duration::from_millis(20);
    let uia_hit = deepest_uia_at(top, sx, sy, deadline);
    let (child, hwnd_depth) = deepest_child_hwnd(top, sx, sy);
    let hwnd_rect = hwnd_rect(child).or_else(|| hwnd_rect(top))?;

    let (rect, depth, src) = match uia_hit {
        Some((r, d)) if d > 0 || r.area() <= hwnd_rect.area() => (r, d, 1u8),
        Some((r, d)) if hwnd_depth == 0 => (r, d, 1u8),
        _ if hwnd_depth > 0 => (hwnd_rect, hwnd_depth, 2u8),
        Some((r, d)) => (r, d, 1u8),
        None => (hwnd_rect, hwnd_depth, 2u8),
    };
    LAST_SOURCE.store(src, std::sync::atomic::Ordering::Relaxed);
    Some(PointHit {
        rect,
        exe,
        depth,
        elapsed_ms: t0.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_stem_keeps_filename() {
        assert_eq!(
            exe_stem(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            "chrome.exe"
        );
        assert_eq!(exe_stem("firefox"), "firefox");
    }

    #[test]
    fn rect_contains_screen_point() {
        let r = Rect::from_i32(10, 20, 100, 40);
        assert!(rect_contains_i32(r, 10, 20));
        assert!(rect_contains_i32(r, 109, 59));
        assert!(!rect_contains_i32(r, 110, 20));
        assert!(!rect_contains_i32(r, 10, 60));
    }

    #[test]
    fn better_hit_prefers_deeper_then_smaller() {
        let win = Rect::from_i32(0, 0, 800, 600);
        let pane = Rect::from_i32(10, 10, 400, 300);
        let btn = Rect::from_i32(20, 20, 80, 24);
        let wide = Rect::from_i32(20, 20, 200, 24);
        assert!(better_hit(None, win, 0));
        assert!(better_hit(Some((win, 0)), pane, 1));
        assert!(better_hit(Some((pane, 1)), btn, 2));
        assert!(!better_hit(Some((btn, 2)), wide, 2));
        assert!(better_hit(Some((wide, 2)), btn, 2));
    }
}

//! Top-level windows (EnumWindows + DWM) and live element hit-test.
//!
//! Public Inspect / AHK-UIA / screenshot-tool approach: wake Chromium a11y
//! (`WM_GETOBJECT`), make the overlay hit-through, then
//! `IUIAutomation::ElementFromPoint` + `AccessibleObjectFromPoint`, and walk
//! only the smallest child that still contains the cursor ("go deeper").

use std::cell::{OnceCell, RefCell};
use std::collections::HashSet;
use std::time::{Duration, Instant};

use pinshot_core::geom::{Point, Rect};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{AccessibleObjectFromPoint, IAccessible};
use windows::Win32::UI::WindowsAndMessaging::{
    ChildWindowFromPointEx, EnumChildWindows, EnumWindows, GetAncestor, GetClassNameW,
    GetTopWindow, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, RealChildWindowFromPoint,
    SendMessageTimeoutW, WindowFromPoint, CWP_SKIPINVISIBLE, CWP_SKIPTRANSPARENT, GA_ROOT,
    GWL_EXSTYLE, GW_HWNDNEXT, OBJID_CLIENT, SMTO_ABORTIFHUNG, WM_GETOBJECT, WS_EX_TOOLWINDOW,
};

use crate::win32::{is_cloaked, window_frame, with_hit_through};

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
        let class = window_class(hwnd);
        // Taskbar / notify flyouts are TOOLWINDOW with no title. Dropping
        // them is why a tray snip selected the menu once, then never again.
        if (ex & WS_EX_TOOLWINDOW.0 != 0) && title.is_empty() && !is_tray_class(&class) {
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

fn window_class(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 256];
        let n = GetClassNameW(hwnd, &mut buf);
        if n <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

fn is_tray_class(class: &str) -> bool {
    matches!(
        class,
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "TrayNotifyWnd"
            | "TopLevelWindowForOverflowXamlIsland"
    )
}

struct TrayEnum {
    hwnds: Vec<HWND>,
}

unsafe extern "system" fn enum_tray_tops(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let st = &mut *(lparam.0 as *mut TrayEnum);
    unsafe {
        if IsWindowVisible(hwnd).as_bool() && is_tray_class(&window_class(hwnd)) {
            st.hwnds.push(hwnd);
        }
    }
    BOOL(1)
}

unsafe extern "system" fn enum_tray_kids(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = &mut *(lparam.0 as *mut Vec<HWND>);
    unsafe {
        if IsWindowVisible(hwnd).as_bool() {
            out.push(hwnd);
        }
    }
    BOOL(1)
}

fn tray_top_hwnds() -> Vec<HWND> {
    let mut st = TrayEnum { hwnds: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(enum_tray_tops), LPARAM(&mut st as *mut _ as isize));
    }
    st.hwnds
}

fn append_tray_rects(out: &mut Vec<(Rect, String)>) {
    for hwnd in tray_top_hwnds() {
        if let Some(r) = hwnd_rect(hwnd) {
            out.push((r, window_class(hwnd)));
        }
        let mut kids = Vec::new();
        unsafe {
            let _ = EnumChildWindows(
                hwnd,
                Some(enum_tray_kids),
                LPARAM(&mut kids as *mut _ as isize),
            );
        }
        for kid in kids {
            if let Some(r) = hwnd_rect(kid) {
                if r.w >= 8.0 && r.h >= 8.0 {
                    out.push((r, window_class(kid)));
                }
            }
        }
    }
}

fn tray_rects() -> Vec<(Rect, u32)> {
    thread_local! {
        static CACHE: std::cell::RefCell<(Option<Instant>, Vec<(Rect, u32)>)> =
            const { std::cell::RefCell::new((None, Vec::new())) };
    }
    CACHE.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(at) = slot.0 {
            if at.elapsed() < Duration::from_millis(300) {
                return slot.1.clone();
            }
        }
        let mut out = Vec::new();
        for top in tray_top_hwnds() {
            if let Some(r) = hwnd_rect(top) {
                out.push((r, 1));
            }
            let mut kids = Vec::new();
            unsafe {
                let _ = EnumChildWindows(
                    top,
                    Some(enum_tray_kids),
                    LPARAM(&mut kids as *mut _ as isize),
                );
            }
            for kid in kids {
                if let Some(r) = hwnd_rect(kid) {
                    if r.w >= 8.0 && r.h >= 8.0 {
                        out.push((r, 2));
                    }
                }
            }
        }
        *slot = (Some(Instant::now()), out.clone());
        out
    })
}

/// Smallest visible tray / notify rect under the cursor. Independent of
/// overlay z-order — the first snip-from-tray hit is the popup menu; after
/// it closes, WindowFromPoint often returns the desktop instead.
fn tray_hit_at(sx: i32, sy: i32) -> Option<(Rect, u32)> {
    let mut best: Option<(Rect, u32)> = None;
    for (r, depth) in tray_rects() {
        if rect_contains_i32(r, sx, sy) && better_hit(best.map(|(br, bd)| (br, bd)), r, depth) {
            best = Some((r, depth));
        }
    }
    best
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
    append_tray_rects(&mut state.rects);
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
        let el = uia.element_from_handle(uia_handle(hwnd)).ok();
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
        3 => "MSAA",
        _ => "hit-test",
    }
}

fn uia_handle(hwnd: HWND) -> uiautomation::types::Handle {
    // pinshot's `windows` crate and `uiautomation`'s crate are different versions.
    uiautomation::types::Handle::from(hwnd.0 as isize)
}

fn uia_instance() -> Option<uiautomation::UIAutomation> {
    thread_local! {
        static UIA: OnceCell<Option<uiautomation::UIAutomation>> = const { OnceCell::new() };
    }
    UIA.with(|cell| {
        cell.get_or_init(|| {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(
                    None,
                    windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
                );
            }
            uiautomation::UIAutomation::new().ok()
        })
        .clone()
    })
}

fn hwnd_nonzero(hwnd: HWND) -> bool {
    !hwnd.is_invalid()
}

fn exclude_hwnd(raw: Option<isize>) -> HWND {
    raw.map(|v| HWND(v as *mut _)).unwrap_or_default()
}

fn usable_top(hwnd: HWND, exclude: HWND, exclude_pid: u32) -> bool {
    if !hwnd_nonzero(hwnd) || hwnd == exclude {
        return false;
    }
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() || is_cloaked(hwnd) {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid != exclude_pid
    }
}

fn top_window_at(sx: i32, sy: i32, exclude: HWND, exclude_pid: u32) -> Option<HWND> {
    unsafe {
        let hit = WindowFromPoint(POINT { x: sx, y: sy });
        if hwnd_nonzero(hit) {
            let root = GetAncestor(hit, GA_ROOT);
            let top = if hwnd_nonzero(root) { root } else { hit };
            if usable_top(top, exclude, exclude_pid) {
                if let Some((x, y, w, h)) = window_frame(top) {
                    if sx >= x && sy >= y && sx < x + w && sy < y + h {
                        return Some(top);
                    }
                }
            }
        }
        let mut hwnd = GetTopWindow(HWND::default()).ok()?;
        loop {
            if usable_top(hwnd, exclude, exclude_pid) {
                if let Some((x, y, w, h)) = window_frame(hwnd) {
                    if sx >= x && sy >= y && sx < x + w && sy < y + h {
                        return Some(hwnd);
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
            let child = {
                let real = RealChildWindowFromPoint(cur, pt);
                if hwnd_nonzero(real) && real != cur {
                    real
                } else {
                    ChildWindowFromPointEx(cur, pt, CWP_SKIPINVISIBLE | CWP_SKIPTRANSPARENT)
                }
            };
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

fn element_rect(el: &uiautomation::UIElement) -> Option<Rect> {
    let r = el.get_bounding_rectangle().ok()?;
    let rect = Rect::from_i32(r.get_left(), r.get_top(), r.get_width(), r.get_height());
    if rect.w <= 3.0 || rect.h <= 3.0 {
        None
    } else {
        Some(rect)
    }
}

fn is_our_uia(el: &uiautomation::UIElement, skip: HWND, self_pid: u32) -> bool {
    if el.get_process_id().ok() == Some(self_pid) {
        return true;
    }
    if let Ok(h) = el.get_native_window_handle() {
        let hid: isize = h.into();
        if hid != 0 && hid == skip.0 as isize {
            return true;
        }
    }
    false
}

fn wake_accessibility(hwnd: HWND) {
    if !hwnd_nonzero(hwnd) {
        return;
    }
    thread_local! {
        static WOKE: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
    }
    let key = hwnd.0 as isize;
    let already = WOKE.with(|s| s.borrow().contains(&key));
    if already {
        return;
    }
    unsafe {
        let mut result = 0usize;
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_GETOBJECT,
            WPARAM(0),
            LPARAM(OBJID_CLIENT.0 as isize),
            SMTO_ABORTIFHUNG,
            50,
            Some(&mut result),
        );
    }
    WOKE.with(|s| {
        s.borrow_mut().insert(key);
    });
}

/// Drill into the first smaller child that contains the point.
///
/// Visual Studio's control tree has hundreds of siblings at the first
/// level. Scanning all of them in 25ms never leaves depth 0/1. Recursing
/// as soon as one containing child is found actually reaches buttons.
fn go_deeper(
    walker: &uiautomation::UITreeWalker,
    el: &uiautomation::UIElement,
    sx: i32,
    sy: i32,
    depth: u32,
    deadline: Instant,
) -> Option<(Rect, u32)> {
    if depth > 24 || Instant::now() > deadline {
        return element_rect(el)
            .filter(|r| rect_contains_i32(*r, sx, sy))
            .map(|r| (r, depth));
    }
    let rect = element_rect(el)?;
    if !rect_contains_i32(rect, sx, sy) {
        return None;
    }
    if let Ok(mut child) = walker.get_first_child(el) {
        loop {
            if Instant::now() > deadline {
                break;
            }
            if let Some(cr) = element_rect(&child) {
                if rect_contains_i32(cr, sx, sy) && cr.area() + 1.0 < rect.area() {
                    if let Some(deeper) = go_deeper(walker, &child, sx, sy, depth + 1, deadline) {
                        return Some(deeper);
                    }
                }
            }
            match walker.get_next_sibling(&child) {
                Ok(next) => child = next,
                Err(_) => break,
            }
        }
    }
    Some((rect, depth))
}

fn walk_from(
    uia: &uiautomation::UIAutomation,
    start: &uiautomation::UIElement,
    sx: i32,
    sy: i32,
    deadline: Instant,
) -> Option<(Rect, u32)> {
    let control = uia.get_control_view_walker().ok()?;
    let hit = go_deeper(&control, start, sx, sy, 0, deadline);
    let shallow = hit.as_ref().map(|(_, d)| *d < 2).unwrap_or(true);
    // Raw walk is a second tree walk — only if the first pass stayed shallow
    // and the mouse still has budget (otherwise the loupe stutters).
    if shallow && Instant::now() + Duration::from_millis(6) < deadline {
        if let Ok(raw) = uia.get_raw_view_walker() {
            if let Some(raw_hit) = go_deeper(&raw, start, sx, sy, 0, deadline) {
                let take = match &hit {
                    None => true,
                    Some((r, d)) => better_hit(Some((*r, *d)), raw_hit.0, raw_hit.1),
                };
                if take {
                    return Some(raw_hit);
                }
            }
        }
    }
    hit
}

fn uia_from_point(
    sx: i32,
    sy: i32,
    top: HWND,
    skip: HWND,
    self_pid: u32,
    deadline: Instant,
) -> Option<(Rect, u32)> {
    let uia = uia_instance()?;
    let start = uia
        .element_from_point(uiautomation::types::Point::new(sx, sy))
        .ok()
        .filter(|el| !is_our_uia(el, skip, self_pid))
        .or_else(|| uia.element_from_handle(uia_handle(top)).ok());
    let start = start.filter(|el| !is_our_uia(el, skip, self_pid))?;
    walk_from(&uia, &start, sx, sy, deadline)
}

/// Drop automation hits that are basically the top-level window. Those
/// lose to the single WPF child HWND (depth=1, "child windows").
fn is_shallow_window_hit(hit: Rect, top: Rect) -> bool {
    let ta = top.area().max(1.0);
    hit.area() >= ta * 0.82
}

fn msaa_from_point(sx: i32, sy: i32) -> Option<(Rect, u32)> {
    unsafe {
        let mut acc: Option<IAccessible> = None;
        let mut child = windows::core::VARIANT::default();
        AccessibleObjectFromPoint(POINT { x: sx, y: sy }, &mut acc, &mut child).ok()?;
        let acc = acc?;
        let mut left = 0i32;
        let mut top = 0i32;
        let mut width = 0i32;
        let mut height = 0i32;
        acc.accLocation(&mut left, &mut top, &mut width, &mut height, &child)
            .ok()?;
        if width <= 3 || height <= 3 {
            return None;
        }
        let rect = Rect::from_i32(left, top, width, height);
        if rect_contains_i32(rect, sx, sy) {
            Some((rect, 1))
        } else {
            None
        }
    }
}

fn pick_hit(cands: &[(Rect, u32, u8)]) -> Option<(Rect, u32, u8)> {
    cands.iter().copied().min_by(|a, b| {
        a.0.area()
            .partial_cmp(&b.0.area())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.cmp(&a.1))
    })
}

/// Live hit-test at a virtual-desktop (screen) point. Skips `exclude_hwnd` (overlay).
pub fn element_at_point(sx: i32, sy: i32, exclude: Option<isize>) -> Option<PointHit> {
    let t0 = Instant::now();
    let self_pid = unsafe { GetCurrentProcessId() };
    let skip = exclude_hwnd(exclude);
    // Taskbar / notify geometry first. Maximized apps (VS, Chrome) extend
    // under the Win11 taskbar, so UIA / WindowFromPoint return devenv even
    // when the cursor is on the clock. Tray HWND can also sit above the
    // overlay and steal the click; the rect is still the right hover.
    if let Some((rect, depth)) = tray_hit_at(sx, sy) {
        LAST_SOURCE.store(2, std::sync::atomic::Ordering::Relaxed);
        log_detect_change("tray", depth, 2, t0.elapsed().as_millis());
        return Some(PointHit {
            rect,
            exe: "tray".into(),
            depth,
            elapsed_ms: t0.elapsed().as_millis(),
        });
    }
    // Hit-through for z-order and UIA: a recycled overlay HWND stays in the
    // stack even when hidden, and otherwise absorbs ElementFromPoint.
    with_hit_through(skip, || element_at_point_inner(sx, sy, skip, self_pid, t0))
}

/// HWND-only hover for mouse motion. No UIA / MSAA — those stall WPF hosts
/// for 20–120ms and make the loupe hitch.
pub fn window_at_point(sx: i32, sy: i32, exclude: Option<isize>) -> Option<PointHit> {
    let t0 = Instant::now();
    let self_pid = unsafe { GetCurrentProcessId() };
    let skip = exclude_hwnd(exclude);
    if let Some((rect, depth)) = tray_hit_at(sx, sy) {
        LAST_SOURCE.store(2, std::sync::atomic::Ordering::Relaxed);
        log_detect_change("tray", depth, 2, t0.elapsed().as_millis());
        return Some(PointHit {
            rect,
            exe: "tray".into(),
            depth,
            elapsed_ms: t0.elapsed().as_millis(),
        });
    }
    with_hit_through(skip, || {
        let top = top_window_at(sx, sy, skip, self_pid)?;
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(top, Some(&mut pid));
        }
        let exe = process_exe(pid);
        let (child, hwnd_depth) = deepest_child_hwnd(top, sx, sy);
        let rect = hwnd_rect(child).or_else(|| hwnd_rect(top))?;
        LAST_SOURCE.store(2, std::sync::atomic::Ordering::Relaxed);
        log_detect_change(&exe, hwnd_depth, 2, t0.elapsed().as_millis());
        Some(PointHit {
            rect,
            exe,
            depth: hwnd_depth,
            elapsed_ms: t0.elapsed().as_millis(),
        })
    })
}

fn element_at_point_inner(
    sx: i32,
    sy: i32,
    skip: HWND,
    self_pid: u32,
    t0: Instant,
) -> Option<PointHit> {
    let top = top_window_at(sx, sy, skip, self_pid)?;
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(top, Some(&mut pid));
    }
    let exe = process_exe(pid);

    let (child, hwnd_depth) = deepest_child_hwnd(top, sx, sy);
    wake_accessibility(top);
    wake_accessibility(child);

    let top_rect = hwnd_rect(top);
    // Live hover must stay off the UI thread's 16ms budget. MSAA
    // AccessibleObjectFromPoint was 100ms+ on WPF/NovaFlow and is skipped.
    let deadline = Instant::now() + Duration::from_millis(8);
    let uia_hit = uia_from_point(sx, sy, top, skip, self_pid, deadline).filter(|(r, _)| {
        top_rect
            .map(|top| !is_shallow_window_hit(*r, top))
            .unwrap_or(true)
    });

    let mut cands = Vec::new();
    if let Some((r, d)) = uia_hit {
        cands.push((r, d, 1u8));
    }
    if let Some(r) = hwnd_rect(child).or_else(|| hwnd_rect(top)) {
        cands.push((r, hwnd_depth, 2u8));
    }
    if let Some((r, d)) = tray_hit_at(sx, sy) {
        cands.push((r, d, 2u8));
    }
    let (rect, depth, src) = pick_hit(&cands)?;
    LAST_SOURCE.store(src, std::sync::atomic::Ordering::Relaxed);
    log_detect_change(&exe, depth, src, t0.elapsed().as_millis());
    Some(PointHit {
        rect,
        exe,
        depth,
        elapsed_ms: t0.elapsed().as_millis(),
    })
}

fn log_detect_change(exe: &str, depth: u32, src: u8, ms: u128) {
    thread_local! {
        static LAST: std::cell::RefCell<(String, u32, u8)> =
            const { std::cell::RefCell::new((String::new(), u32::MAX, 255)) };
    }
    let changed = LAST.with(|slot| {
        let mut prev = slot.borrow_mut();
        let cur = (exe.to_string(), depth, src);
        if *prev == cur {
            false
        } else {
            *prev = cur;
            true
        }
    });
    if changed {
        crate::logutil::log_at("trace", &format!(
            "detect element for: {exe} depth={depth} {ms}ms ({})",
            element_source()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_class_names() {
        assert!(is_tray_class("Shell_TrayWnd"));
        assert!(is_tray_class("TrayNotifyWnd"));
        assert!(is_tray_class("NotifyIconOverflowWindow"));
        assert!(is_tray_class("TopLevelWindowForOverflowXamlIsland"));
        assert!(!is_tray_class("Chrome_WidgetWin_1"));
    }

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

    #[test]
    fn pick_hit_takes_smallest_then_deeper() {
        let window = (Rect::from_i32(0, 0, 800, 600), 0u32, 2u8);
        let button = (Rect::from_i32(40, 40, 80, 24), 3u32, 1u8);
        let pane = (Rect::from_i32(10, 10, 400, 300), 1u32, 3u8);
        let picked = pick_hit(&[window, pane, button]).unwrap();
        assert_eq!(
            (picked.0.w as i32, picked.0.h as i32, picked.2),
            (80, 24, 1)
        );
    }

    #[test]
    fn shallow_window_hit_rejects_almost_full_frame() {
        let top = Rect::from_i32(0, 0, 1920, 1080);
        let wpf_host = Rect::from_i32(0, 0, 1920, 1040);
        let button = Rect::from_i32(40, 80, 96, 28);
        assert!(is_shallow_window_hit(wpf_host, top));
        assert!(!is_shallow_window_hit(button, top));
    }
}

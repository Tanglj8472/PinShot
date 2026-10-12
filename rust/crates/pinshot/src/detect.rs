//! Top-level windows (EnumWindows + DWM) and element hit-test (UIA or child HWND fallback).

use pinshot_core::geom::Rect;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetWindowLongW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

use crate::win32::{is_cloaked, window_frame};

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
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }
    let uia = uiautomation::UIAutomation::new().ok()?;
    let walker = uia.get_control_view_walker().ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(600);
    let mut out = Vec::new();
    let mut tops = Tops {
        exclude: unsafe { GetCurrentProcessId() },
        hwnds: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(enum_tops), LPARAM(&mut tops as *mut _ as isize));
    }
    for hwnd in tops.hwnds.into_iter().take(12) {
        if std::time::Instant::now() > deadline {
            break;
        }
        let handle = uiautomation::types::Handle::from(hwnd.0 as isize);
        let el = uia.element_from_handle(handle).ok();
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
    deadline: std::time::Instant,
    out: &mut Vec<(Rect, String)>,
) {
    if depth > 10 || std::time::Instant::now() > deadline {
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
            if std::time::Instant::now() > deadline {
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
    let _ = try_uia();
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
        _ => "child windows",
    }
}

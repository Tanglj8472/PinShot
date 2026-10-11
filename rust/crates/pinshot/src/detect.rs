//! Top-level window rectangles via EnumWindows + DWM frame bounds.
//! UI Automation is a Phase-later hook (`ElementDetectorStub`).

use pinshot_core::geom::Rect;
use pinshot_core::hooks::{ElementDetector, ElementDetectorStub};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
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

/// Overlay-local hover candidates (translated by virtual origin).
pub fn translated_windows(origin: (i32, i32)) -> Vec<Rect> {
    window_rects()
        .into_iter()
        .map(|(r, _)| r.translated(-origin.0 as f32, -origin.1 as f32))
        .collect()
}

pub fn element_rects_stub() -> Vec<Rect> {
    let mut det = ElementDetectorStub;
    det.snapshot(0).into_iter().map(|(r, _)| r).collect()
}

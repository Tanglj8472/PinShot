//! Win32 helpers: DPI, window styles, click-through, global hotkeys.

use pinshot_core::hotkey::{parse_hotkey, Command, Hotkey, MOD_NOREPEAT};
use pinshot_core::Config;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::Mutex;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::UpdateWindow;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, VK_CONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, IsWindow, PostMessageW,
    RegisterClassW, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    GWL_EXSTYLE, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SW_HIDE, WM_DESTROY, WM_HOTKEY, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::event::AppEvent;

pub fn hwnd_of(window: &Window) -> HWND {
    match window.window_handle() {
        Ok(h) => match h.as_raw() {
            RawWindowHandle::Win32(w) => HWND(w.hwnd.get() as *mut _),
            _ => HWND::default(),
        },
        Err(_) => HWND::default(),
    }
}

pub fn init_dpi() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

pub fn hide_sync(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = UpdateWindow(hwnd);
    }
}

pub fn apply_overlay_style(hwnd: HWND) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            (ex | WS_EX_TOPMOST.0 as isize | WS_EX_TOOLWINDOW.0 as isize)
                & !(WS_EX_NOACTIVATE.0 as isize),
        );
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub fn apply_pin_style(hwnd: HWND) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex | WS_EX_TOPMOST.0 as isize | WS_EX_TOOLWINDOW.0 as isize | WS_EX_LAYERED.0 as isize,
        );
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        let _ = SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), 255, LWA_ALPHA);
    }
}

pub fn pointer_ctrl_down() -> bool {
    unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 }
}

pub fn set_opacity(hwnd: HWND, opacity: f32) {
    let alpha = (opacity.clamp(0.1, 1.0) * 255.0).round() as u8;
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex & WS_EX_LAYERED.0 as isize == 0 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
        }
        let _ = SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), alpha, LWA_ALPHA);
    }
}

pub fn set_click_through(hwnd: HWND, on: bool, opacity: f32) {
    unsafe {
        let mut ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if on {
            ex |= WS_EX_TRANSPARENT.0 as isize | WS_EX_LAYERED.0 as isize;
        } else {
            ex &= !(WS_EX_TRANSPARENT.0 as isize);
            ex |= WS_EX_LAYERED.0 as isize;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        let a = if on {
            (opacity.min(0.55) * 255.0).round() as u8
        } else {
            (opacity.clamp(0.1, 1.0) * 255.0).round() as u8
        };
        let _ = SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), a, LWA_ALPHA);
    }
}

#[derive(Clone, Copy)]
struct HotkeyBind {
    id: i32,
    cmd: Command,
}

static PROXY: Mutex<Option<EventLoopProxy<AppEvent>>> = Mutex::new(None);
static BINDS: Mutex<Vec<HotkeyBind>> = Mutex::new(Vec::new());

unsafe extern "system" fn hotkey_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_HOTKEY {
        let id = wparam.0 as i32;
        if let Ok(binds) = BINDS.lock() {
            if let Some(b) = binds.iter().find(|b| b.id == id) {
                if let Ok(g) = PROXY.lock() {
                    if let Some(p) = g.as_ref() {
                        let _ = p.send_event(AppEvent::Hotkey(b.cmd));
                    }
                }
            }
        }
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

pub struct HotkeyHost {
    hwnd: HWND,
    ids: Vec<i32>,
}

impl HotkeyHost {
    pub fn install(proxy: EventLoopProxy<AppEvent>, cfg: &Config) -> windows::core::Result<Self> {
        *PROXY.lock().expect("proxy") = Some(proxy);
        let class = w!("PinShotHotkeyHost");
        unsafe {
            let hinst = GetModuleHandleW(None)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(hotkey_wndproc),
                hInstance: hinst.into(),
                lpszClassName: class,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class,
                w!("PinShotHotkeys"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                hinst,
                None,
            )?;
            let _ = ShowWindow(hwnd, SW_HIDE);
            let _ = UpdateWindow(hwnd);

            let specs: [(&str, Command); 6] = [
                (&cfg.hotkey_snip, Command::Snip),
                (&cfg.hotkey_pin, Command::PinClipboard),
                (&cfg.hotkey_hide_all, Command::HideAllPins),
                (&cfg.hotkey_restore, Command::RestoreClosed),
                (&cfg.hotkey_selection_translate, Command::SelectionTranslate),
                (&cfg.hotkey_capture_translate, Command::CaptureTranslate),
            ];
            let mut ids = Vec::new();
            let mut binds = Vec::new();
            let mut next = 0xA000i32;
            for (seq, cmd) in specs {
                if let Some(Hotkey { mods, vk }) = parse_hotkey(seq) {
                    if RegisterHotKey(
                        hwnd,
                        next,
                        HOT_KEY_MODIFIERS(mods | MOD_NOREPEAT),
                        vk,
                    )
                    .is_ok()
                    {
                        binds.push(HotkeyBind { id: next, cmd });
                        ids.push(next);
                    }
                }
                next += 1;
            }
            if !cfg.hotkey_clickthrough.trim().is_empty() {
                if let Some(Hotkey { mods, vk }) = parse_hotkey(&cfg.hotkey_clickthrough) {
                    if RegisterHotKey(hwnd, next, HOT_KEY_MODIFIERS(mods | MOD_NOREPEAT), vk).is_ok()
                    {
                        binds.push(HotkeyBind {
                            id: next,
                            cmd: Command::ClickThrough,
                        });
                        ids.push(next);
                    }
                }
            }
            *BINDS.lock().expect("binds") = binds;
            Ok(Self { hwnd, ids })
        }
    }
}

impl Drop for HotkeyHost {
    fn drop(&mut self) {
        unsafe {
            for id in &self.ids {
                let _ = UnregisterHotKey(self.hwnd, *id);
            }
            if IsWindow(self.hwnd).as_bool() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
        let _ = BINDS.lock().map(|mut b| b.clear());
    }
}

pub fn window_frame(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mut r = windows::Win32::Foundation::RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut _ as *mut _,
            std::mem::size_of::<windows::Win32::Foundation::RECT>() as u32,
        )
        .is_ok()
        {
            return Some((r.left, r.top, r.right - r.left, r.bottom - r.top));
        }
        if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut r).is_ok() {
            return Some((r.left, r.top, r.right - r.left, r.bottom - r.top));
        }
        None
    }
}

pub fn is_cloaked(hwnd: HWND) -> bool {
    unsafe {
        let mut v: u32 = 0;
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut v as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && v != 0
    }
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn pcw(buf: &[u16]) -> PCWSTR {
    PCWSTR(buf.as_ptr())
}

pub fn post_quit_like(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(hwnd, WM_DESTROY, WPARAM(0), LPARAM(0));
    }
}

/// F5: send Ctrl+C, read clipboard text, restore later via caller.
pub fn send_ctrl_c() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };
    unsafe {
        for vk in [0x12u16, 0x10, 0x5B, 0x5C] {
            if GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 {
                let up = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(vk),
                            dwFlags: KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                };
                let _ = SendInput(&[up], std::mem::size_of::<INPUT>() as i32);
            }
        }
        let n = SendInput(
            &[
                key(0x11, false),
                key(0x43, false),
                key(0x43, true),
                key(0x11, true),
            ],
            std::mem::size_of::<INPUT>() as i32,
        );
        if n != 4 {
            return Err("SendInput Ctrl+C failed".into());
        }
    }
    Ok(())
}

fn key(vk: u16, up: bool) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() },
                ..Default::default()
            },
        },
    }
}

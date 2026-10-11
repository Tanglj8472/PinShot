//! Always-on-top OCR / translation result window (Win32 EDIT + buttons).

use std::sync::{Mutex, OnceLock};

use pinshot_core::i18n::tr;
use pinshot_core::translate::{read_translation, request_body, request_url, TARGETS};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::UpdateWindow;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetDlgItem, GetWindowTextLengthW,
    GetWindowTextW, MoveWindow, PostMessageW, RegisterClassW, SendMessageW, SetWindowPos, SetWindowTextW,
    ShowWindow, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SW_SHOW, WM_COMMAND, WM_DESTROY, WM_SIZE,
    WNDCLASSW, WS_BORDER, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

use crate::clipboard;
use crate::win32::{pcw, wide};

const ID_ORIG: i32 = 101;
const ID_TRANS: i32 = 102;
const ID_DO_TR: i32 = 103;
const ID_COPY_O: i32 = 104;
const ID_COPY_T: i32 = 105;
const ID_LANG: i32 = 106;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_WANTRETURN: u32 = 0x1000;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const CB_ADDSTRING: u32 = 0x0143;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETCURSEL: u32 = 0x0147;

struct WinState {
    hwnd: isize,
}

static STATE: OnceLock<Mutex<Option<WinState>>> = OnceLock::new();

fn cell() -> &'static Mutex<Option<WinState>> {
    STATE.get_or_init(|| Mutex::new(None))
}

pub fn open(text: String, auto_translate: bool) {
    unsafe {
        let class = w!("PinShotOcrResult");
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
        let title = wide(&tr("PinShot OCR"));
        let hwnd = CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST
                | windows::Win32::UI::WindowsAndMessaging::WS_EX_TOOLWINDOW,
            class,
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            200,
            120,
            520,
            520,
            None,
            None,
            hinst,
            None,
        );
        let Ok(hwnd) = hwnd else { return };
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        create_children(hwnd);
        set_edit(hwnd, ID_ORIG, &text);
        layout(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);
        *cell().lock().expect("ocr win") = Some(WinState { hwnd: hwnd.0 as isize });
        if auto_translate && !text.trim().is_empty() {
            do_translate(hwnd);
        }
    }
}

unsafe fn create_children(hwnd: HWND) {
    let hinst = GetModuleHandleW(None).unwrap_or_default();
    let edit_style = WS_CHILD | WS_VISIBLE | WS_BORDER | WS_VSCROLL | WS_TABSTOP
        | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN);
    let _ = CreateWindowExW(
        Default::default(),
        w!("EDIT"),
        w!(""),
        edit_style,
        12,
        28,
        480,
        180,
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(ID_ORIG as isize as *mut _),
        hinst,
        None,
    );
    let _ = CreateWindowExW(
        Default::default(),
        w!("EDIT"),
        w!(""),
        edit_style,
        12,
        280,
        480,
        140,
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(ID_TRANS as isize as *mut _),
        hinst,
        None,
    );
    let _ = CreateWindowExW(
        Default::default(),
        w!("BUTTON"),
        PCWSTR(wide(&tr("Translate")).as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        160,
        216,
        100,
        28,
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(ID_DO_TR as isize as *mut _),
        hinst,
        None,
    );
    let _ = CreateWindowExW(
        Default::default(),
        w!("BUTTON"),
        PCWSTR(wide(&tr("Copy original")).as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        270,
        216,
        120,
        28,
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(ID_COPY_O as isize as *mut _),
        hinst,
        None,
    );
    let _ = CreateWindowExW(
        Default::default(),
        w!("BUTTON"),
        PCWSTR(wide(&tr("Copy translation")).as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        12,
        430,
        160,
        28,
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(ID_COPY_T as isize as *mut _),
        hinst,
        None,
    );
    let combo = CreateWindowExW(
        Default::default(),
        w!("COMBOBOX"),
        w!(""),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(CBS_DROPDOWNLIST),
        12,
        216,
        140,
        200,
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(ID_LANG as isize as *mut _),
        hinst,
        None,
    );
    if let Ok(cb) = combo {
        for (name, _) in TARGETS {
            let w = wide(name);
            SendMessageW(cb, CB_ADDSTRING, WPARAM(0), LPARAM(w.as_ptr() as isize));
        }
        SendMessageW(cb, CB_SETCURSEL, WPARAM(0), LPARAM(0));
    }
    let _ = CreateWindowExW(
        Default::default(),
        w!("STATIC"),
        PCWSTR(wide(&tr("Original (editable)")).as_ptr()),
        WS_CHILD | WS_VISIBLE,
        12,
        8,
        200,
        18,
        hwnd,
        None,
        hinst,
        None,
    );
    let _ = CreateWindowExW(
        Default::default(),
        w!("STATIC"),
        PCWSTR(wide(&tr("Translation")).as_ptr()),
        WS_CHILD | WS_VISIBLE,
        12,
        258,
        200,
        18,
        hwnd,
        None,
        hinst,
        None,
    );
}

unsafe fn layout(hwnd: HWND) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let w = (rc.right - 24).max(200);
    let _ = MoveWindow(GetDlgItem(hwnd, ID_ORIG).unwrap_or_default(), 12, 28, w, 180, true);
    let _ = MoveWindow(GetDlgItem(hwnd, ID_TRANS).unwrap_or_default(), 12, 280, w, 140, true);
}

fn edit_text(hwnd: HWND, id: i32) -> String {
    unsafe {
        let child = GetDlgItem(hwnd, id).unwrap_or_default();
        let n = GetWindowTextLengthW(child);
        if n <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; n as usize + 1];
        let got = GetWindowTextW(child, &mut buf);
        String::from_utf16_lossy(&buf[..got as usize])
    }
}

fn set_edit(hwnd: HWND, id: i32, text: &str) {
    unsafe {
        if let Ok(child) = GetDlgItem(hwnd, id) {
            let w = wide(text);
            let _ = SetWindowTextW(child, PCWSTR(w.as_ptr()));
        }
    }
}

fn current_target(hwnd: HWND) -> &'static str {
    unsafe {
        let cb = GetDlgItem(hwnd, ID_LANG).unwrap_or_default();
        let i = SendMessageW(cb, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as usize;
        TARGETS.get(i).map(|t| t.1).unwrap_or("zh-Hans")
    }
}

fn do_translate(hwnd: HWND) {
    let text = edit_text(hwnd, ID_ORIG);
    let target = current_target(hwnd).to_string();
    std::thread::spawn(move || {
        let result = (|| {
            let body = request_body(&text)?;
            let url = request_url(&target);
            let resp = ureq::post(&url)
                .set("Content-Type", "application/json")
                .set("Accept", "application/json")
                .set(
                    "User-Agent",
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Edg/128.0.0.0",
                )
                .timeout(std::time::Duration::from_secs(8))
                .send_string(&body)
                .map_err(|e| e.to_string())?;
            let s = resp.into_string().map_err(|e| e.to_string())?;
            read_translation(&s)
        })();
        // Marshal back via WM_COMMAND custom — just SetWindowText from this thread is OK for EDIT.
        if let Ok(g) = cell().lock() {
            if let Some(st) = g.as_ref() {
                let hwnd = HWND(st.hwnd as *mut _);
                match result {
                    Ok(t) => set_edit(hwnd, ID_TRANS, &t),
                    Err(e) => set_edit(hwnd, ID_TRANS, &e),
                }
            }
        }
    });
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            layout(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            match id {
                ID_DO_TR => do_translate(hwnd),
                ID_COPY_O => {
                    let _ = clipboard::write_text(&edit_text(hwnd, ID_ORIG));
                }
                ID_COPY_T => {
                    let _ = clipboard::write_text(&edit_text(hwnd, ID_TRANS));
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Ok(mut g) = cell().lock() {
                *g = None;
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub fn raise_above_pins() {
    if let Ok(g) = cell().lock() {
        if let Some(st) = g.as_ref() {
            unsafe {
                let _ = SetWindowPos(HWND(st.hwnd as *mut _), HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            }
        }
    }
}

/// F5: send Ctrl+C, read clipboard text, restore later via caller.
pub fn send_ctrl_c() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };
    unsafe {
        // Release Alt/Shift/Win if held so the synthetic Ctrl+C is clean.
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
        let down_ctrl = key(0x11, false);
        let down_c = key(0x43, false);
        let up_c = key(0x43, true);
        let up_ctrl = key(0x11, true);
        let n = SendInput(&[down_ctrl, down_c, up_c, up_ctrl], std::mem::size_of::<INPUT>() as i32);
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

pub fn close() {
    if let Ok(mut g) = cell().lock() {
        if let Some(st) = g.take() {
            unsafe {
                let _ = DestroyWindow(HWND(st.hwnd as *mut _));
            }
        }
    }
}

#[allow(dead_code)]
fn _post(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(hwnd, 0, WPARAM(0), LPARAM(0));
    }
}

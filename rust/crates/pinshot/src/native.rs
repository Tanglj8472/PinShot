//! Thin platform facade. Windows keeps the existing Win32 helpers; Linux uses X11.

use winit::event_loop::EventLoopProxy;
use winit::window::Window;

use crate::event::AppEvent;
use pinshot_core::Config;

#[cfg(windows)]
pub use crate::win32::HotkeyHost;

#[cfg(unix)]
pub use crate::x11util::HotkeyHost;

#[cfg(windows)]
pub type NativeWin = windows::Win32::Foundation::HWND;

#[cfg(not(windows))]
pub type NativeWin = ();

#[cfg(windows)]
pub fn hwnd_of(window: &Window) -> NativeWin {
    crate::win32::hwnd_of(window)
}

#[cfg(not(windows))]
pub fn hwnd_of(_window: &Window) -> NativeWin {}

pub fn init_process() {
    #[cfg(windows)]
    crate::win32::init_dpi();
    #[cfg(unix)]
    {
        if let Err(e) = gtk::init() {
            crate::logutil::log(&format!("gtk::init: {e}"));
        }
    }
}

pub fn apply_overlay_style(window: &Window) {
    #[cfg(windows)]
    crate::win32::apply_overlay_style(hwnd_of(window));
    #[cfg(unix)]
    crate::x11util::apply_overlay_style(window);
}

pub fn apply_pin_style(window: &Window) {
    #[cfg(windows)]
    crate::win32::apply_pin_style(hwnd_of(window));
    #[cfg(unix)]
    crate::x11util::apply_pin_style(window);
}

pub fn set_opacity(window: &Window, opacity: f32) {
    #[cfg(windows)]
    crate::win32::set_opacity(hwnd_of(window), opacity);
    #[cfg(unix)]
    crate::x11util::set_opacity(window, opacity);
}

pub fn set_click_through(window: &Window, on: bool, opacity: f32) {
    #[cfg(windows)]
    crate::win32::set_click_through(hwnd_of(window), on, opacity);
    #[cfg(unix)]
    crate::x11util::set_click_through(window, on, opacity);
}

pub fn raise_on_top(window: &Window) {
    #[cfg(unix)]
    crate::x11util::raise_on_top(window);
    let _ = window;
}

pub fn unmap_sync(window: &Window) {
    #[cfg(unix)]
    crate::x11util::unmap_sync(window);
    let _ = window;
}

pub fn install_hotkeys(
    proxy: EventLoopProxy<AppEvent>,
    cfg: &Config,
) -> Result<HotkeyHost, String> {
    #[cfg(windows)]
    {
        crate::win32::HotkeyHost::install(proxy, cfg).map_err(|e| e.to_string())
    }
    #[cfg(unix)]
    {
        crate::x11util::HotkeyHost::install(proxy, cfg)
    }
}

pub fn platform_name() -> &'static str {
    #[cfg(windows)]
    {
        "windows-rust"
    }
    #[cfg(unix)]
    {
        "linux-rust"
    }
    #[cfg(not(any(windows, unix)))]
    {
        "rust"
    }
}

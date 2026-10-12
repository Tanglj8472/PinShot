//! PinShot host. Windowing is behind a platform facade (`native`): Win32 on Windows,
//! X11 on Linux. `pinshot-core` stays portable.

mod app;
mod event;
mod clipboard;
mod draw;
mod ocr_run;
mod overlay;
mod pin;
mod pin_menu;
mod prefs;
mod prefs_win;
mod text;
mod toolbar;
mod tray;
mod fallback;
mod countdown;
mod logutil;
mod native;

#[cfg(windows)]
mod win32;
#[cfg(unix)]
mod x11util;

#[cfg(windows)]
mod autostart;
#[cfg(unix)]
#[path = "autostart_xdg.rs"]
mod autostart;

#[cfg(windows)]
mod capture;
#[cfg(unix)]
#[path = "capture_x11.rs"]
mod capture;

#[cfg(windows)]
mod detect;
#[cfg(unix)]
#[path = "detect_x11.rs"]
mod detect;

#[cfg(windows)]
mod dialog;
#[cfg(unix)]
#[path = "dialog_unix.rs"]
mod dialog;

#[cfg(windows)]
mod ipc;
#[cfg(unix)]
#[path = "ipc_unix.rs"]
mod ipc;

#[cfg(windows)]
mod result_win;
#[cfg(unix)]
#[path = "result_win_unix.rs"]
mod result_win;

pub use app::run;
#[cfg(windows)]
pub use prefs::run_preferences;

pub fn logutil_ensure() {
    crate::logutil::ensure_stdio();
}

pub fn platform_tag() -> &'static str {
    crate::native::platform_name()
}

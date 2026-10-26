//! PinShot host. Windowing is behind a platform facade (`native`): Win32 on Windows,
//! X11 on Linux. `pinshot-core` stays portable.

mod app;
mod automation;
mod cursor;
mod event;
mod clipboard;
mod draw;
#[cfg(not(feature = "lite"))]
mod ocr_run;
#[cfg(feature = "lite")]
#[path = "ocr_disabled.rs"]
mod ocr_run;
pub mod update_run;
mod overlay;
#[cfg(not(feature = "lite"))]
mod translate_run;
#[cfg(feature = "lite")]
#[path = "translate_disabled.rs"]
mod translate_run;
mod translate_popup;
mod pin;
mod pin_menu;
mod prefs;
mod prefs_win;
mod text;
mod ui_font;
mod toolbar;
mod tray;
mod fallback;
mod countdown;
mod logutil;
mod native;

#[cfg(all(feature = "lite", feature = "ocr"))]
compile_error!("lite cannot be combined with OCR features");

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

pub use app::run;
pub use prefs::run_preferences;

/// Capture the desktop with the application's native backend, without an overlay.
pub fn capture_to_file(path: &std::path::Path) -> Result<(), String> {
    native::init_process();
    let capture = capture::Capture::grab(false)?;
    dialog::save_raster(&capture.composed, &path.to_string_lossy(), 92)
}


/// Logging entry points shared by the GUI host and installer helper binary.
pub fn init_diagnostics(args: &[String]) {
    logutil::init(args);
}

pub fn flush_diagnostics() {
    logutil::flush();
}

pub fn report_runtime_error(context: &str, error: &str) {
    logutil::error(&format!("{context}: {error}"));
    logutil::flush();
}

pub fn logutil_ensure() {
    crate::logutil::ensure_stdio();
}

pub fn platform_tag() -> &'static str {
    crate::native::platform_name()
}

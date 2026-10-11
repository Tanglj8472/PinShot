//! Windows host for PinShot. Modules are `cfg(windows)` so `cargo test` works on Linux.

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod autostart;
#[cfg(windows)]
mod event;
#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod detect;
#[cfg(windows)]
mod dialog;
#[cfg(windows)]
mod draw;
#[cfg(windows)]
mod ipc;
#[cfg(windows)]
mod ocr_run;
#[cfg(windows)]
mod overlay;
#[cfg(windows)]
mod pin;
#[cfg(windows)]
mod prefs;
#[cfg(windows)]
mod result_win;
#[cfg(windows)]
mod text;
#[cfg(windows)]
mod toolbar;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod win32;

#[cfg(windows)]
pub use app::run;

#[cfg(not(windows))]
pub fn run(_args: Vec<String>) -> Result<(), String> {
    Err("windows only".into())
}

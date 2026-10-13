//! File log under the data dir; `--debug` / `PINSHOT_DEBUG` also keep stderr (and a
//! console on Windows release builds that use `windows_subsystem = "windows"`).

use pinshot_core::config::data_dir;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);
static DEBUG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn debug_enabled() -> bool {
    DEBUG.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn init(args: &[String]) {
    let debug = args.iter().any(|a| a == "--debug")
        || std::env::var("PINSHOT_DEBUG")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);
    DEBUG.store(debug, std::sync::atomic::Ordering::Relaxed);
    if debug {
        attach_console();
    }
    let dir = data_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("pinshot.log");
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(&path) {
        *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
    }
    log(&format!(
        "PinShot {} starting debug={debug} args={args:?}",
        pinshot_core::VERSION
    ));
    install_panic_hook();
}

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        log(&format!("PANIC: {info}\n{bt}"));
        prev(info);
    }));
}

pub fn ms(label: &str, started: Instant) {
    log(&format!("{label} {}ms", started.elapsed().as_millis()));
}

pub fn log(msg: &str) {
    let line = format!("[pinshot] {msg}");
    if debug_enabled() {
        eprintln!("{line}");
    }
    if let Ok(mut g) = FILE.lock() {
        if let Some(f) = g.as_mut() {
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        }
    }
}

fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            let _ = AllocConsole();
        }
    }
}

/// `--version` / `--help` must be visible even when the exe is a Windows GUI subsystem binary.
pub fn ensure_stdio() {
    attach_console();
}

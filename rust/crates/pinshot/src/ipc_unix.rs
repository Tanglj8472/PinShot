//! Single-instance lock + Unix socket (`$XDG_RUNTIME_DIR/pinshot.sock`).

use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use pinshot_core::hotkey::Command;
use winit::event_loop::EventLoopProxy;

use crate::event::AppEvent;

pub enum Boot {
    Primary(LockFile),
    Forwarded,
}

pub struct LockFile {
    _file: std::fs::File,
}

fn runtime_dir() -> PathBuf {
    if let Ok(p) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(p);
    }
    std::env::temp_dir()
}

fn sock_path() -> PathBuf {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    runtime_dir().join(format!("pinshot-{user}.sock"))
}

fn lock_path() -> PathBuf {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    runtime_dir().join(format!("pinshot-{user}.lock"))
}

pub fn parse_cli_cmd(args: &[String]) -> &'static str {
    if args.iter().any(|a| a == "--snip" || a == "snip") {
        "snip"
    } else if args.iter().any(|a| a == "--pin" || a == "--pin-clipboard" || a == "pin") {
        "pin"
    } else if args.iter().any(|a| a == "--prefs" || a == "prefs") {
        "prefs"
    } else if args.iter().any(|a| a == "--translate" || a == "translate") {
        "translate"
    } else if args.iter().any(|a| a == "--selection" || a == "selection") {
        "selection"
    } else if args.iter().any(|a| a == "--quit" || a == "quit") {
        "quit"
    } else {
        "raise"
    }
}

pub fn claim_or_forward(cmd: &str) -> Result<Boot, String> {
    let lp = lock_path();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lp)
        .map_err(|e| e.to_string())?;
    let fd = file.as_raw_fd();
    let rc = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        send(cmd)?;
        return Ok(Boot::Forwarded);
    }
    let _ = std::fs::remove_file(sock_path());
    Ok(Boot::Primary(LockFile { _file: file }))
}

pub fn send(cmd: &str) -> Result<(), String> {
    let payload = serde_json::json!({ "cmd": cmd }).to_string();
    let mut s = UnixStream::connect(sock_path()).map_err(|_| {
        "PinShot is running but the IPC socket is not ready".to_string()
    })?;
    s.write_all(payload.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn spawn_server(proxy: EventLoopProxy<AppEvent>) {
    let _ = std::thread::Builder::new().name("pinshot-ipc".into()).spawn(move || {
        let path = sock_path();
        let _ = std::fs::remove_file(&path);
        let Ok(listener) = UnixListener::bind(&path) else {
            return;
        };
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let mut buf = String::new();
            let _ = stream.read_to_string(&mut buf);
            let v: serde_json::Value = serde_json::from_str(&buf).unwrap_or(serde_json::json!({"cmd":"raise"}));
            let cmd = v.get("cmd").and_then(|x| x.as_str()).unwrap_or("raise");
            let ev = match cmd {
                "snip" => AppEvent::Hotkey(Command::Snip),
                "pin" => AppEvent::Hotkey(Command::PinClipboard),
                "pins" => AppEvent::Hotkey(Command::HideAllPins),
                "prefs" => AppEvent::Hotkey(Command::OpenSettings),
                "quit" => AppEvent::Hotkey(Command::Quit),
                "translate" => AppEvent::Hotkey(Command::CaptureTranslate),
                "selection" => AppEvent::Hotkey(Command::SelectionTranslate),
                other => AppEvent::Ipc(other.to_string()),
            };
            let _ = proxy.send_event(ev);
        }
    });
}

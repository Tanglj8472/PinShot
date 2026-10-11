//! Single-instance mutex + named pipe (`\\.\pipe\PinShotIpc`).

use std::ffi::c_void;

use pinshot_core::hotkey::Command;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_PIPE_CONNECTED, GENERIC_READ,
    GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FlushFileBuffers, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::Threading::CreateMutexW;
use winit::event_loop::EventLoopProxy;

use crate::event::AppEvent;
use crate::win32::wide;

const PIPE: &str = r"\\.\pipe\PinShotIpc";
const MUTEX: &str = r"Local\PinShot.SingleInstance";

pub enum Boot {
    Primary(HANDLE),
    Forwarded,
}

pub fn claim_or_forward(cmd: &str) -> Result<Boot, String> {
    unsafe {
        let name = wide(MUTEX);
        let mtx = CreateMutexW(None, true, PCWSTR(name.as_ptr())).map_err(|e| e.to_string())?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mtx);
            send(cmd)?;
            return Ok(Boot::Forwarded);
        }
        Ok(Boot::Primary(mtx))
    }
}

pub fn send(cmd: &str) -> Result<(), String> {
    let payload = serde_json::json!({ "cmd": cmd }).to_string();
    let bytes = payload.into_bytes();
    unsafe {
        let path = wide(PIPE);
        let h = CreateFileW(
            PCWSTR(path.as_ptr()),
            (GENERIC_READ.0 | GENERIC_WRITE.0) as u32,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .map_err(|_| "PinShot is running but the IPC pipe is not ready".to_string())?;
        let mut wrote = 0u32;
        WriteFile(h, Some(&bytes), Some(&mut wrote), None).map_err(|e| e.to_string())?;
        let _ = FlushFileBuffers(h);
        let _ = CloseHandle(h);
    }
    Ok(())
}

pub fn spawn_server(proxy: EventLoopProxy<AppEvent>) {
    let _ = std::thread::Builder::new().name("pinshot-ipc".into()).spawn(move || loop {
        if let Ok(msg) = accept_one() {
            let cmd = msg.get("cmd").and_then(|v| v.as_str()).unwrap_or("raise");
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

fn accept_one() -> Result<serde_json::Value, String> {
    unsafe {
        let path = wide(PIPE);
        let pipe = CreateNamedPipeW(
            PCWSTR(path.as_ptr()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            4096,
            4096,
            0,
            None,
        );
        if pipe == INVALID_HANDLE_VALUE {
            return Err("CreateNamedPipe failed".into());
        }
        let connected = ConnectNamedPipe(pipe, None);
        let ok = connected.is_ok() || GetLastError() == ERROR_PIPE_CONNECTED;
        if !ok {
            let _ = CloseHandle(pipe);
            return Err("connect".into());
        }
        let mut buf = [0u8; 4096];
        let mut n = 0u32;
        let _ = ReadFile(pipe, Some(&mut buf), Some(&mut n), None);
        let _ = DisconnectNamedPipe(pipe);
        let _ = CloseHandle(pipe);
        let text = String::from_utf8_lossy(&buf[..n as usize]);
        serde_json::from_str(&text).or_else(|_| Ok(serde_json::json!({"cmd":"raise"})))
    }
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

#[allow(dead_code)]
fn _unused(_: PWSTR, _: *mut c_void) {}

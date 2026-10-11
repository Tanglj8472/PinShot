//! HKCU\\...\\Run "PinShot" → this exe.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

use crate::win32::wide;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "PinShot";

pub fn exe_path() -> Result<String, String> {
    unsafe {
        let mut buf = [0u16; MAX_PATH as usize];
        let n = GetModuleFileNameW(None, &mut buf);
        if n == 0 {
            return Err("GetModuleFileNameW failed".into());
        }
        Ok(String::from_utf16_lossy(&buf[..n as usize]))
    }
}

pub fn is_enabled() -> bool {
    unsafe {
        let sub = wide(RUN);
        let mut key = Default::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), 0, KEY_READ, &mut key).is_err() {
            return false;
        }
        let name = wide(VALUE);
        let mut n = 0u32;
        let ok = RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, None, None, Some(&mut n)).is_ok();
        let _ = RegCloseKey(key);
        ok && n > 0
    }
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    unsafe {
        let sub = wide(RUN);
        let mut key = Default::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            0,
            w!(""),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
        .map_err(|e| e.to_string())?;
        let name = wide(VALUE);
        if on {
            let path = exe_path()?;
            let data = wide(&format!("\"{path}\""));
            let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
            RegSetValueExW(key, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(bytes)).map_err(|e| e.to_string())?;
        } else {
            let _ = RegDeleteValueW(key, PCWSTR(name.as_ptr()));
        }
        let _ = RegCloseKey(key);
        Ok(())
    }
}

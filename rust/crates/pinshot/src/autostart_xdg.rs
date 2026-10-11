//! XDG autostart: `~/.config/autostart/pinshot.desktop`.

use pinshot_core::config::xdg_autostart_dir;
use std::fs;
use std::path::PathBuf;

const DESKTOP_NAME: &str = "pinshot.desktop";

pub fn autostart_path() -> PathBuf {
    xdg_autostart_dir().join(DESKTOP_NAME)
}

pub fn exe_path() -> Result<String, String> {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .map_err(|e| e.to_string())
}

pub fn is_enabled() -> bool {
    autostart_path().is_file()
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    let path = autostart_path();
    if on {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let exec = exe_path()?;
        let body = format!(
            "[Desktop Entry]\nType=Application\nVersion=1.0\nName=PinShot\nComment=Screenshot and pin tool\nExec={exec}\nTerminal=false\nCategories=Utility;Graphics;\nX-GNOME-Autostart-enabled=true\n"
        );
        fs::write(&path, body).map_err(|e| e.to_string())?;
    } else if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

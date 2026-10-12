use pinshot_core::config::Config;
use pinshot_core::Command;

#[derive(Clone, Debug)]
pub enum AppEvent {
    Hotkey(Command),
    Tray(TrayCmd),
    PrefsApplied(Config),
    RaisePrefs,
    Ipc(String),
    OcrDone(String, bool),
    OpenPrefs,
    X11Key {
        keysym: u32,
        mods: u16,
        pressed: bool,
    },
    PinMenu(PinMenuAction),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PinMenuAction {
    Copy,
    Save,
    ToggleToolbar,
    ToggleClickThrough,
    ToggleShadow,
    Zoom(i32),
    Rotate(i32),
    FlipH,
    FlipV,
    Paste,
    ReplaceFile,
    MoveGroup(String),
    NewGroup,
    ViewInFolder,
    Close,
    Destroy,
    CopySize,
    ResetZoom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayCmd {
    Snip,
    Pin,
    HideAll,
    Settings,
    Quit,
    Translate,
    Restore,
    ToggleGroup(String),
}

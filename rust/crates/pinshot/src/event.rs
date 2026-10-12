use pinshot_core::config::Config;
use pinshot_core::ocr::OcrPage;
use pinshot_core::Command;
use winit::window::WindowId;

#[derive(Clone, Debug)]
pub enum AppEvent {
    Hotkey(Command),
    Tray(TrayCmd),
    PrefsApplied(Config),
    RaisePrefs,
    Ipc(String),
    OcrDone {
        pin: WindowId,
        result: Result<OcrPage, String>,
        translate: bool,
    },
    TranslateDone {
        pin: WindowId,
        result: Result<String, String>,
    },
    #[cfg_attr(not(windows), allow(dead_code))]
    SelectionText(Result<String, String>),
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
    NewGroupNamed(String),
    RenameGroup,
    RenameGroupNamed(String),
    ViewInFolder,
    Close,
    Destroy,
    CopySize,
    ResetZoom,
    ToggleSmooth,
    ToggleGrayscale,
    ToggleInvert,
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

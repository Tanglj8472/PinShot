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

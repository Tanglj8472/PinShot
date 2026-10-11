use pinshot_core::Command;

#[derive(Clone, Debug)]
pub enum AppEvent {
    Hotkey(Command),
    Tray(TrayCmd),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCmd {
    Snip,
    Pin,
    HideAll,
    Settings,
    Quit,
}

use pinshot_core::groups;
use pinshot_core::i18n::tr;
use pinshot_core::icons::app_icon_rgba;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::event::TrayCmd;

pub struct Tray {
    _icon: TrayIcon,
    snip: MenuItem,
    pin: MenuItem,
    translate: MenuItem,
    hide: MenuItem,
    restore: MenuItem,
    prefs: MenuItem,
    quit: MenuItem,
    groups: Vec<(MenuItem, String)>,
}

impl Tray {
    pub fn new(group_vis: &[(String, bool)]) -> Result<Self, String> {
        let (w, h, rgba) = app_icon_rgba(32);
        let icon = Icon::from_rgba(rgba, w, h).map_err(|e| e.to_string())?;
        let menu = Menu::new();
        let snip = MenuItem::new(tr("Snip"), true, None);
        let pin = MenuItem::new(tr("Pin clipboard"), true, None);
        let translate = MenuItem::new(tr("Snip and translate"), true, None);
        let hide = MenuItem::new(tr("Hide/Show all pins"), true, None);
        let restore = MenuItem::new(tr("Restore last closed pin"), true, None);
        let prefs = MenuItem::new(tr("Preferences…"), true, None);
        let quit = MenuItem::new(tr("Quit"), true, None);
        menu.append(&snip).map_err(|e| e.to_string())?;
        menu.append(&pin).map_err(|e| e.to_string())?;
        menu.append(&translate).map_err(|e| e.to_string())?;
        menu.append(&hide).map_err(|e| e.to_string())?;
        menu.append(&restore).map_err(|e| e.to_string())?;
        let gm = Submenu::new(tr("Groups"), true);
        let mut groups = Vec::new();
        for (name, vis) in group_vis {
            let label = format!(
                "{}{}",
                if *vis { tr("Hide ") } else { tr("Show ") },
                groups::label(name)
            );
            let it = MenuItem::new(label, true, None);
            gm.append(&it).map_err(|e| e.to_string())?;
            groups.push((it, name.clone()));
        }
        menu.append(&gm).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator()).map_err(|e| e.to_string())?;
        menu.append(&prefs).map_err(|e| e.to_string())?;
        menu.append(&quit).map_err(|e| e.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("PinShot")
            .with_icon(icon)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            _icon: tray,
            snip,
            pin,
            translate,
            hide,
            restore,
            prefs,
            quit,
            groups,
        })
    }

    pub fn poll(&self) -> Option<TrayCmd> {
        // Menu ids first and drain the queue. Windows emits a tray Click when a
        // context item is chosen; treating any-button Click as Snip made every
        // item start a capture (Python only snips on Trigger = left click).
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == self.snip.id() {
                return Some(TrayCmd::Snip);
            }
            if ev.id == self.pin.id() {
                return Some(TrayCmd::Pin);
            }
            if ev.id == self.translate.id() {
                return Some(TrayCmd::Translate);
            }
            if ev.id == self.hide.id() {
                return Some(TrayCmd::HideAll);
            }
            if ev.id == self.restore.id() {
                return Some(TrayCmd::Restore);
            }
            if ev.id == self.prefs.id() {
                return Some(TrayCmd::Settings);
            }
            if ev.id == self.quit.id() {
                return Some(TrayCmd::Quit);
            }
            for (it, name) in &self.groups {
                if ev.id == it.id() {
                    return Some(TrayCmd::ToggleGroup(name.clone()));
                }
            }
        }
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            match ev {
                TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: tray_icon::MouseButton::Left,
                    ..
                } => {
                    return Some(TrayCmd::Snip);
                }
                _ => {}
            }
        }
        None
    }
}

//! Windows settings use a helper process: winit permits one event loop per process.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, RichText, Visuals};
use pinshot_core::config::{self, Config};
use pinshot_core::groups;
use pinshot_core::hotkey::{bindings_from_config, find_conflicts, normalize_seq};
use pinshot_core::i18n::{self, tr, Lang};
use winit::event_loop::EventLoopProxy;

use crate::event::AppEvent;

static OPEN: AtomicBool = AtomicBool::new(false);

/// Logical size of the Windows settings dialog (egui points).
///
/// Python `PrefsDialog` is `setMinimumWidth(440)` + `adjustSize()`, so the
/// General tab with Advanced collapsed is a short landscape window. The old
/// 500×460 inner size was nearly square and left a large empty band under the
/// few General rows.
const PREFS_W: f32 = 480.0;
const PREFS_MIN_W: f32 = 440.0;
const PREFS_MIN_H: f32 = 240.0;
const FORM_LABEL_W: f32 = 168.0;

fn prefs_inner_size(tab: usize, advanced: bool) -> egui::Vec2 {
    let h = match (tab, advanced) {
        (0, true) => 560.0,
        (0, false) => 292.0,
        (1, _) => 392.0,
        _ => 248.0,
    };
    egui::vec2(PREFS_W, h)
}

pub fn is_open() -> bool {
    OPEN.load(Ordering::SeqCst)
}

pub fn mark_closed() {
    OPEN.store(false, Ordering::SeqCst);
}

pub fn open(cfg: Config, proxy: EventLoopProxy<AppEvent>) {
    crate::logutil::log("prefs: open requested");
    #[cfg(unix)]
    {
        let _ = cfg;
        let _ = proxy.send_event(AppEvent::OpenPrefs);
    }
    #[cfg(not(unix))]
    {
        let _ = (cfg, proxy);
        static CHILD: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);
        let mut child = CHILD.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(proc) = child.as_mut() {
            if matches!(proc.try_wait(), Ok(None)) {
                raise_process_window(proc.id());
                return;
            }
        }
        match std::env::current_exe().and_then(|exe| {
            std::process::Command::new(exe).arg("--settings-window").spawn()
        }) {
            Ok(proc) => *child = Some(proc),
            Err(e) => crate::logutil::log(&format!("settings launch: {e}")),
        }
    }
}

#[cfg(windows)]
fn raise_process_window(pid: u32) {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::*;
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == data.0 as u32 && IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
            return BOOL(0);
        }
        BOOL(1)
    }
    unsafe { let _ = EnumWindows(Some(visit), LPARAM(pid as isize)); }
}

#[cfg(windows)]
pub fn run_preferences() {
    crate::logutil::init(&[]);
    crate::native::init_process();
    let cfg = config::load().unwrap_or_default();
    i18n::set_lang(cfg.lang());
    let size = prefs_inner_size(0, false);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(tr("PinShot Settings"))
            .with_inner_size([size.x, size.y])
            .with_min_inner_size([PREFS_MIN_W, PREFS_MIN_H]),
        persist_window: false,
        centered: true,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native("PinShot Settings", opts, Box::new(move |cc| {
        if let Ok(font) = std::fs::read(std::path::PathBuf::from(
            std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into())
        ).join("Fonts/msyh.ttc")) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert("cjk".into(), egui::FontData::from_owned(font).into());
            fonts.families.entry(egui::FontFamily::Proportional).or_default().push("cjk".into());
            cc.egui_ctx.set_fonts(fonts);
        }
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(16.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        cc.egui_ctx.set_style(style);
        apply_theme(&cc.egui_ctx, &cfg.theme);
        Ok(Box::new(PrefsApp {
            draft: cfg,
            tab: 0,
            advanced: false,
            group_sel: 0,
            group_edit: String::new(),
            status: String::new(),
            size_key: (0, false),
        }))
    })) {
        crate::logutil::log(&format!("settings window: {e}"));
    }
}

fn apply_theme(ctx: &egui::Context, theme: &str) {
    let dark = match theme {
        "dark" => true,
        "light" => false,
        _ => system_dark(),
    };
    ctx.set_visuals(if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    });
}

fn system_dark() -> bool {
    // HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize AppsUseLightTheme
    #[cfg(windows)]
    {
        use windows::Win32::System::Registry::{
            RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
        };
        use windows::core::w;
        unsafe {
            let mut key = Default::default();
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                0,
                KEY_READ,
                &mut key,
            )
            .is_err()
            {
                return false;
            }
            let mut ty = REG_DWORD;
            let mut val: u32 = 1;
            let mut n = std::mem::size_of::<u32>() as u32;
            let _ = RegQueryValueExW(
                key,
                w!("AppsUseLightTheme"),
                None,
                Some(&mut ty),
                Some((&mut val as *mut u32).cast()),
                Some(&mut n),
            );
            let _ = windows::Win32::System::Registry::RegCloseKey(key);
            return val == 0;
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

struct PrefsApp {
    draft: Config,
    tab: usize,
    advanced: bool,
    group_sel: usize,
    group_edit: String,
    status: String,
    size_key: (usize, bool),
}

impl PrefsApp {
    fn apply(&mut self) -> bool {
        self.draft.hotkey_snip = normalize_seq(&self.draft.hotkey_snip);
        self.draft.hotkey_pin = normalize_seq(&self.draft.hotkey_pin);
        self.draft.hotkey_hide_all = normalize_seq(&self.draft.hotkey_hide_all);
        self.draft.hotkey_restore = normalize_seq(&self.draft.hotkey_restore);
        self.draft.hotkey_clickthrough = normalize_seq(&self.draft.hotkey_clickthrough);
        self.draft.hotkey_selection_translate = normalize_seq(&self.draft.hotkey_selection_translate);
        self.draft.hotkey_capture_translate = normalize_seq(&self.draft.hotkey_capture_translate);
        let conflicts = find_conflicts(&bindings_from_config(&self.draft));
        if !conflicts.is_empty() {
            self.status = format!(
                "{}: {} / {}",
                tr("Conflicting hotkeys"),
                conflicts[0].0,
                conflicts[0].1
            );
            return false;
        }
        self.draft = self.draft.clone().validate();
        if let Err(e) = config::save(&self.draft) {
            crate::logutil::log(&format!("save settings: {e}"));
            self.status = tr("Could not save settings");
            return false;
        }
        i18n::set_lang(self.draft.lang());
        #[cfg(windows)]
        if let Err(e) = crate::ipc::send("reload-settings") {
            crate::logutil::log(&format!("apply settings: {e}"));
            self.status = tr("Saved. Restart PinShot to apply.");
            return false;
        }
        self.status = tr("Settings applied.");
        true
    }
}

impl eframe::App for PrefsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(tr("PinShot Settings")));
        apply_theme(ctx, &self.draft.theme);
        let key = (self.tab, self.advanced);
        if self.size_key != key {
            self.size_key = key;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(prefs_inner_size(
                key.0, key.1,
            )));
        }
        egui::TopBottomPanel::bottom("btns")
            .frame(
                egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if !self.status.is_empty() {
                        ui.colored_label(Color32::from_rgb(180, 80, 40), &self.status);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr("Cancel")).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button(tr("OK")).clicked() {
                            if self.apply() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        if ui.button(tr("Apply")).clicked() {
                            self.apply();
                        }
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin::same(12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (i, name) in [tr("General"), tr("Hotkeys"), tr("Output")].iter().enumerate()
                    {
                        if ui.selectable_label(self.tab == i, name).clicked() {
                            self.tab = i;
                        }
                    }
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                match self.tab {
                    0 => self.ui_general(ui),
                    1 => self.ui_hotkeys(ui),
                    _ => self.ui_output(ui),
                }
            });
    }
}

impl PrefsApp {
    fn ui_general(&mut self, ui: &mut egui::Ui) {
        form_combo(ui, tr("Language:"), "lang", self.draft.lang().display_name(), |ui| {
            ui.selectable_value(
                &mut self.draft.language,
                "zh_CN".into(),
                Lang::ZhCn.display_name(),
            );
            ui.selectable_value(
                &mut self.draft.language,
                "en".into(),
                Lang::En.display_name(),
            );
        });
        form_combo(
            ui,
            tr("Theme:"),
            "theme",
            match self.draft.theme.as_str() {
                "light" => tr("Light"),
                "dark" => tr("Dark"),
                _ => tr("System"),
            },
            |ui| {
                ui.selectable_value(&mut self.draft.theme, "system".into(), tr("System"));
                ui.selectable_value(&mut self.draft.theme, "light".into(), tr("Light"));
                ui.selectable_value(&mut self.draft.theme, "dark".into(), tr("Dark"));
            },
        );
        ui.checkbox(&mut self.draft.run_at_login, tr("Run at login"));
        let mut secs = self.draft.snip_delay;
        form_combo(
            ui,
            tr("Snip delay:"),
            "delay",
            if secs == 0 {
                tr("None (0 s)")
            } else {
                tr(&format!("{secs} s"))
            },
            |ui| {
                for s in [0, 1, 2, 3, 5] {
                    ui.selectable_value(
                        &mut secs,
                        s,
                        if s == 0 {
                            tr("None (0 s)")
                        } else {
                            tr(&format!("{s} s"))
                        },
                    );
                }
            },
        );
        self.draft.snip_delay = secs;

        ui.add_space(4.0);
        let body = egui::CollapsingHeader::new(tr("Advanced"))
            .id_salt("prefs_advanced")
            .open(Some(self.advanced))
            .show(ui, |ui| {
                let mut confirm = !self.draft.skip_abort_confirm;
                ui.checkbox(&mut confirm, tr("Confirm before abort (Esc)"));
                self.draft.skip_abort_confirm = !confirm;
                form_combo(
                    ui,
                    tr("Double-click pin:"),
                    "dbl",
                    match self.draft.dblclick_pin_action.as_str() {
                        "hide" => tr("Hide pin"),
                        "none" => tr("Do nothing"),
                        _ => tr("Close pin"),
                    },
                    |ui| {
                        ui.selectable_value(
                            &mut self.draft.dblclick_pin_action,
                            "close".into(),
                            tr("Close pin"),
                        );
                        ui.selectable_value(
                            &mut self.draft.dblclick_pin_action,
                            "hide".into(),
                            tr("Hide pin"),
                        );
                        ui.selectable_value(
                            &mut self.draft.dblclick_pin_action,
                            "none".into(),
                            tr("Do nothing"),
                        );
                    },
                );
                form_slider(ui, tr("Snip history:"), &mut self.draft.history_max, 1..=50);
                form_slider(ui, tr("Closed pins:"), &mut self.draft.closed_pins_max, 0..=50);
                ui.label(tr("Pin groups:"));
                let groups = self.draft.groups.clone();
                egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                    for (i, g) in groups.iter().enumerate() {
                        if ui
                            .selectable_label(self.group_sel == i, groups::label(g))
                            .clicked()
                        {
                            self.group_sel = i;
                            self.group_edit = g.clone();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.group_edit)
                            .desired_width(ui.available_width() - 180.0),
                    );
                    if ui.button(tr("Add")).clicked() {
                        let _ = groups::ensure(&mut self.draft.groups, &self.group_edit);
                    }
                    if ui.button(tr("Rename")).clicked() {
                        if let Some(old) = self.draft.groups.get(self.group_sel).cloned() {
                            match groups::rename(&mut self.draft.groups, &old, &self.group_edit) {
                                Ok(_) => {}
                                Err(e) => self.status = tr(&e),
                            }
                        }
                    }
                    if ui.button(tr("Remove")).clicked() {
                        if let Some(name) = self.draft.groups.get(self.group_sel).cloned() {
                            if let Err(e) = groups::remove(&mut self.draft.groups, &name) {
                                self.status = tr(&e);
                            }
                            self.group_sel = 0;
                        }
                    }
                });
            });
        if body.header_response.clicked() {
            self.advanced = !self.advanced;
        }
    }

    fn ui_hotkeys(&mut self, ui: &mut egui::Ui) {
        form_line(ui, tr("Snip:"), &mut self.draft.hotkey_snip);
        form_line(ui, tr("Pin clipboard:"), &mut self.draft.hotkey_pin);
        form_line(ui, tr("Hide/show pins:"), &mut self.draft.hotkey_hide_all);
        form_line(ui, tr("Restore last pin:"), &mut self.draft.hotkey_restore);
        form_line(
            ui,
            tr("Select-text translate:"),
            &mut self.draft.hotkey_selection_translate,
        );
        form_line(
            ui,
            tr("Snip and translate:"),
            &mut self.draft.hotkey_capture_translate,
        );
        form_line(ui, tr("Click-through:"), &mut self.draft.hotkey_clickthrough);
        let conflicts = find_conflicts(&bindings_from_config(&self.draft));
        if !conflicts.is_empty() {
            ui.colored_label(
                Color32::from_rgb(200, 60, 40),
                RichText::new(format!(
                    "{}: {} + {}",
                    tr("Conflicting hotkeys"),
                    conflicts[0].0,
                    conflicts[0].1
                )),
            );
        }
        ui.add_space(8.0);
        ui.weak(tr(
            "Hotkeys already used by another app cannot be bound. Click Apply to rebind.",
        ));
    }

    fn ui_output(&mut self, ui: &mut egui::Ui) {
        form_line(ui, tr("Save folder:"), &mut self.draft.quick_save_folder);
        form_slider(ui, tr("Quality:"), &mut self.draft.image_quality, 1..=100);
    }
}

fn form_combo(
    ui: &mut egui::Ui,
    label: String,
    id: &'static str,
    selected: impl Into<egui::WidgetText>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        form_label(ui, &label);
        let w = ui.available_width().max(160.0);
        egui::ComboBox::from_id_salt(id)
            .width(w)
            .selected_text(selected)
            .show_ui(ui, add_contents);
    });
}

fn form_line(ui: &mut egui::Ui, label: String, val: &mut String) {
    ui.horizontal(|ui| {
        form_label(ui, &label);
        ui.add(egui::TextEdit::singleline(val).desired_width(ui.available_width()));
    });
}

fn form_slider<Num: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: String,
    val: &mut Num,
    range: std::ops::RangeInclusive<Num>,
) {
    ui.horizontal(|ui| {
        form_label(ui, &label);
        let w = ui.available_width().max(120.0);
        ui.add_sized(
            [w, ui.spacing().interact_size.y],
            egui::Slider::new(val, range),
        );
    });
}

fn form_label(ui: &mut egui::Ui, label: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(FORM_LABEL_W, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(label);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_tab_is_landscape_compact() {
        let sz = prefs_inner_size(0, false);
        assert!(sz.x >= PREFS_MIN_W);
        assert!(sz.y >= PREFS_MIN_H);
        assert!(FORM_LABEL_W >= 120.0);
        assert!(
            sz.y <= 320.0,
            "collapsed General should hug content, got h={}",
            sz.y
        );
        assert!(
            sz.x > sz.y,
            "General collapsed must be landscape (Python adjustSize), got {:?}",
            sz
        );
        assert!(
            sz.y < 400.0,
            "old 500x460 inner size left a large empty band"
        );
    }

    #[test]
    fn window_grows_for_advanced_and_hotkeys() {
        let general = prefs_inner_size(0, false);
        let advanced = prefs_inner_size(0, true);
        let hotkeys = prefs_inner_size(1, false);
        let output = prefs_inner_size(2, false);
        assert!(advanced.y > general.y);
        assert!(hotkeys.y > general.y);
        assert!(output.y <= general.y);
        assert!(advanced.y < 640.0);
        assert_eq!(general.x, PREFS_W);
        assert_eq!(hotkeys.x, PREFS_W);
    }
}

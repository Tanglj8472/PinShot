//! Settings window (egui). Spawned as `--settings-window` so it has its own
//! event loop: eframe on a second thread after `gtk::init` fails on Linux, and
//! winit only allows one loop per process on Windows.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, RichText, Ui, Visuals};
use pinshot_core::config::{self, Config};
use pinshot_core::groups;
use pinshot_core::hotkey::{bindings_from_config, find_conflicts, normalize_seq};
use pinshot_core::i18n::{self, tr, Lang};
use winit::event_loop::EventLoopProxy;

use crate::event::AppEvent;

static OPEN: AtomicBool = AtomicBool::new(false);

/// Snipaste-like compact landscape dialog. General (Advanced collapsed) hugs
/// the form instead of leaving a square empty band.
const PREFS_W: f32 = 520.0;
const PREFS_MIN_W: f32 = 460.0;
const PREFS_MIN_H: f32 = 260.0;
const FORM_LABEL_W: f32 = 168.0;
const ROW_H: f32 = 28.0;

fn prefs_inner_size(tab: usize, advanced: bool) -> egui::Vec2 {
    let h = match (tab, advanced) {
        (0, true) => 620.0,
        (0, false) => 318.0,
        (1, _) => 420.0,
        _ => 268.0,
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
    let _ = (cfg, &proxy);
    static CHILD: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);
    let mut child = CHILD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(proc) = child.as_mut() {
        if matches!(proc.try_wait(), Ok(None)) {
            raise_settings_window(proc.id());
            return;
        }
    }
    match std::env::current_exe().and_then(|exe| {
        std::process::Command::new(exe).arg("--settings-window").spawn()
    }) {
        Ok(proc) => {
            *child = Some(proc);
            OPEN.store(true, Ordering::SeqCst);
        }
        Err(e) => {
            crate::logutil::log(&format!("settings launch: {e}"));
            let _ = proxy.send_event(AppEvent::OpenPrefs);
        }
    }
}

fn raise_settings_window(pid: u32) {
    #[cfg(windows)]
    {
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
        unsafe {
            let _ = EnumWindows(Some(visit), LPARAM(pid as isize));
        }
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("xdotool")
            .args(["search", "--pid", &pid.to_string(), "windowactivate"])
            .status();
    }
}

pub fn run_preferences() {
    crate::logutil::init(&[]);
    // Do not gtk::init here: eframe/winit on the same process after GTK
    // init is what made the old Linux settings window fail silently.
    #[cfg(windows)]
    crate::native::init_process();
    let cfg = config::load().unwrap_or_default();
    i18n::set_lang(cfg.lang());
    let size = prefs_inner_size(0, false);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(tr("PinShot Settings"))
            .with_inner_size([size.x, size.y])
            .with_min_inner_size([PREFS_MIN_W, PREFS_MIN_H])
            .with_always_on_top(),
        persist_window: false,
        centered: true,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "PinShot Settings",
        opts,
        Box::new(move |cc| {
            crate::ui_font::install_cjk(&cc.egui_ctx);
            let mut style = (*cc.egui_ctx.style()).clone();
            style.spacing.item_spacing = egui::vec2(16.0, 8.0);
            style.spacing.button_padding = egui::vec2(12.0, 5.0);
            style.spacing.interact_size.y = ROW_H;
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
        }),
    ) {
        crate::logutil::log(&format!("settings window: {e}"));
    }
    OPEN.store(false, Ordering::SeqCst);
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
    #[cfg(windows)]
    {
        use windows::core::w;
        use windows::Win32::System::Registry::{
            RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
        };
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
        std::env::var("GTK_THEME")
            .or_else(|_| std::env::var("GTK_THEME_VARIANT"))
            .map(|s| s.to_ascii_lowercase().contains("dark"))
            .unwrap_or(false)
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
        match crate::ipc::send("reload-settings") {
            Ok(()) => self.status = tr("Settings applied."),
            Err(e) => {
                crate::logutil::log(&format!("apply settings: {e}"));
                self.status = tr("Saved. Restart PinShot to apply.");
            }
        }
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
            .exact_height(48.0)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(16, 10)),
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
                        ui.add_space(6.0);
                        if ui.button(tr("OK")).clicked() {
                            if self.apply() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        ui.add_space(6.0);
                        if ui.button(tr("Apply")).clicked() {
                            self.apply();
                        }
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(12.0, 8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (i, name) in [tr("General"), tr("Hotkeys"), tr("Output")]
                        .iter()
                        .enumerate()
                    {
                        let selected = self.tab == i;
                        let btn = egui::SelectableLabel::new(selected, name.as_str());
                        if ui.add_sized([108.0, 28.0], btn).clicked() {
                            self.tab = i;
                        }
                    }
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(10.0);
                match self.tab {
                    0 => self.ui_general(ui),
                    1 => self.ui_hotkeys(ui),
                    _ => self.ui_output(ui),
                }
            });
    }
}

impl PrefsApp {
    fn ui_general(&mut self, ui: &mut Ui) {
        egui::Grid::new("prefs_general")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .min_col_width(FORM_LABEL_W)
            .min_row_height(ROW_H)
            .show(ui, |ui| {
                form_combo(
                    ui,
                    tr("Language:"),
                    "lang",
                    self.draft.lang().display_name(),
                    |ui| {
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
                    },
                );
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
                ui.label("");
                ui.checkbox(&mut self.draft.run_at_login, tr("Run at login"))
                    .on_hover_text(tr("Start PinShot when you log in"));
                ui.end_row();
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
            });

        ui.add_space(8.0);
        let body = egui::CollapsingHeader::new(tr("Advanced"))
            .id_salt("prefs_advanced")
            .open(Some(self.advanced))
            .show(ui, |ui| {
                ui.add_space(6.0);
                egui::Grid::new("prefs_advanced_grid")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .min_col_width(FORM_LABEL_W)
                    .min_row_height(ROW_H)
                    .show(ui, |ui| {
                        let mut confirm = !self.draft.skip_abort_confirm;
                        ui.label("");
                        ui.checkbox(&mut confirm, tr("Confirm before abort (Esc)"));
                        self.draft.skip_abort_confirm = !confirm;
                        ui.end_row();
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
                        form_slider(
                            ui,
                            tr("Closed pins:"),
                            &mut self.draft.closed_pins_max,
                            0..=50,
                        );
                    });
                ui.add_space(8.0);
                ui.label(tr("Pin groups:"));
                let groups = self.draft.groups.clone();
                egui::ScrollArea::vertical()
                    .max_height(100.0)
                    .show(ui, |ui| {
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
                            .desired_width(ui.available_width() - 200.0),
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

    fn ui_hotkeys(&mut self, ui: &mut Ui) {
        egui::Grid::new("prefs_hotkeys")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .min_col_width(FORM_LABEL_W)
            .min_row_height(ROW_H)
            .show(ui, |ui| {
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
            });
        let conflicts = find_conflicts(&bindings_from_config(&self.draft));
        if !conflicts.is_empty() {
            ui.add_space(8.0);
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
        ui.add_space(10.0);
        ui.weak(tr(
            "Hotkeys already used by another app cannot be bound. Click Apply to rebind.",
        ));
    }

    fn ui_output(&mut self, ui: &mut Ui) {
        egui::Grid::new("prefs_output")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .min_col_width(FORM_LABEL_W)
            .min_row_height(ROW_H)
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_min_width(FORM_LABEL_W);
                    ui.label(tr("Save folder:"));
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.draft.quick_save_folder)
                            .desired_width(ui.available_width() - 88.0),
                    );
                    if ui.button(tr("Browse…")).clicked() {
                        if let Some(p) = crate::dialog::open_folder() {
                            self.draft.quick_save_folder = p;
                        }
                    }
                });
                ui.end_row();
                form_slider(ui, tr("Quality:"), &mut self.draft.image_quality, 1..=100);
            });
    }
}

fn form_combo(
    ui: &mut Ui,
    label: String,
    id: &'static str,
    selected: impl Into<egui::WidgetText>,
    add_contents: impl FnOnce(&mut Ui),
) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.set_min_width(FORM_LABEL_W);
        ui.label(label);
    });
    let w = ui.available_width().max(180.0);
    egui::ComboBox::from_id_salt(id)
        .width(w)
        .selected_text(selected)
        .show_ui(ui, add_contents);
    ui.end_row();
}

fn form_line(ui: &mut Ui, label: String, val: &mut String) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.set_min_width(FORM_LABEL_W);
        ui.label(label);
    });
    ui.add(egui::TextEdit::singleline(val).desired_width(ui.available_width()).min_size(egui::vec2(0.0, ROW_H)));
    ui.end_row();
}

fn form_slider<Num: egui::emath::Numeric>(
    ui: &mut Ui,
    label: String,
    val: &mut Num,
    range: std::ops::RangeInclusive<Num>,
) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.set_min_width(FORM_LABEL_W);
        ui.label(label);
    });
    let w = ui.available_width().max(160.0);
    ui.add_sized([w, ROW_H], egui::Slider::new(val, range));
    ui.end_row();
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
            sz.y <= 340.0,
            "collapsed General should hug content, got h={}",
            sz.y
        );
        assert!(
            sz.x > sz.y,
            "General collapsed must be landscape (Snipaste-like hug), got {:?}",
            sz
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
        assert!(advanced.y < 700.0);
        assert_eq!(general.x, PREFS_W);
        assert_eq!(hotkeys.x, PREFS_W);
    }

    #[test]
    fn form_label_column_is_aligned() {
        assert_eq!(FORM_LABEL_W, 168.0);
        assert!(ROW_H >= 24.0);
    }
}

//! Settings window (egui). Spawned as `--settings-window` so it has its own
//! event loop: eframe on a second thread after `gtk::init` fails on Linux, and
//! winit only allows one loop per process on Windows.
//!
//! Tab names and page layout follow Snipaste Preferences (public UI):
//! General / Interface / Snip / Paste / Output / Control / About.
//! PinShot-only keys stay; no Snipaste-only options are invented.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, RichText, Ui, Visuals};
use pinshot_core::config::{self, Config};
use pinshot_core::groups;
use pinshot_core::hotkey::{bindings_from_config, find_conflicts, normalize_seq};
use pinshot_core::i18n::{self, tr, Lang};
use pinshot_core::VERSION;
use winit::event_loop::EventLoopProxy;

use crate::event::AppEvent;

static OPEN: AtomicBool = AtomicBool::new(false);

/// Window hugs the form: width is the tab strip + fields, height is measured
/// from the widgets each frame (not a leftover empty square).
const PREFS_W: f32 = 480.0;
const PREFS_MIN_W: f32 = 420.0;
const PREFS_MIN_H: f32 = 100.0;
const PREFS_MAX_H: f32 = 320.0;
const FOOTER_H: f32 = 32.0;
const BODY_MARGIN_Y: f32 = 8.0;
const SIZE_SLACK: f32 = 18.0;
const FORM_LABEL_W: f32 = 104.0;
const ROW_H: f32 = 20.0;
const CONTROL_W: f32 = 260.0;

fn prefs_inner_size(body_h: f32) -> egui::Vec2 {
    let h = (body_h + FOOTER_H + BODY_MARGIN_Y + SIZE_SLACK).clamp(PREFS_MIN_H, PREFS_MAX_H);
    egui::vec2(PREFS_W, h)
}

const TABS: &[&str] = &[
    "General",
    "Interface",
    "Snip",
    "Paste",
    "Output",
    "Control",
    "About",
];

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
    let size = prefs_inner_size(72.0);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(tr("PinShot Preferences"))
            .with_inner_size([size.x, size.y])
            .with_min_inner_size([PREFS_MIN_W, PREFS_MIN_H])
            .with_max_inner_size([PREFS_W, PREFS_MAX_H])
            .with_resizable(false)
            .with_always_on_top(),
        persist_window: false,
        centered: true,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "PinShot Preferences",
        opts,
        Box::new(move |cc| {
            crate::ui_font::install_cjk(&cc.egui_ctx);
            let mut style = (*cc.egui_ctx.style()).clone();
            style.spacing.item_spacing = egui::vec2(4.0, 3.0);
            style.spacing.button_padding = egui::vec2(6.0, 2.0);
            style.spacing.interact_size.y = ROW_H;
            cc.egui_ctx.set_style(style);
            apply_theme(&cc.egui_ctx, &cfg.theme);
            let tab = std::env::var("PINSHOT_PREFS_TAB")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0)
                .min(TABS.len() - 1);
            Ok(Box::new(PrefsApp {
                draft: cfg,
                tab,
                group_sel: 0,
                group_edit: String::new(),
                status: String::new(),
                size_key: usize::MAX,
                last_h: 0.0,
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

fn reveal_dir(path: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

struct PrefsApp {
    draft: Config,
    tab: usize,
    group_sel: usize,
    group_edit: String,
    status: String,
    size_key: usize,
    last_h: f32,
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

    fn restore_tab(&mut self) {
        let d = Config::default();
        match self.tab {
            0 => {
                self.draft.language = d.language;
                self.draft.run_at_login = d.run_at_login;
                self.draft.start_minimized = d.start_minimized;
            }
            1 => {
                self.draft.theme = d.theme;
            }
            2 => {
                self.draft.snip_delay = d.snip_delay;
                self.draft.skip_abort_confirm = d.skip_abort_confirm;
                self.draft.history_max = d.history_max;
                self.draft.dim_alpha = d.dim_alpha;
            }
            3 => {
                self.draft.dblclick_pin_action = d.dblclick_pin_action;
                self.draft.closed_pins_max = d.closed_pins_max;
                self.draft.groups = d.groups;
                self.group_sel = 0;
                self.group_edit.clear();
            }
            4 => {
                self.draft.quick_save_folder = d.quick_save_folder;
                self.draft.image_quality = d.image_quality;
            }
            5 => {
                self.draft.hotkey_snip = d.hotkey_snip;
                self.draft.hotkey_pin = d.hotkey_pin;
                self.draft.hotkey_hide_all = d.hotkey_hide_all;
                self.draft.hotkey_restore = d.hotkey_restore;
                self.draft.hotkey_selection_translate = d.hotkey_selection_translate;
                self.draft.hotkey_capture_translate = d.hotkey_capture_translate;
                self.draft.hotkey_clickthrough = d.hotkey_clickthrough;
            }
            _ => {}
        }
        i18n::set_lang(self.draft.lang());
        self.status = tr("Restored this page to defaults.");
    }
}

impl eframe::App for PrefsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(tr("PinShot Preferences")));
        apply_theme(ctx, &self.draft.theme);
        egui::TopBottomPanel::bottom("btns")
            .exact_height(FOOTER_H)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.tab != 6 && ui.button(tr("Restore Defaults")).clicked() {
                        self.restore_tab();
                    }
                    if !self.status.is_empty() {
                        ui.colored_label(Color32::from_rgb(180, 80, 40), &self.status);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr("Cancel")).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        ui.add_space(4.0);
                        if ui.button(tr("OK")).clicked() {
                            if self.apply() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        ui.add_space(4.0);
                        if ui.button(tr("Apply")).clicked() {
                            self.apply();
                        }
                    });
                });
            });
        let mut body_h = 0.0;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ctx, |ui| {
                // Height 0 so the child grows with widgets instead of filling
                // the leftover CentralPanel square (that was the empty band).
                let inner = ui.allocate_ui(egui::vec2(ui.available_width(), 0.0), |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (i, name) in TABS.iter().enumerate() {
                            let selected = self.tab == i;
                            if ui.selectable_label(selected, tr(name)).clicked() {
                                self.tab = i;
                            }
                        }
                    });
                    ui.add_space(1.0);
                    ui.separator();
                    ui.add_space(4.0);
                    match self.tab {
                        0 => self.ui_general(ui),
                        1 => self.ui_interface(ui),
                        2 => self.ui_snip(ui),
                        3 => self.ui_paste(ui),
                        4 => self.ui_output(ui),
                        5 => self.ui_control(ui),
                        _ => self.ui_about(ui),
                    };
                });
                body_h = inner.response.rect.height();
            });
        let size = prefs_inner_size(body_h);
        if self.size_key != self.tab || (self.last_h - size.y).abs() > 1.0 {
            crate::logutil::log(&format!(
                "prefs hug tab={} body={:.0} inner={:.0}x{:.0}",
                self.tab, body_h, size.x, size.y
            ));
            self.size_key = self.tab;
            self.last_h = size.y;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
    }
}

impl PrefsApp {
    fn ui_general(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(tr("Language:"));
            egui::ComboBox::from_id_salt("lang")
                .selected_text(self.draft.lang().display_name())
                .show_ui(ui, |ui| {
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
        });
        ui.checkbox(&mut self.draft.run_at_login, tr("Run at login"))
            .on_hover_text(tr("Start PinShot when you log in"));
        ui.checkbox(&mut self.draft.start_minimized, tr("Start minimized"));
        ui.add_space(4.0);
        ui.label(RichText::new(tr("Configuration")).strong());
        ui.horizontal(|ui| {
            ui.label(config::config_dir().display().to_string());
        });
        ui.horizontal(|ui| {
            if ui.button(tr("Open containing folder")).clicked() {
                reveal_dir(&config::config_dir());
            }
            if ui.button(tr("Open log folder")).clicked() {
                reveal_dir(&config::data_dir());
            }
        });
    }

    fn ui_interface(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(tr("Theme:"));
            let theme_txt = match self.draft.theme.as_str() {
                "light" => tr("Light"),
                "dark" => tr("Dark"),
                _ => tr("System"),
            };
            egui::ComboBox::from_id_salt("theme")
                .selected_text(theme_txt)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.draft.theme, "system".into(), tr("System"));
                    ui.selectable_value(&mut self.draft.theme, "light".into(), tr("Light"));
                    ui.selectable_value(&mut self.draft.theme, "dark".into(), tr("Dark"));
                });
        });
    }

    fn ui_snip(&mut self, ui: &mut Ui) {
        form_grid(ui, "prefs_snip", |ui| {
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
            form_slider(ui, tr("Mask opacity:"), &mut self.draft.dim_alpha, 0..=200);
            form_slider(ui, tr("Snip history:"), &mut self.draft.history_max, 1..=50);
        });
        ui.add_space(2.0);
        let mut confirm = !self.draft.skip_abort_confirm;
        ui.checkbox(&mut confirm, tr("Confirm before abort (Esc)"));
        self.draft.skip_abort_confirm = !confirm;
    }

    fn ui_paste(&mut self, ui: &mut Ui) {
        form_grid(ui, "prefs_paste", |ui| {
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
            form_slider(
                ui,
                tr("Closed pins:"),
                &mut self.draft.closed_pins_max,
                0..=50,
            );
        });
        ui.add_space(2.0);
        ui.label(RichText::new(tr("Pin groups:")).strong());
        let groups = self.draft.groups.clone();
        for (i, g) in groups.iter().enumerate() {
            if ui
                .selectable_label(self.group_sel == i, groups::label(g))
                .clicked()
            {
                self.group_sel = i;
                self.group_edit = g.clone();
            }
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.group_edit).desired_width(200.0),
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
    }

    fn ui_output(&mut self, ui: &mut Ui) {
        ui.label(RichText::new(tr("File")).strong());
        form_grid(ui, "prefs_output", |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(FORM_LABEL_W);
                ui.label(tr("Save folder:"));
            });
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 96.0).max(140.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.draft.quick_save_folder).desired_width(w),
                );
                if ui.button(tr("Browse…")).clicked() {
                    if let Some(p) = crate::dialog::open_folder() {
                        self.draft.quick_save_folder = p;
                    }
                }
                if ui.button(tr("Open")).clicked() && !self.draft.quick_save_folder.is_empty() {
                    reveal_dir(Path::new(&self.draft.quick_save_folder));
                }
            });
            ui.end_row();
            form_slider(ui, tr("Quality:"), &mut self.draft.image_quality, 1..=100);
        });
    }

    fn ui_control(&mut self, ui: &mut Ui) {
        ui.label(RichText::new(tr("Global hotkeys")).strong());
        form_grid(ui, "prefs_hotkeys", |ui| {
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
            ui.add_space(4.0);
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
        ui.add_space(4.0);
        ui.weak(tr(
            "Hotkeys already used by another app cannot be bound. Click Apply to rebind.",
        ));
    }

    fn ui_about(&mut self, ui: &mut Ui) {
        ui.label(RichText::new(format!("PinShot {VERSION}")).strong());
        ui.label(crate::native::platform_name());
        ui.add_space(2.0);
        ui.label(tr(
            "Settings layout follows Snipaste Preferences. Features stay PinShot's.",
        ));
    }
}

fn form_grid(ui: &mut Ui, id: &'static str, add: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([8.0, 4.0])
        .min_col_width(FORM_LABEL_W)
        .min_row_height(ROW_H)
        .show(ui, add);
}

fn form_combo(
    ui: &mut Ui,
    label: String,
    id: &'static str,
    selected: impl Into<egui::WidgetText>,
    add_contents: impl FnOnce(&mut Ui),
) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(FORM_LABEL_W);
        ui.label(label);
    });
    egui::ComboBox::from_id_salt(id)
        .width(CONTROL_W)
        .selected_text(selected)
        .show_ui(ui, add_contents);
    ui.end_row();
}

fn form_line(ui: &mut Ui, label: String, val: &mut String) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(FORM_LABEL_W);
        ui.label(label);
    });
    ui.add(
        egui::TextEdit::singleline(val)
            .desired_width(CONTROL_W)
            .min_size(egui::vec2(CONTROL_W, ROW_H)),
    );
    ui.end_row();
}

fn form_slider<Num: egui::emath::Numeric>(
    ui: &mut Ui,
    label: String,
    val: &mut Num,
    range: std::ops::RangeInclusive<Num>,
) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(FORM_LABEL_W);
        ui.label(label);
    });
    ui.add_sized([CONTROL_W, ROW_H], egui::Slider::new(val, range));
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_window_hugs_each_tab() {
        let interface = prefs_inner_size(48.0);
        let about = prefs_inner_size(64.0);
        let general = prefs_inner_size(148.0);
        let control = prefs_inner_size(236.0);
        assert_eq!(interface.x, PREFS_W);
        assert!(interface.y < 160.0, "short tabs must not keep a tall square");
        assert!(about.y < general.y);
        assert!(interface.y < general.y);
        assert!(control.y > general.y);
        assert!(control.y <= PREFS_MAX_H);
        assert!(PREFS_W < 500.0);
        assert_eq!(TABS.len(), 7);
    }

    #[test]
    fn form_label_column_is_aligned() {
        assert!(FORM_LABEL_W >= 100.0);
        assert!(ROW_H >= 20.0);
        assert!(CONTROL_W >= 180.0);
    }

    #[test]
    fn restore_general_resets_language_and_login() {
        let mut app = PrefsApp {
            draft: Config {
                language: "en".into(),
                run_at_login: true,
                start_minimized: false,
                ..Config::default()
            },
            tab: 0,
            group_sel: 0,
            group_edit: String::new(),
            status: String::new(),
            size_key: 0,
            last_h: 0.0,
        };
        app.restore_tab();
        let d = Config::default();
        assert_eq!(app.draft.language, d.language);
        assert_eq!(app.draft.run_at_login, d.run_at_login);
        assert_eq!(app.draft.start_minimized, d.start_minimized);
    }
}

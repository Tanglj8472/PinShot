//! egui preferences window (own thread + event loop). Apply sends `AppEvent::PrefsApplied`.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, RichText, Visuals};
use pinshot_core::config::{self, Config};
use pinshot_core::groups;
use pinshot_core::hotkey::{bindings_from_config, find_conflicts, normalize_seq};
use pinshot_core::i18n::{self, tr, Lang};
use winit::event_loop::EventLoopProxy;

use crate::event::AppEvent;

static OPEN: AtomicBool = AtomicBool::new(false);

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
        if OPEN.swap(true, Ordering::SeqCst) {
            let _ = proxy.send_event(AppEvent::RaisePrefs);
            return;
        }
        let _ = std::thread::Builder::new()
            .name("pinshot-prefs".into())
            .spawn(move || {
                i18n::set_lang(cfg.lang());
                let opts = eframe::NativeOptions {
                    viewport: egui::ViewportBuilder::default()
                        .with_title(tr("PinShot Preferences"))
                        .with_inner_size([500.0, 580.0])
                        .with_min_inner_size([440.0, 420.0]),
                    ..Default::default()
                };
                let _ = eframe::run_native(
                    "PinShot Preferences",
                    opts,
                    Box::new(move |cc| {
                        apply_theme(cc, &cfg.theme);
                        Ok(Box::new(PrefsApp {
                            draft: cfg,
                            proxy,
                            tab: 0,
                            advanced: false,
                            group_sel: 0,
                            group_edit: String::new(),
                            status: String::new(),
                        }))
                    }),
                );
                OPEN.store(false, Ordering::SeqCst);
            });
    }
}

fn apply_theme(cc: &eframe::CreationContext<'_>, theme: &str) {
    let dark = match theme {
        "dark" => true,
        "light" => false,
        _ => system_dark(),
    };
    cc.egui_ctx.set_visuals(if dark {
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
    proxy: EventLoopProxy<AppEvent>,
    tab: usize,
    advanced: bool,
    group_sel: usize,
    group_edit: String,
    status: String,
}

impl PrefsApp {
    fn apply(&mut self) {
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
            return;
        }
        self.draft = self.draft.clone().validate();
        if let Err(e) = config::save(&self.draft) {
            self.status = e.to_string();
            return;
        }
        i18n::set_lang(self.draft.lang());
        let _ = self.proxy.send_event(AppEvent::PrefsApplied(self.draft.clone()));
        self.status = tr("Apply");
    }
}

impl eframe::App for PrefsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::bottom("btns").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button(tr("Apply")).clicked() {
                    self.apply();
                }
                if ui.button(tr("OK")).clicked() {
                    self.apply();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button(tr("Cancel")).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if !self.status.is_empty() {
                    ui.colored_label(Color32::from_rgb(180, 80, 40), &self.status);
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(tr("PinShot Preferences"));
            ui.horizontal(|ui| {
                for (i, name) in [tr("General"), tr("Hotkeys"), tr("Output")].iter().enumerate() {
                    if ui.selectable_label(self.tab == i, name).clicked() {
                        self.tab = i;
                    }
                }
            });
            ui.separator();
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
        ui.horizontal(|ui| {
            ui.label(tr("Language:"));
            egui::ComboBox::from_id_salt("lang")
                .selected_text(self.draft.lang().display_name())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.draft.language, "zh_CN".into(), Lang::ZhCn.display_name());
                    ui.selectable_value(&mut self.draft.language, "en".into(), Lang::En.display_name());
                });
        });
        ui.horizontal(|ui| {
            ui.label(tr("Theme:"));
            egui::ComboBox::from_id_salt("theme")
                .selected_text(match self.draft.theme.as_str() {
                    "light" => tr("Light"),
                    "dark" => tr("Dark"),
                    _ => tr("System"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.draft.theme, "system".into(), tr("System"));
                    ui.selectable_value(&mut self.draft.theme, "light".into(), tr("Light"));
                    ui.selectable_value(&mut self.draft.theme, "dark".into(), tr("Dark"));
                });
        });
        ui.checkbox(&mut self.draft.run_at_login, tr("Run at login"));
        ui.horizontal(|ui| {
            ui.label(tr("Snip delay:"));
            let mut secs = self.draft.snip_delay;
            egui::ComboBox::from_id_salt("delay")
                .selected_text(if secs == 0 {
                    tr("None (0 s)")
                } else {
                    tr(&format!("{secs} s"))
                })
                .show_ui(ui, |ui| {
                    for s in [0, 1, 2, 3, 5] {
                        ui.selectable_value(&mut secs, s, if s == 0 {
                            tr("None (0 s)")
                        } else {
                            tr(&format!("{s} s"))
                        });
                    }
                });
            self.draft.snip_delay = secs;
        });

        if ui
            .button(if self.advanced {
                format!("▾ {}", tr("Advanced"))
            } else {
                format!("▸ {}", tr("Advanced"))
            })
            .clicked()
        {
            self.advanced = !self.advanced;
        }
        if !self.advanced {
            return;
        }
        let mut confirm = !self.draft.skip_abort_confirm;
        ui.checkbox(&mut confirm, tr("Confirm before abort (Esc)"));
        self.draft.skip_abort_confirm = !confirm;
        ui.horizontal(|ui| {
            ui.label(tr("Double-click pin:"));
            egui::ComboBox::from_id_salt("dbl")
                .selected_text(match self.draft.dblclick_pin_action.as_str() {
                    "hide" => tr("Hide pin"),
                    "none" => tr("Do nothing"),
                    _ => tr("Close pin"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.draft.dblclick_pin_action, "close".into(), tr("Close pin"));
                    ui.selectable_value(&mut self.draft.dblclick_pin_action, "hide".into(), tr("Hide pin"));
                    ui.selectable_value(&mut self.draft.dblclick_pin_action, "none".into(), tr("Do nothing"));
                });
        });
        ui.horizontal(|ui| {
            ui.label(tr("Snip history:"));
            ui.add(egui::Slider::new(&mut self.draft.history_max, 1..=50));
        });
        ui.horizontal(|ui| {
            ui.label(tr("Closed pins:"));
            ui.add(egui::Slider::new(&mut self.draft.closed_pins_max, 0..=50));
        });
        ui.label(tr("Pin groups:"));
        let groups = self.draft.groups.clone();
        egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
            for (i, g) in groups.iter().enumerate() {
                if ui.selectable_label(self.group_sel == i, groups::label(g)).clicked() {
                    self.group_sel = i;
                    self.group_edit = g.clone();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.group_edit);
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

    fn ui_hotkeys(&mut self, ui: &mut egui::Ui) {
        let rows = [
            (tr("Snip:"), &mut self.draft.hotkey_snip),
            (tr("Pin clipboard:"), &mut self.draft.hotkey_pin),
            (tr("Hide/show pins:"), &mut self.draft.hotkey_hide_all),
            (tr("Restore last pin:"), &mut self.draft.hotkey_restore),
            (tr("Select-text translate:"), &mut self.draft.hotkey_selection_translate),
            (tr("Snip and translate:"), &mut self.draft.hotkey_capture_translate),
            (tr("Click-through:"), &mut self.draft.hotkey_clickthrough),
        ];
        // borrow checker: edit one at a time
        let _ = rows;
        fn row(ui: &mut egui::Ui, label: String, val: &mut String) {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.text_edit_singleline(val);
            });
        }
        row(ui, tr("Snip:"), &mut self.draft.hotkey_snip);
        row(ui, tr("Pin clipboard:"), &mut self.draft.hotkey_pin);
        row(ui, tr("Hide/show pins:"), &mut self.draft.hotkey_hide_all);
        row(ui, tr("Restore last pin:"), &mut self.draft.hotkey_restore);
        row(ui, tr("Select-text translate:"), &mut self.draft.hotkey_selection_translate);
        row(ui, tr("Snip and translate:"), &mut self.draft.hotkey_capture_translate);
        row(ui, tr("Click-through:"), &mut self.draft.hotkey_clickthrough);
        let conflicts = find_conflicts(&bindings_from_config(&self.draft));
        if !conflicts.is_empty() {
            ui.colored_label(
                Color32::from_rgb(200, 60, 40),
                RichText::new(format!("{}: {} + {}", tr("Conflicting hotkeys"), conflicts[0].0, conflicts[0].1)),
            );
        }
        ui.add_space(8.0);
        ui.weak(tr(
            "Hotkeys already used by another app cannot be bound. Click Apply to rebind.",
        ));
    }

    fn ui_output(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(tr("Save folder:"));
            ui.text_edit_singleline(&mut self.draft.quick_save_folder);
        });
        ui.horizontal(|ui| {
            ui.label(tr("Quality:"));
            ui.add(egui::Slider::new(&mut self.draft.image_quality, 1..=100));
        });
    }
}

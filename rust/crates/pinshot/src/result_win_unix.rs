//! OCR / translation result window. Spawned as `--ocr-window` so eframe is not
//! started on a second thread after `gtk::init` (that path fails silently).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use eframe::egui;
use pinshot_core::i18n::tr;
use pinshot_core::translate::{read_translation, request_body, request_url, TARGETS};

use crate::clipboard;

static OPEN: AtomicBool = AtomicBool::new(false);

static CHILD: std::sync::Mutex<Option<std::process::Child>> =
    std::sync::Mutex::new(None);

pub fn open(text: String, auto_translate: bool) {
    spawn_window(&text, auto_translate, false);
}

pub fn open_error(err: String) {
    spawn_window(&err, false, true);
}

fn spawn_window(text: &str, auto_translate: bool, is_error: bool) {
    reap_child();
    let dir = pinshot_core::config::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!(
        "ocr-result-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    if let Err(e) = std::fs::write(&path, text.as_bytes()) {
        crate::logutil::log(&format!("ocr result write: {e}"));
        return;
    }
    match std::env::current_exe() {
        Ok(exe) => {
            let mut cmd = std::process::Command::new(exe);
            cmd.arg("--ocr-window").arg(&path);
            if auto_translate {
                cmd.arg("--ocr-translate");
            }
            if is_error {
                cmd.arg("--ocr-error");
            }
            match cmd.spawn() {
                Ok(proc) => {
                    crate::logutil::log(&format!("ocr window spawn {}", path.display()));
                    if let Ok(mut slot) = CHILD.lock() {
                        *slot = Some(proc);
                    }
                }
                Err(e) => crate::logutil::log(&format!("ocr window spawn: {e}")),
            }
        }
        Err(e) => crate::logutil::log(&format!("ocr window exe: {e}")),
    }
}

pub fn reap_child() {
    if let Ok(mut slot) = CHILD.lock() {
        if let Some(child) = slot.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    *slot = None;
                    #[cfg(unix)]
                    crate::x11util::release_stray_grabs();
                }
                Ok(None) => {}
                Err(_) => *slot = None,
            }
        }
    }
}

pub fn run_ocr_window(path: &str, auto_translate: bool, is_error: bool) {
    crate::logutil::init(&[]);
    let text = std::fs::read_to_string(path).unwrap_or_else(|_| path.to_string());
    let _ = std::fs::remove_file(path);
    OPEN.store(true, Ordering::SeqCst);
    schedule_raises();
    let cfg = pinshot_core::config::load().unwrap_or_default();
    let theme = cfg.theme.clone();
    let orig = if is_error { String::new() } else { text.clone() };
    let status0 = if is_error {
        if text.trim().is_empty() {
            tr("OCR failed")
        } else {
            text
        }
    } else if auto_translate {
        tr("Translating…")
    } else {
        tr("OCR done. Edit, copy, or translate.")
    };
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(tr("PinShot OCR"))
            .with_inner_size([520.0, 420.0])
            .with_min_inner_size([460.0, 320.0])
            .with_always_on_top(),
        persist_window: false,
        centered: true,
        ..Default::default()
    };
    let _ = eframe::run_native(
        "PinShot OCR",
        opts,
        Box::new(move |cc| {
            crate::ui_font::install_cjk(&cc.egui_ctx);
            let mut style = (*cc.egui_ctx.style()).clone();
            style.spacing.item_spacing = egui::vec2(12.0, 8.0);
            style.spacing.button_padding = egui::vec2(12.0, 5.0);
            cc.egui_ctx.set_style(style);
            let dark = theme == "dark";
            cc.egui_ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            Ok(Box::new(OcrWin {
                orig,
                trans: String::new(),
                target: 0,
                status: status0,
                auto: auto_translate && !is_error,
                busy: auto_translate && !is_error,
                raised: false,
            }))
        }),
    );
    OPEN.store(false, Ordering::SeqCst);
}

fn schedule_raises() {
    let _ = std::thread::Builder::new()
        .name("pinshot-ocr-raise".into())
        .spawn(|| {
            for ms in [0u64, 60, 250] {
                if ms > 0 {
                    std::thread::sleep(Duration::from_millis(ms));
                }
                if !OPEN.load(Ordering::SeqCst) {
                    return;
                }
                raise_above_pins();
            }
        });
}

pub fn raise_above_pins() {
    crate::x11util::raise_ocr_above_pins();
}

struct OcrWin {
    orig: String,
    trans: String,
    target: usize,
    status: String,
    auto: bool,
    busy: bool,
    raised: bool,
}

impl eframe::App for OcrWin {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.raised {
            self.raised = true;
            raise_above_pins();
        }
        if self.auto && !self.orig.trim().is_empty() {
            self.auto = false;
            self.busy = true;
            let tgt = TARGETS[self.target.min(TARGETS.len() - 1)].1;
            let body = match request_body(&self.orig) {
                Ok(b) => b,
                Err(e) => {
                    self.status = e;
                    self.busy = false;
                    return;
                }
            };
            let url = request_url(tgt);
            match ureq::post(&url)
                .set("Content-Type", "application/json")
                .send_string(&body)
            {
                Ok(resp) => {
                    let t = resp.into_string().unwrap_or_default();
                    self.trans = read_translation(&t).unwrap_or_default();
                    self.status = tr("Translation done");
                }
                Err(e) => self.status = e.to_string(),
            }
            self.busy = false;
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin::same(16)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                ui.label(egui::RichText::new(&self.status).weak());
                ui.add_space(4.0);
                ui.label(tr("Original (editable)"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.orig)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if ui.button(tr("Copy original")).clicked() {
                        let _ = clipboard::write_text(&self.orig);
                    }
                    egui::ComboBox::from_id_salt("lang")
                        .selected_text(TARGETS[self.target].0)
                        .show_ui(ui, |ui| {
                            for (i, (name, _)) in TARGETS.iter().enumerate() {
                                ui.selectable_value(&mut self.target, i, *name);
                            }
                        });
                    if ui.button(tr("Translate")).clicked() && !self.orig.trim().is_empty() {
                        self.auto = true;
                        self.status = tr("Translating…");
                    }
                });
                ui.add_space(6.0);
                ui.label(tr("Translation"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.trans)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY),
                );
                if ui.button(tr("Copy translation")).clicked() {
                    let _ = clipboard::write_text(&self.trans);
                }
            });
    }
}

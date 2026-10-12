//! OCR / translation result window (egui, always-on-top).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use eframe::egui;
use pinshot_core::i18n::tr;
use pinshot_core::translate::{read_translation, request_body, request_url, TARGETS};

use crate::clipboard;

static OPEN: AtomicBool = AtomicBool::new(false);

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

pub fn open(text: String, auto_translate: bool) {
    if OPEN.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("pinshot-ocr-win".into())
        .spawn(move || {
            schedule_raises();
            let opts = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_title(tr("PinShot OCR"))
                    .with_inner_size([520.0, 520.0])
                    .with_always_on_top(),
                ..Default::default()
            };
            let _ = eframe::run_native(
                "PinShot OCR",
                opts,
                Box::new(move |_cc| {
                    Ok(Box::new(OcrWin {
                        orig: text,
                        trans: String::new(),
                        target: 0,
                        status: if auto_translate {
                            tr("Translating…")
                        } else {
                            tr("OCR done. Edit, copy, or translate.")
                        },
                        auto: auto_translate,
                        busy: auto_translate,
                        raised: false,
                    }))
                }),
            );
            OPEN.store(false, Ordering::SeqCst);
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
            match ureq::post(&url).set("Content-Type", "application/json").send_string(&body) {
                Ok(resp) => {
                    let t = resp.into_string().unwrap_or_default();
                    self.trans = read_translation(&t).unwrap_or_default();
                    self.status = tr("Translation done");
                }
                Err(e) => self.status = e.to_string(),
            }
            self.busy = false;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(&self.status);
            ui.label(tr("Original (editable)"));
            ui.add(egui::TextEdit::multiline(&mut self.orig).desired_rows(8).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if ui.button(tr("Copy original")).clicked() {
                    let _ = clipboard::write_text(&self.orig);
                }
                egui::ComboBox::from_id_salt("lang").selected_text(TARGETS[self.target].0).show_ui(ui, |ui| {
                    for (i, (name, _)) in TARGETS.iter().enumerate() {
                        ui.selectable_value(&mut self.target, i, *name);
                    }
                });
                if ui.button(tr("Translate")).clicked() && !self.orig.trim().is_empty() {
                    self.auto = true;
                    self.status = tr("Translating…");
                }
            });
            ui.label(tr("Translation"));
            ui.add(egui::TextEdit::multiline(&mut self.trans).desired_rows(8).desired_width(f32::INFINITY));
            if ui.button(tr("Copy translation")).clicked() {
                let _ = clipboard::write_text(&self.trans);
            }
        });
    }
}

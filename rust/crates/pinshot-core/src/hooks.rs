//! Extension-point traits for later phases. Phase 1 ships stubs only.

use crate::geom::{Point, Rect};
use crate::raster::Raster;

/// egui (or other) settings window. Phase 1: not shown.
pub trait SettingsUi {
    fn open(&mut self);
    fn is_open(&self) -> bool;
}

#[derive(Debug, Default)]
pub struct SettingsUiStub {
    open: bool,
}

impl SettingsUi for SettingsUiStub {
    fn open(&mut self) {
        self.open = true;
    }
    fn is_open(&self) -> bool {
        self.open
    }
}

/// UI Automation / AT-SPI element detector. Phase 1: window enum only.
pub trait ElementDetector {
    fn source_name(&self) -> &'static str;
    fn hit_test(&self, pt: Point) -> Option<Rect>;
    fn snapshot(&mut self, exclude_pid: u32) -> Vec<(Rect, String)>;
}

#[derive(Debug, Default)]
pub struct ElementDetectorStub;

impl ElementDetector for ElementDetectorStub {
    fn source_name(&self) -> &'static str {
        "stub"
    }
    fn hit_test(&self, _pt: Point) -> Option<Rect> {
        None
    }
    fn snapshot(&mut self, _exclude_pid: u32) -> Vec<(Rect, String)> {
        Vec::new()
    }
}

/// Local OCR (planned: `ort` + PP-OCRv5).
pub trait OcrEngine {
    fn recognize(&self, image: &Raster) -> Result<String, String>;
}

#[derive(Debug, Default)]
pub struct OcrStub;

impl OcrEngine for OcrStub {
    fn recognize(&self, _image: &Raster) -> Result<String, String> {
        Err("OCR not available in this build. Use pinshot-ocr.exe.".into())
    }
}

/// Translation backend (planned: Edge / local model).
pub trait Translator {
    fn translate(&self, text: &str, from: &str, to: &str) -> Result<String, String>;
}

#[derive(Debug, Default)]
pub struct TranslatorStub;

impl Translator for TranslatorStub {
    fn translate(&self, _text: &str, _from: &str, _to: &str) -> Result<String, String> {
        Err("translation not implemented (Phase 1 stub)".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stubs_are_honest() {
        let mut s = SettingsUiStub::default();
        assert!(!s.is_open());
        s.open();
        assert!(s.is_open());
        assert!(OcrStub.recognize(&Raster::new(1, 1)).is_err());
        assert!(TranslatorStub.translate("hi", "en", "zh").is_err());
        assert!(ElementDetectorStub.hit_test(Point::ZERO).is_none());
    }
}

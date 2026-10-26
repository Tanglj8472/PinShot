//! Lite builds never load an OCR engine or download models.
use pinshot_core::{ocr::OcrPage, raster::Raster};

pub fn recognize_with_progress(_: &Raster, _: impl FnMut(&str)) -> Result<OcrPage, String> {
    Err("OCR is not included in PinShot Lite".into())
}
pub fn release_engine() {}
pub fn prefetch_models() {}
pub fn models_ready_for_tier(_: &str) -> bool { false }
pub fn download_models_for_tier(_: &str, _: impl FnMut(f32) + Send) -> Result<(), String> {
    Err("OCR is not included in PinShot Lite".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_model_download_or_recognition_in_lite() {
        assert!(!models_ready_for_tier("server"));
        assert!(download_models_for_tier("mobile", |_| panic!("no download")).is_err());
        assert!(recognize_with_progress(&Raster::new(1, 1), |_| panic!("no inference")).is_err());
    }
}

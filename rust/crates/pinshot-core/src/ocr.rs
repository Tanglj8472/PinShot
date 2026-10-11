//! PP-OCRv5 model layout + CTC greedy decode (inference lives behind `--features ocr`).

use std::path::{Path, PathBuf};

pub const MODEL_DIR_NAME: &str = "ocr-models";

pub const DET_FILE: &str = "ch_PP-OCRv5_mobile_det.onnx";
pub const REC_FILE: &str = "ch_PP-OCRv5_mobile_rec.onnx";
pub const CLS_FILE: &str = "ch_PP-LCNet_x0_25_textline_ori.onnx";
pub const KEYS_FILE: &str = "ppocr_keys_v5.txt";

/// RapidOCR / PaddleOCR mobile v5 ONNX (downloaded on first use).
pub const DET_URL: &str =
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.2.0/onnx/PP-OCRv5/det/ch_PP-OCRv5_mobile_det.onnx";
pub const REC_URL: &str =
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.2.0/onnx/PP-OCRv5/rec/ch_PP-OCRv5_mobile_rec.onnx";
pub const CLS_URL: &str =
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.2.0/onnx/PP-OCRv5/cls/ch_PP-LCNet_v1_textline_ori.onnx";
pub const KEYS_URL: &str =
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.2.0/onnx/PP-OCRv5/rec/ppocrv5_dict.txt";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPaths {
    pub dir: PathBuf,
    pub det: PathBuf,
    pub rec: PathBuf,
    pub keys: PathBuf,
    pub cls: PathBuf,
}

impl ModelPaths {
    pub fn in_dir(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        Self {
            det: dir.join(DET_FILE),
            rec: dir.join(REC_FILE),
            keys: dir.join(KEYS_FILE),
            cls: dir.join(CLS_FILE),
            dir,
        }
    }

    pub fn ready(&self) -> bool {
        self.det.is_file() && self.rec.is_file() && self.keys.is_file()
    }

    pub fn downloads(&self) -> Vec<(&'static str, PathBuf)> {
        vec![
            (DET_URL, self.det.clone()),
            (REC_URL, self.rec.clone()),
            (KEYS_URL, self.keys.clone()),
            (CLS_URL, self.cls.clone()),
        ]
    }
}

pub fn model_dir(data: &Path) -> PathBuf {
    data.join(MODEL_DIR_NAME)
}

/// CTC greedy decode: skip blanks (`0`) and consecutive repeats.
pub fn ctc_greedy(indices: &[usize], alphabet: &[String]) -> String {
    let mut out = String::new();
    let mut prev = usize::MAX;
    for &i in indices {
        if i == 0 || i == prev {
            prev = i;
            continue;
        }
        prev = i;
        if let Some(ch) = alphabet.get(i.saturating_sub(1)) {
            out.push_str(ch);
        }
    }
    out
}

pub fn load_keys(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

pub fn join_lines(lines: &[String]) -> String {
    lines
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctc_skips_blank_and_repeats() {
        let alpha = vec!["你".into(), "好".into(), "啊".into()];
        // 0=blank, 1=你, 2=好
        assert_eq!(ctc_greedy(&[0, 1, 1, 0, 2, 2, 0], &alpha), "你好");
    }

    #[test]
    fn model_paths_under_data() {
        let p = ModelPaths::in_dir("/tmp/ocr-models");
        assert!(p.det.ends_with(DET_FILE));
        assert_eq!(p.downloads().len(), 4);
    }

    #[test]
    fn join_filters_blank() {
        assert_eq!(join_lines(&[" a ".into(), "".into(), "b".into()]), "a\nb");
    }
}

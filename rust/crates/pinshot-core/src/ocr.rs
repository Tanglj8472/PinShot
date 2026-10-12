//! PP-OCRv5 model layout, DBNet box extraction, and CTC greedy decode.
//! Inference lives in the host (`--features ocr` / Linux).

use std::path::{Path, PathBuf};

pub const MODEL_DIR_NAME: &str = "ocr-models";

pub const DET_FILE: &str = "ch_PP-OCRv5_mobile_det.onnx";
pub const REC_FILE: &str = "ch_PP-OCRv5_mobile_rec.onnx";
pub const CLS_FILE: &str = "ch_PP-LCNet_x0_25_textline_ori.onnx";
pub const KEYS_FILE: &str = "ppocr_keys_v5.txt";

/// RapidOCR / PaddleOCR mobile v5 ONNX. First URL is ModelScope v3.9.2
/// (`default_models.yaml`); local dest names stay stable so existing caches hit.
pub const DET_URLS: &[&str] = &[
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_mobile.onnx",
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.2.0/onnx/PP-OCRv5/det/ch_PP-OCRv5_mobile_det.onnx",
    "https://huggingface.co/RapidAI/RapidOCR/resolve/main/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_mobile.onnx",
];
pub const REC_URLS: &[&str] = &[
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile.onnx",
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.2.0/onnx/PP-OCRv5/rec/ch_PP-OCRv5_mobile_rec.onnx",
    "https://huggingface.co/RapidAI/RapidOCR/resolve/main/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile.onnx",
];
pub const CLS_URLS: &[&str] = &[
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/cls/ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx",
    "https://huggingface.co/RapidAI/RapidOCR/resolve/main/onnx/PP-OCRv5/cls/ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx",
];
pub const KEYS_URLS: &[&str] = &[
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/paddle/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile/ppocrv5_dict.txt",
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.2.0/onnx/PP-OCRv5/rec/ppocrv5_dict.txt",
    "https://huggingface.co/RapidAI/RapidOCR/resolve/main/onnx/PP-OCRv5/rec/ppocrv5_dict.txt",
];

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

    pub fn downloads(&self) -> Vec<(Vec<&'static str>, PathBuf)> {
        vec![
            (DET_URLS.to_vec(), self.det.clone()),
            (REC_URLS.to_vec(), self.rec.clone()),
            (KEYS_URLS.to_vec(), self.keys.clone()),
            (CLS_URLS.to_vec(), self.cls.clone()),
        ]
    }
}

pub fn model_dir(data: &Path) -> PathBuf {
    data.join(MODEL_DIR_NAME)
}

/// Axis-aligned text box in source-image pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub score: f32,
}

/// DBNet probability map → boxes. `prob` is the first channel of the det
/// output, sized `dw × dh` (model input, including 32-aligned padding).
/// `scale` maps that grid back to the original raster; padding is top-left.
pub fn db_boxes(
    prob: &[f32],
    dw: u32,
    dh: u32,
    img_w: u32,
    img_h: u32,
    scale: f32,
) -> Vec<TextBox> {
    let dw = dw.max(1) as usize;
    let dh = dh.max(1) as usize;
    if prob.len() < dw * dh {
        return Vec::new();
    }
    const BIN: f32 = 0.3;
    const MIN_SCORE: f32 = 0.5;
    const UNCLIP: f32 = 1.5;
    let mut seen = vec![false; dw * dh];
    let mut boxes = Vec::new();
    let mut stack = Vec::new();
    for y0 in 0..dh {
        for x0 in 0..dw {
            let i0 = y0 * dw + x0;
            if seen[i0] || prob[i0] < BIN {
                continue;
            }
            stack.clear();
            stack.push((x0, y0));
            seen[i0] = true;
            let mut minx = x0;
            let mut maxx = x0;
            let mut miny = y0;
            let mut maxy = y0;
            let mut sum = 0.0f32;
            let mut n = 0u32;
            while let Some((x, y)) = stack.pop() {
                let i = y * dw + x;
                sum += prob[i];
                n += 1;
                minx = minx.min(x);
                maxx = maxx.max(x);
                miny = miny.min(y);
                maxy = maxy.max(y);
                for (nx, ny) in [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ] {
                    if nx >= dw || ny >= dh {
                        continue;
                    }
                    let ni = ny * dw + nx;
                    if seen[ni] || prob[ni] < BIN {
                        continue;
                    }
                    seen[ni] = true;
                    stack.push((nx, ny));
                }
            }
            if n < 8 {
                continue;
            }
            let score = sum / n as f32;
            if score < MIN_SCORE {
                continue;
            }
            let bw = (maxx - minx + 1) as f32;
            let bh = (maxy - miny + 1) as f32;
            let peri = (2.0 * (bw + bh)).max(1.0);
            let d = (UNCLIP * (n as f32) / peri).ceil().max(1.0);
            let x1 = (minx as f32 - d).max(0.0);
            let y1 = (miny as f32 - d).max(0.0);
            let x2 = (maxx as f32 + 1.0 + d).min(dw as f32);
            let y2 = (maxy as f32 + 1.0 + d).min(dh as f32);
            let s = scale.max(1e-6);
            let sx = ((x1 / s).floor() as i32).max(0) as u32;
            let sy = ((y1 / s).floor() as i32).max(0) as u32;
            let ex = ((x2 / s).ceil() as u32).min(img_w);
            let ey = ((y2 / s).ceil() as u32).min(img_h);
            if ex <= sx + 2 || ey <= sy + 2 {
                continue;
            }
            boxes.push(TextBox {
                x: sx,
                y: sy,
                w: ex - sx,
                h: ey - sy,
                score,
            });
        }
    }
    boxes.sort_by(|a, b| {
        let ya = a.y + a.h / 2;
        let yb = b.y + b.h / 2;
        ya.cmp(&yb).then(a.x.cmp(&b.x))
    });
    merge_line_boxes(&boxes)
}

/// Join boxes that sit on the same text line (RapidOCR-style).
fn merge_line_boxes(boxes: &[TextBox]) -> Vec<TextBox> {
    if boxes.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<TextBox> = Vec::new();
    for b in boxes {
        if let Some(last) = out.last_mut() {
            let same_line = {
                let cy = b.y + b.h / 2;
                let ly = last.y + last.h / 2;
                (cy as i32 - ly as i32).unsigned_abs() < last.h.max(b.h) / 2 + 4
            };
            let gap = b.x.saturating_sub(last.x + last.w);
            if same_line && gap < last.h.max(8) * 2 {
                let x2 = (last.x + last.w).max(b.x + b.w);
                let y2 = (last.y + last.h).max(b.y + b.h);
                last.x = last.x.min(b.x);
                last.y = last.y.min(b.y);
                last.w = x2 - last.x;
                last.h = y2 - last.y;
                last.score = last.score.max(b.score);
                continue;
            }
        }
        out.push(*b);
    }
    out
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

    #[test]
    fn db_boxes_finds_two_bands() {
        let dw = 32u32;
        let dh = 32u32;
        let mut prob = vec![0.0f32; (dw * dh) as usize];
        for y in 4..10 {
            for x in 2..28 {
                prob[(y * dw + x) as usize] = 0.9;
            }
        }
        for y in 18..24 {
            for x in 4..20 {
                prob[(y * dw + x) as usize] = 0.85;
            }
        }
        let boxes = db_boxes(&prob, dw, dh, 320, 320, 0.1);
        assert!(
            boxes.len() >= 2,
            "expected two text bands, got {:?}",
            boxes
        );
        assert!(boxes[0].y < boxes[1].y);
        assert!(boxes.iter().all(|b| b.w > 10 && b.h > 4));
    }
}

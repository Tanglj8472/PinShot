//! PP-OCRv5 model layout and in-place OCR tokens (boxes from paddle-ocr-rs).

use std::path::{Path, PathBuf};

use crate::geom::{Point, Rect};

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
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/cls/ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx",
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
        self.det.is_file() && self.rec.is_file() && self.cls.is_file()
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

/// One selectable unit mapped onto the source image.
#[derive(Clone, Debug, PartialEq)]
pub struct OcrToken {
    pub rect: Rect,
    pub text: String,
    pub line: usize,
}

/// Recognized page: reading-order tokens plus joined plain text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrPage {
    pub tokens: Vec<OcrToken>,
    pub text: String,
}

pub fn is_cjk(c: char) -> bool {
    matches!(
        c,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{3000}'..='\u{303F}'
            | '\u{3040}'..='\u{30FF}'
            | '\u{AC00}'..='\u{D7AF}'
            | '\u{FF00}'..='\u{FFEF}'
    )
}

pub fn aabb_from_quad(pts: &[(u32, u32)]) -> Rect {
    if pts.is_empty() {
        return Rect::EMPTY;
    }
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for &(x, y) in pts {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if max_x <= min_x || max_y <= min_y {
        return Rect::EMPTY;
    }
    Rect::new(
        min_x as f32,
        min_y as f32,
        (max_x - min_x) as f32,
        (max_y - min_y) as f32,
    )
}

/// Split a line box into CJK characters / Latin words, mapped by character width.
pub fn tokenize_line(line: usize, text: &str, box_rect: Rect) -> Vec<OcrToken> {
    let text = text.trim();
    if text.is_empty() || box_rect.w <= 0.0 || box_rect.h <= 0.0 {
        return Vec::new();
    }
    let mut units: Vec<String> = Vec::new();
    let mut buf = String::new();
    for c in text.chars() {
        if c.is_whitespace() {
            if !buf.is_empty() {
                units.push(std::mem::take(&mut buf));
            }
            continue;
        }
        if is_cjk(c) {
            if !buf.is_empty() {
                units.push(std::mem::take(&mut buf));
            }
            units.push(c.to_string());
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() {
        units.push(buf);
    }
    if units.is_empty() {
        return Vec::new();
    }
    let total: f32 = units.iter().map(|u| u.chars().count().max(1) as f32).sum();
    let mut x = box_rect.x;
    let mut out = Vec::with_capacity(units.len());
    for u in units {
        let w = box_rect.w * (u.chars().count().max(1) as f32 / total);
        out.push(OcrToken {
            rect: Rect::new(x, box_rect.y, w.max(1.0), box_rect.h.max(1.0)),
            text: u,
            line,
        });
        x += w;
    }
    if let Some(last) = out.last_mut() {
        last.rect.w = (box_rect.right() - last.rect.x).max(1.0);
    }
    out
}

fn join_line_parts(parts: &[String]) -> String {
    let mut s = String::new();
    for p in parts {
        if s.is_empty() {
            s.push_str(p);
            continue;
        }
        if needs_space(s.chars().last(), p.chars().next()) {
            s.push(' ');
        }
        s.push_str(p);
    }
    s
}

fn needs_space(prev: Option<char>, next: Option<char>) -> bool {
    match (prev, next) {
        (Some(a), Some(b))
            if !a.is_whitespace()
                && !b.is_whitespace()
                && !is_cjk(a)
                && !is_cjk(b)
                && (a.is_ascii_alphanumeric() || b.is_ascii_alphanumeric()) =>
        {
            true
        }
        _ => false,
    }
}

/// Build a page from detector quadrilaterals + recognized line strings.
pub fn page_from_blocks(blocks: Vec<(Vec<(u32, u32)>, String)>) -> OcrPage {
    let mut items: Vec<(Rect, String)> = blocks
        .into_iter()
        .filter_map(|(pts, text)| {
            let r = aabb_from_quad(&pts);
            let t = text.trim().to_string();
            if t.is_empty() || r.is_empty() {
                None
            } else {
                Some((r, t))
            }
        })
        .collect();
    items.sort_by(|a, b| {
        a.0.y
            .partial_cmp(&b.0.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.x.partial_cmp(&b.0.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut tokens = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut line_i = 0usize;
    let mut prev_mid = None::<f32>;
    let mut prev_h = 0f32;
    let mut line_parts: Vec<String> = Vec::new();

    for (rect, text) in items {
        let mid = rect.y + rect.h * 0.5;
        let new_line = match prev_mid {
            None => false,
            Some(py) => (mid - py).abs() > prev_h.max(rect.h) * 0.6,
        };
        if new_line {
            if !line_parts.is_empty() {
                lines.push(join_line_parts(&line_parts));
                line_parts.clear();
                line_i += 1;
            }
        }
        prev_mid = Some(mid);
        prev_h = rect.h;
        tokens.extend(tokenize_line(line_i, &text, rect));
        line_parts.push(text);
    }
    if !line_parts.is_empty() {
        lines.push(join_line_parts(&line_parts));
    }
    OcrPage {
        tokens,
        text: lines.join("\n"),
    }
}

/// Cover an image with one line of tokens (Windows F5 copied text, tests).
pub fn page_from_plain(text: &str, width: u32, height: u32) -> OcrPage {
    let t = text.trim();
    if t.is_empty() {
        return OcrPage::default();
    }
    let pad = 8.0;
    let rect = Rect::new(
        pad,
        pad,
        (width as f32 - pad * 2.0).max(8.0),
        (height as f32 - pad * 2.0).max(8.0),
    );
    let tokens = tokenize_line(0, t, rect);
    OcrPage {
        text: t.to_string(),
        tokens,
    }
}

pub fn hit_token(tokens: &[OcrToken], p: Point) -> Option<usize> {
    tokens
        .iter()
        .position(|t| t.rect.contains(p))
        .or_else(|| {
            tokens
                .iter()
                .enumerate()
                .filter(|(_, t)| t.rect.inflated(8.0).contains(p))
                .min_by(|a, b| {
                    a.1.rect
                        .center()
                        .distance(p)
                        .partial_cmp(&b.1.rect.center().distance(p))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
        })
}

/// Token nearest the pointer in reading order (used while dragging off a glyph).
pub fn nearest_token(tokens: &[OcrToken], p: Point) -> Option<usize> {
    if let Some(i) = hit_token(tokens, p) {
        return Some(i);
    }
    if tokens.is_empty() {
        return None;
    }
    tokens
        .iter()
        .enumerate()
        .min_by(|a, b| {
            let da = token_drag_dist(&a.1.rect, p);
            let db = token_drag_dist(&b.1.rect, p);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
}

fn token_drag_dist(r: &Rect, p: Point) -> f32 {
    let cx = p.x.clamp(r.x, r.right());
    let cy = p.y.clamp(r.y, r.bottom());
    Point::new(cx, cy).distance(p) + (r.center().y - p.y).abs() * 0.25
}

pub fn selected_text(tokens: &[OcrToken], a: usize, b: usize) -> String {
    if tokens.is_empty() {
        return String::new();
    }
    let lo = a.min(b).min(tokens.len() - 1);
    let hi = a.max(b).min(tokens.len() - 1);
    let mut s = String::new();
    let mut prev_line = None;
    for t in &tokens[lo..=hi] {
        if let Some(pl) = prev_line {
            if t.line != pl {
                s.push('\n');
            } else if needs_space(s.chars().last(), t.text.chars().next()) {
                s.push(' ');
            }
        }
        s.push_str(&t.text);
        prev_line = Some(t.line);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_paths_under_data() {
        let p = ModelPaths::in_dir("/tmp/ocr-models");
        assert!(p.det.ends_with(DET_FILE));
        assert_eq!(p.downloads().len(), 4);
    }

    #[test]
    fn tokenize_splits_latin_words_and_cjk_chars() {
        let r = Rect::new(0.0, 0.0, 120.0, 16.0);
        let toks = tokenize_line(0, "Hello 你好 PinShot", r);
        let texts: Vec<&str> = toks.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["Hello", "你", "好", "PinShot"]);
        assert!(toks[0].rect.w > toks[1].rect.w);
        assert!((toks.last().unwrap().rect.right() - r.right()).abs() < 0.01);
    }

    #[test]
    fn page_from_blocks_joins_same_line_and_breaks_next() {
        let a = vec![(0, 0), (40, 0), (40, 16), (0, 16)];
        let b = vec![(50, 1), (100, 1), (100, 17), (50, 17)];
        let c = vec![(0, 40), (80, 40), (80, 56), (0, 56)];
        let page = page_from_blocks(vec![
            (a, "In 2009".into()),
            (b, "Mozilla".into()),
            (c, "sponsored".into()),
        ]);
        assert_eq!(page.text, "In 2009 Mozilla\nsponsored");
        assert!(page.tokens.len() >= 3);
        let hit = hit_token(&page.tokens, Point::new(10.0, 8.0));
        assert!(hit.is_some());
        let end = page.tokens.len() - 1;
        let sel = selected_text(&page.tokens, 0, end);
        assert!(sel.contains("Mozilla"));
        assert!(sel.contains('\n'));
    }

    #[test]
    fn selected_text_range_keeps_spaces() {
        let r = Rect::new(0.0, 0.0, 200.0, 16.0);
        let toks = tokenize_line(0, "Mozilla officially sponsored", r);
        assert_eq!(
            selected_text(&toks, 0, toks.len() - 1),
            "Mozilla officially sponsored"
        );
        assert_eq!(selected_text(&toks, 1, 1), "officially");
    }

    #[test]
    fn page_from_plain_covers_image() {
        let page = page_from_plain("Hello world", 100, 40);
        assert_eq!(page.text, "Hello world");
        assert_eq!(page.tokens.len(), 2);
        assert!(hit_token(&page.tokens, Point::new(20.0, 20.0)).is_some());
    }
}

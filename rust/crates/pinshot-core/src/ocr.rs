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

/// RapidOCR rec canvas: height 48, keep the crop aspect, pad right to ≥320.
pub const REC_H: u32 = 48;
pub const REC_MIN_W: u32 = 320;
/// Long Wikipedia lines are ~900×16 → width 2700 at H=48. 1280 would squash.
pub const REC_MAX_W: u32 = 4096;

/// `(resized_w, canvas_w)` for a rec crop. Stretching the line to an 8-aligned
/// width smears `l`/`i`; pad zeros on the right instead (RapidOCR).
pub fn rec_resize_spec(src_w: u32, src_h: u32) -> (u32, u32) {
    let src_h = src_h.max(1) as f32;
    let src_w = src_w.max(1) as f32;
    let ratio = src_w / src_h;
    let max_wh = (REC_MIN_W as f32 / REC_H as f32).max(ratio);
    // RapidOCR: `imgW = int(imgH * max_wh_ratio)` (truncate, not round).
    let mut canvas_w = ((REC_H as f32) * max_wh) as u32;
    canvas_w = canvas_w.clamp(REC_MIN_W, REC_MAX_W);
    let mut resized_w = ((REC_H as f32) * ratio).ceil() as u32;
    if resized_w > canvas_w {
        resized_w = canvas_w;
    }
    (resized_w.max(1), canvas_w.max(REC_H))
}

/// Image-space margin around a DBNet AABB. RapidOCR polygon unclip is looser
/// than connected-component boxes, which otherwise clip `I`/`l`/`f`.
/// Keep Y pad small: extra height shrinks glyphs on the H=48 rec canvas.
pub fn text_crop_pad(box_h: u32) -> (i32, i32) {
    let h = box_h.max(1) as i32;
    ((h / 3).max(8), (h / 10).max(3))
}

/// DBNet probability map → boxes. `prob` is the first channel of the det
/// output, sized `dw × dh` (model input). Mapped back with
/// `x * img_w / dw` (RapidOCR stretch-to-32, no letterbox).
pub fn db_boxes(
    prob: &[f32],
    dw: u32,
    dh: u32,
    img_w: u32,
    img_h: u32,
) -> Vec<TextBox> {
    let dw = dw.max(1) as usize;
    let dh = dh.max(1) as usize;
    if prob.len() < dw * dh {
        return Vec::new();
    }
    const BIN: f32 = 0.3;
    const MIN_SCORE: f32 = 0.5;
    // RapidOCR config.yaml: unclip_ratio 1.6, use_dilation true (2×2).
    const UNCLIP: f32 = 1.6;
    let mut mask = vec![false; dw * dh];
    for y in 0..dh {
        for x in 0..dw {
            if prob[y * dw + x] < BIN {
                continue;
            }
            for dy in 0..2usize {
                for dx in 0..2usize {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < dw && ny < dh {
                        mask[ny * dw + nx] = true;
                    }
                }
            }
        }
    }
    let mut seen = vec![false; dw * dh];
    let mut boxes = Vec::new();
    let mut stack = Vec::new();
    for y0 in 0..dh {
        for x0 in 0..dw {
            let i0 = y0 * dw + x0;
            if seen[i0] || !mask[i0] {
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
                if prob[i] >= BIN {
                    sum += prob[i];
                    n += 1;
                }
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
                    if seen[ni] || !mask[ni] {
                        continue;
                    }
                    seen[ni] = true;
                    stack.push((nx, ny));
                }
            }
            // RapidOCR `min_size` is 3; 8 dropped isolated thin stems (`I`, `l`).
            if n < 3 {
                continue;
            }
            let score = sum / n as f32;
            if score < MIN_SCORE {
                continue;
            }
            let bw = (maxx - minx + 1) as f32;
            let bh = (maxy - miny + 1) as f32;
            let peri = (2.0 * (bw + bh)).max(1.0);
            let d = (UNCLIP * (n as f32) / peri).ceil().max(2.0);
            let x1 = (minx as f32 - d).max(0.0);
            let y1 = (miny as f32 - d).max(0.0);
            let x2 = (maxx as f32 + 1.0 + d).min(dw as f32);
            let y2 = (maxy as f32 + 1.0 + d).min(dh as f32);
            let sx = ((x1 / dw as f32) * img_w as f32).floor().max(0.0) as u32;
            let sy = ((y1 / dh as f32) * img_h as f32).floor().max(0.0) as u32;
            let ex = ((x2 / dw as f32) * img_w as f32).ceil().min(img_w as f32) as u32;
            let ey = ((y2 / dh as f32) * img_h as f32).ceil().min(img_h as f32) as u32;
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
            // Only glue touching glyphs (CJK). English word gaps stay separate so
            // we can insert ASCII spaces when joining.
            let glue = last.h.min(b.h).max(6) / 5;
            if same_line && gap <= glue {
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
    ctc_greedy_blank(indices, alphabet, 0)
}

/// `blank` is the CTC blank class. When blank is 0, alphabet[i-1] is the glyph
/// (PP-OCR). When blank is last, alphabet[i] is the glyph.
///
/// Chinese mobile rec rarely emits the space class. Word gaps are a *longer*
/// CTC blank run than the inter-letter blanks; insert ASCII space there.
pub fn ctc_greedy_blank(indices: &[usize], alphabet: &[String], blank: usize) -> String {
    ctc_decode_fracs(indices, alphabet, blank).0
}

struct CtcGlyph {
    class: usize,
    blanks_before: usize,
    timestep: usize,
}

pub fn ctc_gap_stats(indices: &[usize], blank: usize) -> (usize, usize, usize, usize) {
    let glyphs = ctc_glyphs(indices, blank);
    let gaps: Vec<usize> = glyphs.iter().skip(1).map(|g| g.blanks_before).collect();
    let min = gaps.iter().copied().min().unwrap_or(0);
    let max = gaps.iter().copied().max().unwrap_or(0);
    let mut sorted = gaps;
    sorted.sort_unstable();
    let med = if sorted.is_empty() {
        0
    } else {
        sorted[sorted.len() / 2]
    };
    (glyphs.len(), min, med, max)
}

fn ctc_glyphs(indices: &[usize], blank: usize) -> Vec<CtcGlyph> {
    let mut out = Vec::new();
    let mut prev = usize::MAX;
    let mut blanks = 0usize;
    for (t, &i) in indices.iter().enumerate() {
        if i == blank {
            blanks += 1;
            prev = i;
            continue;
        }
        if i == prev {
            continue;
        }
        out.push(CtcGlyph {
            class: i,
            blanks_before: blanks,
            timestep: t,
        });
        prev = i;
        blanks = 0;
    }
    out
}

/// CTC decode plus each glyph's horizontal fraction along the rec window.
pub fn ctc_decode_fracs(
    indices: &[usize],
    alphabet: &[String],
    blank: usize,
) -> (String, Vec<f32>) {
    let glyphs = ctc_glyphs(indices, blank);
    let mut gaps: Vec<usize> = glyphs.iter().skip(1).map(|g| g.blanks_before).collect();
    gaps.sort_unstable();
    let median = if gaps.is_empty() {
        0
    } else {
        gaps[gaps.len() / 2]
    };
    let word_gap = (median * 2 + 2).max(4);
    let tlen = indices.len().max(1) as f32;
    let mut out = String::new();
    let mut fracs = Vec::new();
    for (n, g) in glyphs.iter().enumerate() {
        // PP-OCR / RapidOCR: class 0 is CTC blank, alphabet[i-1] is the glyph,
        // and the extra trailing class (keys.len()+1) is the space token from
        // `use_space_char`. That token is the primary English word space.
        let ch = ctc_class_glyph(g.class, alphabet, blank);
        if n > 0 && g.blanks_before >= word_gap && !out.ends_with(' ') {
            let next_cjk = ch.and_then(|s| s.chars().next()).is_some_and(is_cjk);
            let prev_cjk = out.chars().last().is_some_and(is_cjk);
            if !(prev_cjk && next_cjk) {
                out.push(' ');
                fracs.push(g.timestep as f32 / tlen);
            }
        }
        if let Some(ch) = ch {
            if ch == " " && out.ends_with(' ') {
                continue;
            }
            let frac = g.timestep as f32 / tlen;
            for _ in ch.chars() {
                fracs.push(frac);
            }
            out.push_str(ch);
        }
    }
    (out, fracs)
}

/// Map a CTC class to a dict glyph. Out-of-range is the rec model's space class.
fn ctc_class_glyph(class: usize, alphabet: &[String], blank: usize) -> Option<&str> {
    if class == blank {
        return None;
    }
    if blank == 0 {
        match alphabet.get(class.saturating_sub(1)) {
            Some(s) => Some(s.as_str()),
            None => Some(" "),
        }
    } else {
        match alphabet.get(class) {
            Some(s) => Some(s.as_str()),
            None => Some(" "),
        }
    }
}

pub fn load_keys(text: &str) -> Vec<String> {
    // Paddle/RapidOCR dicts use a blank (or space-only) first line for ' '.
    // Dropping it shifts every class and the rec stage emits garbage / nothing.
    text.lines()
        .map(|l| {
            let l = l.trim_end_matches('\r');
            if l.is_empty() || l == "\u{3000}" || l == "\u{00a0}" {
                " ".to_string()
            } else {
                l.to_string()
            }
        })
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

/// Join recognized snippets using box layout: space between words on the
/// same line, and space (not a mid-sentence newline) between stacked lines
/// of one paragraph.
pub fn join_ocr_parts(parts: &[(TextBox, String)]) -> String {
    let mut out = String::new();
    let mut prev: Option<(TextBox, String)> = None;
    for (b, raw) in parts {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        if let Some((p, ref prev_t)) = prev {
            let cy = b.y + b.h / 2;
            let py = p.y + p.h / 2;
            let same_line = (cy as i32 - py as i32).unsigned_abs() < p.h.max(b.h) / 2 + 4;
            if same_line {
                let gap = b.x.saturating_sub(p.x + p.w);
                if gap > p.h.max(b.h).max(8) / 5 && !out.ends_with(' ') {
                    out.push(' ');
                }
            } else {
                let stack_gap = b.y.saturating_sub(p.y + p.h);
                let close = stack_gap <= p.h.max(b.h).saturating_mul(3) / 2 + 4;
                if close && !ends_sentence(prev_t) {
                    if out.ends_with('-') {
                        out.pop();
                    } else if !out.ends_with(' ') {
                        out.push(' ');
                    }
                } else {
                    out.push('\n');
                }
            }
        }
        out.push_str(t);
        prev = Some((*b, t.to_string()));
    }
    out
}

fn ends_sentence(s: &str) -> bool {
    s.trim_end()
        .chars()
        .last()
        .is_some_and(|c| ".!?。！？…".contains(c))
}

fn is_cjk(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
        || ('\u{3400}'..='\u{4dbf}').contains(&c)
        || ('\u{3040}'..='\u{30ff}').contains(&c)
}

/// PP-OCRv5 often emits `In2009,Mozilla` with no ASCII spaces. Restore the
/// obvious Latin boundaries without splitting `i.e.` / `2010s`.
pub fn latin_space_tweak(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    for (i, &b) in chars.iter().enumerate() {
        if i > 0 {
            let a = chars[i - 1];
            if !a.is_whitespace() && !b.is_whitespace() {
                let punct_word = match a {
                    ',' | ';' | ':' => b.is_ascii_alphabetic(),
                    '.' | '!' | '?' => b.is_ascii_uppercase(),
                    ')' | ']' => b.is_ascii_alphanumeric(),
                    _ => false,
                };
                let letter_digit = a.is_ascii_alphabetic() && b.is_ascii_digit();
                let digit_letter = a.is_ascii_digit()
                    && b.is_ascii_alphabetic()
                    && !matches!(b, 's' | 'S');
                if punct_word || letter_digit || digit_letter {
                    out.push(' ');
                }
            }
        }
        out.push(b);
    }
    out
}

/// Column ink (count of dark pixels per x). Gaps narrower than ~0.35 of a
/// typical glyph are treated as letter spacing, not word spaces.
pub fn word_gap_fracs(ink: &[u32], _height: u32) -> Vec<f32> {
    let w = ink.len() as u32;
    if w < 8 {
        return Vec::new();
    }
    let peak = ink.iter().copied().max().unwrap_or(0);
    if peak < 2 {
        return Vec::new();
    }
    let empty_max = (peak / 6).max(1);
    let mut raw: Vec<(u32, u32)> = Vec::new();
    let mut x = 0u32;
    while x < w {
        while x < w && ink[x as usize] <= empty_max {
            x += 1;
        }
        if x >= w {
            break;
        }
        let start = x;
        while x < w && ink[x as usize] > empty_max {
            x += 1;
        }
        if x > start + 1 {
            raw.push((start, x));
        }
    }
    if raw.len() < 2 {
        return Vec::new();
    }
    let mut widths: Vec<u32> = raw.iter().map(|(s, e)| e.saturating_sub(*s)).collect();
    widths.sort_unstable();
    let letter_w = widths[widths.len() / 2].max(4);
    let min_gap = ((letter_w as f32) * 0.38).round().clamp(3.0, 28.0) as u32;
    let mut spans: Vec<(u32, u32)> = Vec::new();
    for (s, e) in raw {
        if let Some(last) = spans.last_mut() {
            if s.saturating_sub(last.1) < min_gap {
                last.1 = e;
                continue;
            }
        }
        spans.push((s, e));
    }
    if spans.len() < 2 {
        return Vec::new();
    }
    let width = w.max(1) as f32;
    spans
        .windows(2)
        .map(|w| ((w[0].1 + w[1].0) as f32 * 0.5) / width)
        .collect()
}

/// Insert ASCII spaces where ink gaps fall between CTC glyphs (or, if glyph
/// fractions are missing, by proportional character index).
pub fn insert_spaces_aligned(text: &str, glyph_fracs: &[f32], gap_fracs: &[f32]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    if n == 0 || gap_fracs.is_empty() {
        return text.to_string();
    }
    // Letter-level cuts (more gaps than ~¼ of the glyphs) are tracking, not words.
    if gap_fracs.len() > (n / 4).max(2) {
        return text.to_string();
    }
    let fracs: Vec<f32> = if glyph_fracs.len() == n {
        glyph_fracs.to_vec()
    } else {
        (0..n).map(|i| (i as f32 + 0.5) / n as f32).collect()
    };
    let mut cut = vec![false; n];
    for &g in gap_fracs {
        let mut i = 1usize;
        while i < n && fracs[i] < g {
            i += 1;
        }
        if i == 0 || i >= n {
            continue;
        }
        if is_cjk(chars[i - 1]) && is_cjk(chars[i]) {
            continue;
        }
        if chars[i - 1].is_whitespace() || chars[i].is_whitespace() {
            continue;
        }
        cut[i] = true;
    }
    let mut out = String::with_capacity(n + gap_fracs.len());
    for (i, &ch) in chars.iter().enumerate() {
        if cut[i] && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// Keep the rec model's space token / CTC blank-run spaces. Only restore the
/// obvious Latin letter↔digit and punctuation boundaries — geometric ink
/// gaps and a greedy English dictionary split letters inside real words.
pub fn restore_english_spaces(text: &str, _glyph_fracs: &[f32], _gap_fracs: &[f32]) -> String {
    latin_space_tweak(text)
}

/// Common English tokens, longest first. Used only on long letter-runs that
/// PP-OCRv5 emitted without spaces (`Mozillaofficiallysponsoredtheproject`).
const EN_WORDS: &[&str] = &[
    "officially",
    "sponsored",
    "wikipedia",
    "something",
    "together",
    "project",
    "mozilla",
    "through",
    "between",
    "because",
    "without",
    "another",
    "against",
    "however",
    "welcome",
    "example",
    "english",
    "chinese",
    "version",
    "release",
    "license",
    "source",
    "public",
    "number",
    "people",
    "should",
    "before",
    "change",
    "system",
    "during",
    "around",
    "always",
    "really",
    "please",
    "thanks",
    "update",
    "create",
    "search",
    "result",
    "window",
    "screen",
    "hello",
    "world",
    "about",
    "after",
    "first",
    "their",
    "there",
    "which",
    "would",
    "other",
    "these",
    "those",
    "could",
    "where",
    "under",
    "using",
    "still",
    "every",
    "never",
    "right",
    "small",
    "large",
    "start",
    "close",
    "image",
    "click",
    "input",
    "text",
    "from",
    "this",
    "that",
    "with",
    "have",
    "been",
    "were",
    "will",
    "when",
    "what",
    "your",
    "them",
    "then",
    "than",
    "also",
    "into",
    "over",
    "only",
    "some",
    "more",
    "most",
    "such",
    "make",
    "like",
    "time",
    "year",
    "used",
    "file",
    "open",
    "save",
    "copy",
    "edit",
    "page",
    "line",
    "code",
    "data",
    "name",
    "next",
    "last",
    "both",
    "each",
    "many",
    "well",
    "just",
    "very",
    "back",
    "good",
    "work",
    "the",
    "and",
    "for",
    "are",
    "but",
    "not",
    "you",
    "all",
    "can",
    "had",
    "her",
    "was",
    "one",
    "our",
    "out",
    "has",
    "his",
    "how",
    "its",
    "may",
    "new",
    "now",
    "old",
    "see",
    "two",
    "way",
    "who",
    "did",
    "get",
    "got",
    "let",
    "put",
    "say",
    "she",
    "too",
    "use",
];

fn dict_match_at(lower: &str, i: usize) -> Option<usize> {
    let rest = &lower[i..];
    for w in EN_WORDS {
        if rest.starts_with(w) {
            return Some(w.len());
        }
    }
    None
}

/// Break a long `[A-Za-z]+` run with a greedy longest-dictionary match,
/// keeping unknown prefixes (`Mozilla` + `officially` …).
pub fn break_latin_run(run: &str) -> String {
    if run.len() < 12 {
        return run.to_string();
    }
    let lower = run.to_ascii_lowercase();
    let bytes = run.as_bytes();
    let mut i = 0usize;
    let mut parts: Vec<&str> = Vec::new();
    while i < bytes.len() {
        if let Some(n) = dict_match_at(&lower, i) {
            parts.push(&run[i..i + n]);
            i += n;
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && dict_match_at(&lower, j).is_none() {
            j += 1;
        }
        parts.push(&run[i..j]);
        i = j;
    }
    parts.join(" ")
}

pub fn segment_glued_latin(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    let mut i = 0usize;
    while i < chars.len() {
        if !chars[i].is_ascii_alphabetic() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_alphabetic() {
            i += 1;
        }
        let run: String = chars[start..i].iter().collect();
        out.push_str(&break_latin_run(&run));
    }
    out
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
    fn ctc_inserts_space_on_blank_runs() {
        let alpha = vec!["I".into(), "n".into(), "2".into(), "0".into(), "9".into()];
        // In ··· 2009  (two+ blanks between words)
        let idx = [0, 1, 2, 0, 0, 0, 0, 3, 4, 0, 4, 5];
        assert_eq!(ctc_greedy(&idx, &alpha), "In 2009");
        // CJK with a single blank between glyphs stays unspaced.
        let zh = vec!["你".into(), "好".into()];
        assert_eq!(ctc_greedy(&[0, 1, 0, 2, 0], &zh), "你好");
    }

    #[test]
    fn ctc_trailing_class_is_the_space_token() {
        // RapidOCR: classes = keys + 2 (blank + trailing ' ').
        let alpha = vec!["H".into(), "i".into(), "t".into()];
        // H · space-class · i t
        let space = alpha.len() + 1;
        let idx = [0, 1, space, 2, 3];
        assert_eq!(ctc_greedy(&idx, &alpha), "H it");
        assert_eq!(
            restore_english_spaces("officially", &[], &[]),
            "officially",
            "must not split inside words"
        );
        assert_eq!(
            restore_english_spaces("contributors", &[], &[]),
            "contributors"
        );
        assert_eq!(
            restore_english_spaces("In2009,Mozilla", &[], &[]),
            "In 2009, Mozilla"
        );
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
    fn load_keys_keeps_blank_as_space() {
        let keys = load_keys("\n你\n好\n \nA\n");
        assert_eq!(keys[0], " ");
        assert_eq!(keys[1], "你");
        assert_eq!(keys[3], " ");
        assert_eq!(keys[4], "A");
        let ideo = load_keys("\u{3000}\n你\n");
        assert_eq!(ideo[0], " ");
        assert_eq!(ideo[1], "你");
    }

    #[test]
    fn join_ocr_parts_inserts_english_spaces() {
        let a = TextBox {
            x: 0,
            y: 0,
            w: 40,
            h: 20,
            score: 1.0,
        };
        let b = TextBox {
            x: 55,
            y: 1,
            w: 50,
            h: 20,
            score: 1.0,
        };
        let c = TextBox {
            x: 0,
            y: 30,
            w: 80,
            h: 20,
            score: 1.0,
        };
        let s = join_ocr_parts(&[
            (a, "In2009".into()),
            (b, "Mozilla".into()),
            (c, "sponsored".into()),
        ]);
        assert_eq!(s, "In2009 Mozilla sponsored");
        let d = TextBox {
            x: 0,
            y: 80,
            w: 80,
            h: 20,
            score: 1.0,
        };
        let para = join_ocr_parts(&[
            (a, "Hello.".into()),
            (d, "Next paragraph".into()),
        ]);
        assert_eq!(para, "Hello.\nNext paragraph");
    }

    #[test]
    fn latin_space_tweak_splits_digit_and_punct() {
        assert_eq!(
            latin_space_tweak("In2009,Mozilla"),
            "In 2009, Mozilla"
        );
        assert_eq!(latin_space_tweak("你好PinShot"), "你好PinShot");
        assert_eq!(latin_space_tweak("Hello world"), "Hello world");
        assert_eq!(latin_space_tweak("i.e.,"), "i.e.,");
        assert_eq!(latin_space_tweak("2010s"), "2010s");
        assert_eq!(latin_space_tweak("[21]A"), "[21] A");
        assert_eq!(
            latin_space_tweak("end.Next"),
            "end. Next"
        );
    }

    #[test]
    fn rec_resize_spec_keeps_aspect_and_pads() {
        let (rw, cw) = rec_resize_spec(80, 16);
        assert_eq!(rw, 240);
        assert_eq!(cw, 320, "short lines pad to RapidOCR rec_img_shape width");
        let (rw, cw) = rec_resize_spec(400, 16);
        assert_eq!(rw, 1200);
        assert_eq!(cw, 1200);
        let (rw, cw) = rec_resize_spec(16, 16);
        assert_eq!(rw, 48);
        assert_eq!(cw, 320);
        let (rw, cw) = rec_resize_spec(900, 16);
        assert_eq!(rw, 2700);
        assert_eq!(cw, 2700, "long lines keep aspect; do not squash to 1280");
        assert_eq!(text_crop_pad(16), (8, 3));
        assert_eq!(text_crop_pad(30), (10, 3));
    }

    #[test]
    fn insert_spaces_aligned_uses_gap_fracs() {
        let text = "Mozillaofficiallysponsored";
        let n = text.chars().count();
        let fracs: Vec<f32> = (0..n).map(|i| (i as f32 + 0.5) / n as f32).collect();
        // Gaps at Mozilla|officially and officially|sponsored
        let moz = "Mozilla".chars().count() as f32 / n as f32;
        let off = "Mozillaofficially".chars().count() as f32 / n as f32;
        let s = insert_spaces_aligned(text, &fracs, &[moz, off]);
        assert_eq!(s, "Mozilla officially sponsored");
        assert_eq!(
            restore_english_spaces("In2009,Mozillaofficially", &[], &[]),
            "In 2009, Mozillaofficially"
        );
        assert_eq!(
            segment_glued_latin("Mozillaofficiallysponsoredtheproject."),
            "Mozilla officially sponsored the project."
        );
        let glued = "In2009,Hozillaofficiallysponsoredtheproject.";
        let too_many: Vec<f32> = (0..23).map(|i| (i as f32 + 1.0) / 24.0).collect();
        let restored = restore_english_spaces(glued, &[], &too_many);
        assert!(
            restored.contains("officially") && !restored.contains("o f f"),
            "{restored:?}"
        );
        assert!(
            !restored.contains("all y") && !restored.contains("but or"),
            "dictionary must not split inside words: {restored:?}"
        );
    }

    #[test]
    fn word_gap_fracs_finds_wide_columns() {
        // 8px words with a 6px hole: #####......#####
        let mut ink = vec![0u32; 24];
        for x in 0..8 {
            ink[x] = 10;
        }
        for x in 14..22 {
            ink[x] = 10;
        }
        let gaps = word_gap_fracs(&ink, 12);
        assert_eq!(gaps.len(), 1);
        assert!((gaps[0] - 0.458).abs() < 0.08, "{gaps:?}");
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
        let boxes = db_boxes(&prob, dw, dh, 320, 320);
        assert!(
            boxes.len() >= 2,
            "expected two text bands, got {:?}",
            boxes
        );
        assert!(boxes[0].y < boxes[1].y);
        assert!(boxes.iter().all(|b| b.w > 10 && b.h > 4));
    }
}

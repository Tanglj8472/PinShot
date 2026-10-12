//! PP-OCRv5 model layout and in-place OCR tokens (boxes from paddle-ocr-rs).

use std::path::{Path, PathBuf};

use crate::geom::{Point, Rect};

pub const MODEL_DIR_NAME: &str = "ocr-models";

/// RapidOCR PP-OCRv5 ONNX (ModelScope v3.9.2). Server (~170 MB) is the
/// default: mobile (~20 MB) loses English stems on real Chrome text.
pub const MOBILE_DET_FILE: &str = "ch_PP-OCRv5_mobile_det.onnx";
pub const MOBILE_REC_FILE: &str = "ch_PP-OCRv5_mobile_rec.onnx";
pub const SERVER_DET_FILE: &str = "ch_PP-OCRv5_server_det.onnx";
pub const SERVER_REC_FILE: &str = "ch_PP-OCRv5_server_rec.onnx";
pub const DET_FILE: &str = SERVER_DET_FILE;
pub const REC_FILE: &str = SERVER_REC_FILE;
pub const CLS_FILE: &str = "ch_PP-LCNet_x0_25_textline_ori.onnx";
pub const KEYS_FILE: &str = "ppocr_keys_v5.txt";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OcrModelKind {
    Mobile,
    Server,
}

impl OcrModelKind {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "mobile" => Self::Mobile,
            _ => Self::Server,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mobile => "mobile",
            Self::Server => "server",
        }
    }

    pub fn det_file(self) -> &'static str {
        match self {
            Self::Mobile => MOBILE_DET_FILE,
            Self::Server => SERVER_DET_FILE,
        }
    }

    pub fn rec_file(self) -> &'static str {
        match self {
            Self::Mobile => MOBILE_REC_FILE,
            Self::Server => SERVER_REC_FILE,
        }
    }
}

/// RapidOCR defaults: padding, max side, box_score_thresh, box_thresh, unclip.
/// Unclip stays 1.6: 2.0 merged adjacent Latin words into one blob.
pub const DET_PADDING: u32 = 50;
pub const DET_MAX_SIDE: u32 = 2560;
pub const DET_BOX_SCORE: f32 = 0.5;
pub const DET_BOX_THRESH: f32 = 0.3;
pub const DET_UNCLIP: f32 = 1.6;

/// RapidOCR uses 50px pad, but a 2-line 58px crop would become mostly pad
/// and drop the first line. Keep the default on article-sized clips.
pub fn det_padding(width: u32, height: u32) -> u32 {
    let side = width.min(height).max(8);
    DET_PADDING.min(side / 4).max(8)
}

/// paddle-ocr-rs rec is hardcoded to H=48. Chrome Wikipedia is ~13px
/// (glyph band ~14px). 1×/2× det misses those lines (`T` / `In` only);
/// 3× on a real 790×120 F1 crop recovered the 2010s sentence with spaces.
/// Full-screen stays 1× so det does not melt columns. Unclip stays 1.6.
pub fn ocr_upsample(width: u32, height: u32) -> u32 {
    if let Ok(v) = std::env::var("PINSHOT_OCR_SCALE") {
        if let Ok(n) = v.trim().parse::<u32>() {
            return n.clamp(1, 4);
        }
    }
    let short = width.min(height);
    if short < 480 {
        3
    } else if short < 900 {
        2
    } else {
        1
    }
}

/// RapidOCR / PaddleOCR v5 ONNX. First URL is ModelScope v3.9.2.
pub const MOBILE_DET_URLS: &[&str] = &[
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_mobile.onnx",
    "https://huggingface.co/RapidAI/RapidOCR/resolve/main/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_mobile.onnx",
];
pub const MOBILE_REC_URLS: &[&str] = &[
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile.onnx",
    "https://huggingface.co/RapidAI/RapidOCR/resolve/main/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile.onnx",
];
pub const SERVER_DET_URLS: &[&str] = &[
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_server.onnx",
    "https://huggingface.co/RapidAI/RapidOCR/resolve/main/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_server.onnx",
];
pub const SERVER_REC_URLS: &[&str] = &[
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_server.onnx",
    "https://huggingface.co/RapidAI/RapidOCR/resolve/main/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_server.onnx",
];
pub const DET_URLS: &[&str] = SERVER_DET_URLS;
pub const REC_URLS: &[&str] = SERVER_REC_URLS;
pub const CLS_URLS: &[&str] = &[
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/cls/ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx",
    "https://huggingface.co/RapidAI/RapidOCR/resolve/main/onnx/PP-OCRv5/cls/ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx",
];
pub const KEYS_URLS: &[&str] = &[
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/paddle/PP-OCRv5/rec/ch_PP-OCRv5_rec_server/ppocrv5_dict.txt",
    "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/paddle/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile/ppocrv5_dict.txt",
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
        Self::for_kind(dir, OcrModelKind::Server)
    }

    pub fn for_kind(dir: impl Into<PathBuf>, kind: OcrModelKind) -> Self {
        let dir = dir.into();
        Self {
            det: dir.join(kind.det_file()),
            rec: dir.join(kind.rec_file()),
            keys: dir.join(KEYS_FILE),
            cls: dir.join(CLS_FILE),
            dir,
        }
    }

    pub fn ready(&self) -> bool {
        self.det.is_file() && self.rec.is_file() && self.cls.is_file()
    }

    pub fn downloads(&self) -> Vec<(Vec<&'static str>, PathBuf)> {
        let server = self.det.file_name().is_some_and(|n| n == SERVER_DET_FILE);
        let det = if server {
            SERVER_DET_URLS
        } else {
            MOBILE_DET_URLS
        };
        let rec = if server {
            SERVER_REC_URLS
        } else {
            MOBILE_REC_URLS
        };
        vec![
            (det.to_vec(), self.det.clone()),
            (rec.to_vec(), self.rec.clone()),
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

/// Drop OCR wrap spaces that split CJK (`編譯 器`, `2009 年`).
/// Keep CJK–Latin spaces as they appear (`Mozilla 員工`, `Rust 語言`).
/// Latin word spaces (`In 2009`, `Mozilla officially`) stay.
pub fn collapse_cjk_spaces(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            let mut j = i;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if glue_cjk_gap(out.chars().last(), chars.get(j).copied()) {
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn glue_cjk_gap(prev: Option<char>, next: Option<char>) -> bool {
    match (prev, next) {
        (Some(a), Some(b)) if is_cjk(a) && is_cjk(b) => true,
        (Some(a), Some(b)) if a.is_ascii_digit() && is_cjk(b) => true,
        (Some(a), Some(b)) if is_cjk(a) && b.is_ascii_digit() => true,
        _ => false,
    }
}

/// Split a line box into CJK characters / Latin words, mapped by character width.
pub fn tokenize_line(line: usize, text: &str, box_rect: Rect) -> Vec<OcrToken> {
    let text = collapse_cjk_spaces(text.trim());
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

fn is_bracket(c: char) -> bool {
    matches!(
        c,
        '[' | ']' | '【' | '】' | '(' | ')' | '（' | '）' | '「' | '」'
    )
}

fn needs_space(prev: Option<char>, next: Option<char>) -> bool {
    match (prev, next) {
        (Some(a), Some(b)) if is_bracket(a) || is_bracket(b) => false,
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

/// `[16]`, `16]`, `[15`, `5]` — superscript citation fragments. These must
/// not expand a line's body band, or the next paragraph (`5]，並於2011年`)
/// merges onto `稱做「rustc」`.
fn is_citation_fragment(text: &str) -> bool {
    let stripped: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if stripped.is_empty() || stripped.chars().count() > 8 {
        return false;
    }
    if stripped.chars().all(is_bracket) {
        return true;
    }
    let core: String = stripped.chars().filter(|c| !is_bracket(*c)).collect();
    !core.is_empty() && core.chars().all(|c| c.is_ascii_digit())
}

fn translate_join_space(prev: Option<char>, next: Option<char>) -> bool {
    match (prev, next) {
        (Some(a), Some(b)) if a.is_whitespace() || b.is_whitespace() => false,
        (Some(a), Some(b)) if is_cjk(a) || is_cjk(b) => false,
        (Some(_), Some(_)) => true,
        _ => false,
    }
}

fn ends_sentence(s: &str) -> bool {
    s.chars()
        .rev()
        .find(|c| !c.is_whitespace())
        .is_some_and(|c| matches!(c, '.' | '!' | '?' | '。' | '！' | '？' | '…'))
}

/// Merge wrapped OCR lines into sentences before sending to a translator.
/// Line-by-line Edge/OpenAI calls split paragraphs and drop context.
pub fn page_for_translate(text: &str) -> String {
    let mut paras: Vec<String> = Vec::new();
    let mut buf = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            if !buf.is_empty() {
                paras.push(std::mem::take(&mut buf));
            }
            continue;
        }
        if buf.is_empty() {
            buf.push_str(line);
            continue;
        }
        if ends_sentence(&buf) {
            paras.push(std::mem::take(&mut buf));
            buf.push_str(line);
        } else {
            if translate_join_space(buf.chars().last(), line.chars().next()) {
                buf.push(' ');
            }
            buf.push_str(line);
        }
    }
    if !buf.is_empty() {
        paras.push(buf);
    }
    paras.join("\n")
}

/// If det cropped `[15]` into `15]`, put the bracket back. Only at line
/// start so years (`2011年`) stay untouched.
fn repair_leading_cite(s: &str) -> String {
    let t = s.trim_start();
    if t.starts_with('[') || t.starts_with('【') {
        return s.to_string();
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits <= 3 {
        let rest: String = t.chars().skip(digits).collect();
        if rest.starts_with(']') || rest.starts_with('】') {
            let ws = s.len() - t.len();
            return format!("{}[{}", &s[..ws], t);
        }
    }
    s.to_string()
}

fn y_overlap_ratio(a: Rect, b: Rect) -> f32 {
    let top = a.y.max(b.y);
    let bot = a.bottom().min(b.bottom());
    let ov = (bot - top).max(0.0);
    ov / a.h.min(b.h).max(1.0)
}

fn sort_boxes_yx(items: &mut [(Rect, String)]) {
    items.sort_by(|a, b| {
        a.0.y
            .partial_cmp(&b.0.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.0.x
                    .partial_cmp(&b.0.x)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
}

/// Cluster body boxes by y-overlap. Citation fragments attach afterwards by
/// y-center so a superscript `[16]` cannot widen the band onto the next line.
fn cluster_lines(items: Vec<(Rect, String)>) -> Vec<Vec<(Rect, String)>> {
    let mut body = Vec::new();
    let mut cites = Vec::new();
    for item in items {
        if is_citation_fragment(&item.1) {
            cites.push(item);
        } else {
            body.push(item);
        }
    }
    if body.is_empty() {
        return cluster_by_overlap(cites);
    }
    let mut lines = cluster_by_overlap(body);
    for cite in cites {
        let ic = cite.0.center().y;
        let mut best: Option<(usize, f32)> = None;
        for (i, line) in lines.iter().enumerate() {
            let band = line_band(line);
            if band.is_empty() {
                continue;
            }
            let dist = (ic - band.center().y).abs();
            let lim = band.h.max(12.0) * 1.1;
            if dist <= lim && best.map(|(_, d)| dist < d).unwrap_or(true) {
                best = Some((i, dist));
            }
        }
        if let Some((i, _)) = best {
            lines[i].push(cite);
        } else {
            lines.push(vec![cite]);
        }
    }
    lines.sort_by(|a, b| {
        line_band(a)
            .y
            .partial_cmp(&line_band(b).y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for boxes in &mut lines {
        boxes.sort_by(|a, b| {
            a.0.x
                .partial_cmp(&b.0.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    lines
}

fn line_band(boxes: &[(Rect, String)]) -> Rect {
    boxes.iter().fold(Rect::EMPTY, |acc, (r, _)| acc.union(*r))
}

fn cluster_by_overlap(mut items: Vec<(Rect, String)>) -> Vec<Vec<(Rect, String)>> {
    sort_boxes_yx(&mut items);
    let mut lines: Vec<(Rect, Vec<(Rect, String)>)> = Vec::new();
    for item in items {
        let mut best: Option<(usize, f32)> = None;
        for (i, (band, _)) in lines.iter().enumerate() {
            let ov = y_overlap_ratio(item.0, *band);
            if ov >= 0.5 && best.map(|(_, r)| ov > r).unwrap_or(true) {
                best = Some((i, ov));
            }
        }
        if let Some((i, _)) = best {
            lines[i].0 = lines[i].0.union(item.0);
            lines[i].1.push(item);
        } else {
            lines.push((item.0, vec![item]));
        }
    }
    lines.sort_by(|a, b| {
        a.0.y
            .partial_cmp(&b.0.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (_, boxes) in &mut lines {
        boxes.sort_by(|a, b| {
            a.0.x
                .partial_cmp(&b.0.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    lines.into_iter().map(|(_, boxes)| boxes).collect()
}

/// Build a page from detector quadrilaterals + recognized line strings.
pub fn page_from_blocks(blocks: Vec<(Vec<(u32, u32)>, String)>) -> OcrPage {
    let items: Vec<(Rect, String)> = blocks
        .into_iter()
        .filter_map(|(pts, text)| {
            let r = aabb_from_quad(&pts);
            let t = text.trim().to_string();
            if t.is_empty() || r.is_empty() {
                None
            } else {
                Some((r, collapse_cjk_spaces(&t)))
            }
        })
        .collect();

    let mut tokens = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for (line_i, line_boxes) in cluster_lines(items).into_iter().enumerate() {
        let mut line_parts: Vec<String> = Vec::new();
        for (rect, text) in line_boxes {
            tokens.extend(tokenize_line(line_i, &text, rect));
            line_parts.push(text);
        }
        if !line_parts.is_empty() {
            lines.push(repair_leading_cite(&collapse_cjk_spaces(&join_line_parts(
                &line_parts,
            ))));
        }
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
    tokens.iter().position(|t| t.rect.contains(p)).or_else(|| {
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

/// Inclusive reading-order range of tokens whose boxes meet `r`.
pub fn tokens_in_rect(tokens: &[OcrToken], r: Rect) -> Option<(usize, usize)> {
    let r = if r.w < 2.0 || r.h < 2.0 {
        Rect::new(
            r.x - if r.w < 2.0 { 1.0 } else { 0.0 },
            r.y - if r.h < 2.0 { 1.0 } else { 0.0 },
            r.w.max(2.0),
            r.h.max(2.0),
        )
    } else {
        r
    };
    let mut lo = None::<usize>;
    let mut hi = None::<usize>;
    for (i, t) in tokens.iter().enumerate() {
        if t.rect.intersects(r) {
            lo = Some(lo.map_or(i, |n| n.min(i)));
            hi = Some(hi.map_or(i, |n| n.max(i)));
        }
    }
    Some((lo?, hi?))
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
        assert!(p.det.ends_with(SERVER_DET_FILE));
        assert_eq!(p.downloads().len(), 4);
        assert!(p.downloads()[0].0[0].contains("det_server"));
        let m = ModelPaths::for_kind("/tmp/ocr-models", OcrModelKind::Mobile);
        assert!(m.det.ends_with(MOBILE_DET_FILE));
        assert!(m.downloads()[0].0[0].contains("det_mobile"));
    }

    #[test]
    fn det_padding_shrinks_on_thin_clips() {
        assert_eq!(det_padding(1295, 58), 14);
        assert_eq!(det_padding(1100, 400), 50);
        assert_eq!(det_padding(8, 8), 8);
    }

    #[test]
    fn ocr_upsample_lifts_short_chrome_clips() {
        assert_eq!(ocr_upsample(1100, 400), 3);
        assert_eq!(ocr_upsample(900, 120), 3);
        assert_eq!(ocr_upsample(1920, 1200), 1);
        assert_eq!(ocr_upsample(2000, 300), 3);
        assert_eq!(ocr_upsample(800, 479), 3);
        assert_eq!(ocr_upsample(800, 480), 2);
        assert_eq!(ocr_upsample(1000, 900), 1);
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
        let band = Rect::from_xyxy(
            toks[0].rect.x,
            toks[0].rect.y,
            toks[1].rect.right(),
            toks[1].rect.bottom(),
        );
        assert_eq!(tokens_in_rect(&toks, band), Some((0, 1)));
        assert_eq!(selected_text(&toks, 0, 1), "Mozilla officially");
    }

    #[test]
    fn page_from_plain_covers_image() {
        let page = page_from_plain("Hello world", 100, 40);
        assert_eq!(page.text, "Hello world");
        assert_eq!(page.tokens.len(), 2);
        assert!(hit_token(&page.tokens, Point::new(20.0, 20.0)).is_some());
    }

    #[test]
    fn collapse_cjk_wrap_spaces_keeps_latin() {
        assert_eq!(collapse_cjk_spaces("編譯 器"), "編譯器");
        assert_eq!(collapse_cjk_spaces("2009 年"), "2009年");
        assert_eq!(collapse_cjk_spaces("Rust 語言"), "Rust 語言");
        assert_eq!(collapse_cjk_spaces("Mozilla 員工"), "Mozilla 員工");
        assert_eq!(
            collapse_cjk_spaces("In 2009, Mozilla officially"),
            "In 2009, Mozilla officially"
        );
    }

    #[test]
    fn page_keeps_visual_lines_by_y_overlap() {
        let a = vec![(0, 0), (80, 0), (80, 16), (0, 16)];
        let b = vec![(0, 20), (48, 20), (48, 36), (0, 36)];
        let page = page_from_blocks(vec![(a, "Rust語".into()), (b, "言".into())]);
        assert_eq!(page.text, "Rust語\n言");
        assert!(page.tokens.iter().any(|t| t.line == 0));
        assert!(page.tokens.iter().any(|t| t.line == 1));
        let sel = selected_text(&page.tokens, 0, page.tokens.len() - 1);
        assert_eq!(sel, "Rust語\n言");
    }

    #[test]
    fn page_does_not_merge_rustc_into_a_later_line() {
        let l1 = vec![(0, 0), (200, 0), (200, 18), (0, 18)];
        let l2 = vec![(0, 22), (240, 22), (240, 40), (0, 40)];
        let l3a = vec![(0, 44), (40, 44), (40, 62), (0, 62)];
        let l3b = vec![(48, 45), (120, 45), (120, 61), (48, 61)];
        let page = page_from_blocks(vec![
            (l1, "並且在2010年首次發布".into()),
            (l2, "官方實現稱做「rustc」".into()),
            (l3a, "在架".into()),
            (l3b, "構上".into()),
        ]);
        assert!(
            page.text.contains("稱做「rustc」"),
            "rustc left its line: {:?}",
            page.text
        );
        assert!(
            !page.text.contains("在架稱做"),
            "rustc jumped into a later line: {:?}",
            page.text
        );
        let lines: Vec<&str> = page.text.lines().collect();
        assert!(
            lines
                .iter()
                .any(|l| l.contains("稱做「rustc」") && !l.contains("在架")),
            "expected rustc on its own visual line, got {lines:?}"
        );
        assert!(
            page.text.contains("在架構上"),
            "same-line CJK should still join, got {:?}",
            page.text
        );
    }

    #[test]
    fn page_collapses_cjk_spaces_inside_a_block() {
        let a = vec![(0, 0), (80, 0), (80, 16), (0, 16)];
        let compiler = page_from_blocks(vec![(a.clone(), "編譯 器".into())]);
        assert_eq!(compiler.text, "編譯器");
        let year = page_from_blocks(vec![(a, "2009 年".into())]);
        assert_eq!(year.text, "2009年");
    }

    #[test]
    fn page_does_not_merge_citation_onto_next_paragraph() {
        let rustc = vec![(0, 0), (220, 0), (220, 18), (0, 18)];
        let cite = vec![(200, 0), (218, 0), (218, 12), (200, 12)];
        let next = vec![(0, 22), (280, 22), (280, 40), (0, 40)];
        let page = page_from_blocks(vec![
            (rustc, "官方實現稱做「rustc」的編譯器。".into()),
            (cite, "[16]".into()),
            (next, "5]，並於2011年首次發布".into()),
        ]);
        assert!(
            !page.text.contains("rustc」[16] 5]"),
            "citation swallowed the next line: {:?}",
            page.text
        );
        assert!(
            !page.text.contains("[16]5]"),
            "citation glued onto next fragment: {:?}",
            page.text
        );
        let lines: Vec<&str> = page.text.lines().collect();
        assert!(
            lines
                .iter()
                .any(|l| l.contains("稱做「rustc」") && l.contains("[16]") && !l.contains("並於")),
            "expected [16] on the rustc line, got {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("並於2011年")),
            "expected the next paragraph on its own line, got {lines:?}"
        );
    }

    #[test]
    fn page_restores_dropped_cite_bracket() {
        let a = vec![(0, 0), (80, 0), (80, 16), (0, 16)];
        let page = page_from_blocks(vec![(a, "15] 仍在使用".into())]);
        assert!(
            page.text.contains("[15]"),
            "expected restored [, got {:?}",
            page.text
        );
    }

    #[test]
    fn page_for_translate_joins_wrapped_sentences() {
        let src = "Work shifted from the OCaml compiler\n(i.e., written in Rust) targeting LLVM.\n官方實現稱做「rustc」的編譯器。\n5]，並於2011年首次發布";
        let out = page_for_translate(src);
        assert!(
            out.contains("compiler (i.e., written in Rust) targeting LLVM."),
            "expected wrapped English sentence joined, got {out:?}"
        );
        assert!(
            out.contains("編譯器。\n"),
            "CJK sentence end must stay a paragraph break, got {out:?}"
        );
        assert!(
            !out.contains("compiler\n(i.e."),
            "must not translate line-by-line"
        );
    }
}

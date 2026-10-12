//! System-font rasterizer (fontdue). Used for annotation text, numbered markers,
//! clipboard cards, and host chrome when GDI is not available.

use crate::color::Rgba;
use crate::geom::Rect;
use crate::raster::Raster;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tiny_skia::{Pixmap, PremultipliedColorU8};

/// Toolbar chips (Python `QFontComboBox` is the full list; these cover the
/// families we can resolve from system files / fontconfig).
pub const FAMILY_PRESETS: &[(&str, &str)] = &[
    ("", "Default"),
    ("Sans", "Sans"),
    ("Serif", "Serif"),
    ("Mono", "Mono"),
];

fn candidates() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("PINSHOT_FONT") {
        out.push(std::path::PathBuf::from(p));
    }
    out.extend(
        [
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyh.ttf",
            "C:\\Windows\\Fonts\\segoeui.ttf",
            "C:\\Windows\\Fonts\\tahoma.ttf",
            "C:\\Windows\\Fonts\\arial.ttf",
        ]
        .iter()
        .map(std::path::PathBuf::from),
    );
    out
}

struct LoadedFont {
    font: fontdue::Font,
    path: PathBuf,
}

fn load_font(path: &Path) -> Option<fontdue::Font> {
    let bytes = std::fs::read(path).ok()?;
    let settings = fontdue::FontSettings {
        collection_index: 0,
        scale: 40.0,
        ..Default::default()
    };
    fontdue::Font::from_bytes(bytes, settings).ok()
}

fn loaded_default() -> Option<&'static LoadedFont> {
    static FONT: OnceLock<Option<LoadedFont>> = OnceLock::new();
    FONT.get_or_init(|| {
        for p in candidates() {
            if let Some(font) = load_font(&p) {
                return Some(LoadedFont { font, path: p });
            }
        }
        None
    })
    .as_ref()
}

fn font() -> Option<&'static fontdue::Font> {
    loaded_default().map(|l| &l.font)
}

/// Chrome UI faces: Noto/Source Han CJK + Liberation/Noto/DejaVu Latin.
/// Annotation default (`candidates()` / B/I/family) is unchanged.
fn chrome_cjk_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("PINSHOT_CHROME_FONT") {
        out.push(PathBuf::from(p));
    }
    out.extend(
        [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.otf",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansSC-Regular.otf",
            "/usr/share/fonts/opentype/source-han-sans-sc/SourceHanSansSC-Regular.otf",
            "/usr/share/fonts/adobe-source-han-sans/SourceHanSansCN-Regular.otf",
            "/usr/share/fonts/truetype/source-han-sans/SourceHanSansCN-Regular.otf",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            "/usr/share/fonts/truetype/droid/DroidSansFallback.ttf",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyh.ttf",
        ]
        .iter()
        .map(PathBuf::from),
    );
    out
}

fn chrome_latin_paths() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
        "C:\\Windows\\Fonts\\tahoma.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

fn looks_like_cjk_collection(path: &Path) -> bool {
    let n = path.to_string_lossy().to_ascii_lowercase();
    n.contains("notosanscjk") || n.contains("sourcehan") || n.contains("wqy")
}

fn load_font_index(path: &Path, index: u32) -> Option<fontdue::Font> {
    let bytes = std::fs::read(path).ok()?;
    let settings = fontdue::FontSettings {
        collection_index: index,
        scale: 40.0,
        ..Default::default()
    };
    fontdue::Font::from_bytes(bytes, settings).ok()
}

fn has_glyph(font: &fontdue::Font, ch: char) -> bool {
    ch == ' ' || ch == '\t' || font.lookup_glyph_index(ch) != 0
}

/// Prefer Simplified Chinese face (TTC index 2 on Noto/Source Han).
fn load_cjk_face(path: &Path) -> Option<fontdue::Font> {
    if !path.is_file() {
        return None;
    }
    let indices: &[u32] = if looks_like_cjk_collection(path) {
        &[2, 3, 0, 1, 4]
    } else {
        &[0]
    };
    for &idx in indices {
        if let Some(font) = load_font_index(path, idx) {
            if has_glyph(&font, '描') || has_glyph(&font, '中') {
                return Some(font);
            }
        }
    }
    None
}

struct ChromeStack {
    cjk: Option<&'static fontdue::Font>,
    latin: Option<&'static fontdue::Font>,
    cjk_path: Option<PathBuf>,
    latin_path: Option<PathBuf>,
}

fn leak_font(font: fontdue::Font) -> &'static fontdue::Font {
    &*Box::leak(Box::new(font))
}

fn chrome_stack() -> &'static ChromeStack {
    static STACK: OnceLock<ChromeStack> = OnceLock::new();
    STACK.get_or_init(|| {
        let mut cjk = None;
        let mut cjk_path = None;
        for p in chrome_cjk_paths() {
            if let Some(font) = load_cjk_face(&p) {
                cjk = Some(leak_font(font));
                cjk_path = Some(p);
                break;
            }
        }
        let mut latin = None;
        let mut latin_path = None;
        for p in chrome_latin_paths() {
            if p.is_file() {
                if let Some(font) = load_font(&p) {
                    latin = Some(leak_font(font));
                    latin_path = Some(p);
                    break;
                }
            }
        }
        ChromeStack {
            cjk,
            latin,
            cjk_path,
            latin_path,
        }
    })
}

fn pick_chrome(ch: char) -> Option<&'static fontdue::Font> {
    let s = chrome_stack();
    if ch.is_ascii() {
        if let Some(f) = s.latin {
            if has_glyph(f, ch) {
                return Some(f);
            }
        }
    }
    if let Some(f) = s.cjk {
        if has_glyph(f, ch) {
            return Some(f);
        }
    }
    if let Some(f) = s.latin {
        if has_glyph(f, ch) {
            return Some(f);
        }
    }
    font()
}

/// Resolved chrome faces (CJK, Latin). Used by tests / diagnostics.
pub fn chrome_font_paths() -> (Option<PathBuf>, Option<PathBuf>) {
    let s = chrome_stack();
    (s.cjk_path.clone(), s.latin_path.clone())
}

fn compact_name(path: &Path) -> String {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .replace([' ', '-', '_'], "")
}

fn is_outline_font(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("ttf" | "ttc" | "otf")
    )
}

fn file_stem_lower(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| name.to_ascii_lowercase())
}

fn file_looks_bold(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.contains("bold") || n.contains("heavy") || n.contains("black") || n.contains("semibold") {
        return true;
    }
    let stem = file_stem_lower(&n);
    // GDI short names: arialbd, timesbd, segoeuib, segoeuiz, consolab, consolaz
    stem.ends_with("bd")
        || stem.ends_with("bi")
        || stem.ends_with("segoeuib")
        || stem.ends_with("segoeuiz")
        || matches!(
            stem.as_str(),
            "consolab"
                | "consolaz"
                | "calibrib"
                | "calibriz"
                | "verdanab"
                | "verdanaz"
                | "tahomabd"
                | "msyhbd"
        )
}

fn file_looks_italic(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.contains("italic") || n.contains("oblique") || n.contains("it.") {
        return true;
    }
    let stem = file_stem_lower(&n);
    stem.ends_with("segoeuii")
        || stem.ends_with("segoeuiz")
        || stem.ends_with("timesi")
        || stem.ends_with("timesbi")
        || stem.ends_with("ariali")
        || stem.ends_with("arialbi")
        || matches!(
            stem.as_str(),
            "consolai" | "consolaz" | "calibrii" | "calibriz" | "verdanai" | "verdanaz"
        )
}

fn family_needles(family: &str) -> Vec<String> {
    match family.trim().to_ascii_lowercase().as_str() {
        "" | "sans" => [
            "liberationsans",
            "dejavusans",
            "notosans",
            "wqy-microhei",
            "wqymicrohei",
            "msyh",
            "segoeui",
            "arial",
            "freesans",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        "serif" => [
            "liberationserif",
            "dejavuserif",
            "notoserif",
            "times",
            "georgia",
            "nimbusroman",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        "mono" | "monospace" => [
            "liberationmono",
            "dejavusansmono",
            "notosansmono",
            "consola",
            "cour",
            "freemono",
            "nimbusmono",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        other => vec![other.replace([' ', '-'], "")],
    }
}

/// Sans must not match DejaVuSansMono; Serif must not match Sans; PFB is rejected.
fn matches_family(path: &Path, family: &str) -> bool {
    if !is_outline_font(path) {
        return false;
    }
    let c = compact_name(path);
    let fam = family.trim().to_ascii_lowercase();
    match fam.as_str() {
        "" | "sans" => {
            if c.contains("mono") || c.contains("serif") {
                return false;
            }
            family_needles("Sans").iter().any(|n| {
                if let Some(rest) = c.split(n.as_str()).nth(1) {
                    !rest.starts_with("mono")
                } else {
                    false
                }
            })
        }
        "serif" => {
            if c.contains("mono") {
                return false;
            }
            if c.contains("sans") && !c.contains("serif") {
                return false;
            }
            family_needles("Serif").iter().any(|n| c.contains(n))
        }
        "mono" | "monospace" => {
            family_needles("Mono").iter().any(|n| c.contains(n)) || c.contains("mono")
        }
        other => c.contains(&other.replace([' ', '-'], "")),
    }
}

fn style_rank(name: &str, bold: bool, italic: bool) -> i32 {
    let n = name.to_ascii_lowercase();
    let is_bold = file_looks_bold(&n);
    let is_italic = file_looks_italic(&n);
    let mut score = 0;
    if is_bold == bold {
        score += 3;
    } else {
        score -= 3;
    }
    if is_italic == italic {
        score += 3;
    } else {
        score -= 3;
    }
    if !bold && !italic && (n.contains("regular") || n.contains("medium")) {
        score += 1;
    }
    score
}

fn source_rank(path: &Path) -> i32 {
    let n = path.to_string_lossy().to_ascii_lowercase();
    if n.contains("liberation") {
        30
    } else if n.contains("dejavu") {
        20
    } else if n.contains("segoe") || n.contains("arial") || n.contains("msyh") {
        12
    } else if n.contains("noto") && is_outline_font(path) {
        8
    } else {
        0
    }
}

fn preferred_fonts(family: &str, bold: bool, italic: bool) -> Vec<PathBuf> {
    let kind = match family.trim().to_ascii_lowercase().as_str() {
        "" | "sans" => "Sans",
        "serif" => "Serif",
        "mono" | "monospace" => "Mono",
        _ => return Vec::new(),
    };
    let lib = match kind {
        "Sans" => "LiberationSans",
        "Serif" => "LiberationSerif",
        _ => "LiberationMono",
    };
    let lib_style = match (bold, italic) {
        (true, true) => "BoldItalic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    };
    let (djv_reg, djv_bold, djv_italic, djv_bi) = match kind {
        "Sans" => (
            "DejaVuSans.ttf",
            "DejaVuSans-Bold.ttf",
            "DejaVuSans-Oblique.ttf",
            "DejaVuSans-BoldOblique.ttf",
        ),
        "Serif" => (
            "DejaVuSerif.ttf",
            "DejaVuSerif-Bold.ttf",
            "DejaVuSerif-Italic.ttf",
            "DejaVuSerif-BoldItalic.ttf",
        ),
        _ => (
            "DejaVuSansMono.ttf",
            "DejaVuSansMono-Bold.ttf",
            "DejaVuSansMono-Oblique.ttf",
            "DejaVuSansMono-BoldOblique.ttf",
        ),
    };
    let djv = match (bold, italic) {
        (true, true) => djv_bi,
        (true, false) => djv_bold,
        (false, true) => djv_italic,
        (false, false) => djv_reg,
    };
    let (win_reg, win_bold, win_italic, win_bi) = match kind {
        "Sans" => (
            "segoeui.ttf",
            "segoeuib.ttf",
            "segoeuii.ttf",
            "segoeuiz.ttf",
        ),
        "Serif" => ("times.ttf", "timesbd.ttf", "timesi.ttf", "timesbi.ttf"),
        _ => (
            "consola.ttf",
            "consolab.ttf",
            "consolai.ttf",
            "consolaz.ttf",
        ),
    };
    let win = match (bold, italic) {
        (true, true) => win_bi,
        (true, false) => win_bold,
        (false, true) => win_italic,
        (false, false) => win_reg,
    };
    vec![
        PathBuf::from(format!(
            "/usr/share/fonts/truetype/liberation/{lib}-{lib_style}.ttf"
        )),
        PathBuf::from(format!("/usr/share/fonts/truetype/dejavu/{djv}")),
        PathBuf::from(format!("C:\\Windows\\Fonts\\{win}")),
    ]
}

fn font_files() -> &'static [PathBuf] {
    static FILES: OnceLock<Vec<PathBuf>> = OnceLock::new();
    FILES.get_or_init(|| {
        let mut out = candidates();
        let mut dirs = vec![
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
            PathBuf::from("C:\\Windows\\Fonts"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join(".local/share/fonts"));
        }
        for dir in dirs {
            walk_fonts(&dir, 0, 3, &mut out);
        }
        if let Ok(fc) = std::process::Command::new("fc-list")
            .args([":", "file"])
            .output()
        {
            if fc.status.success() {
                for line in String::from_utf8_lossy(&fc.stdout).lines() {
                    let path = line.split(':').next().unwrap_or("").trim();
                    if path.is_empty() {
                        continue;
                    }
                    let p = PathBuf::from(path);
                    if p.is_file() && is_outline_font(&p) {
                        out.push(p);
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    })
}

fn walk_fonts(dir: &Path, depth: u32, max: u32, out: &mut Vec<PathBuf>) {
    if depth > max {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            walk_fonts(&p, depth + 1, max, out);
        } else if matches!(
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("ttf" | "ttc" | "otf")
        ) {
            out.push(p);
        }
    }
}

fn sibling_variant(path: &Path, bold: bool, italic: bool) -> Option<PathBuf> {
    if !bold && !italic {
        return Some(path.to_path_buf());
    }
    let parent = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy();
    let stem = stem
        .trim_end_matches("-Regular")
        .trim_end_matches("Regular")
        .to_string();
    let suffixes: &[&str] = match (bold, italic) {
        (true, true) => &["-BoldItalic", "BoldItalic", "-BoldOblique", "bi"],
        (true, false) => &["-Bold", "Bold", "bd", "-Semibold"],
        (false, true) => &["-Italic", "Italic", "-Oblique", "Oblique"],
        (false, false) => return Some(path.to_path_buf()),
    };
    for ext in ["ttf", "ttc", "otf"] {
        for s in suffixes {
            let p = parent.join(format!("{stem}{s}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn find_family_file(family: &str, bold: bool, italic: bool) -> Option<PathBuf> {
    for p in preferred_fonts(family, bold, italic) {
        if p.is_file() && is_outline_font(&p) && load_font(&p).is_some() {
            return Some(p);
        }
    }
    if family.trim().is_empty() {
        if let Some(def) = loaded_default() {
            if let Some(sib) = sibling_variant(&def.path, bold, italic) {
                if sib.is_file() && is_outline_font(&sib) && load_font(&sib).is_some() {
                    return Some(sib);
                }
            }
            if !bold && !italic {
                return Some(def.path.clone());
            }
        }
    }
    let mut best: Option<(i32, PathBuf)> = None;
    for p in font_files() {
        if !matches_family(p, family) {
            continue;
        }
        if load_font(p).is_none() {
            continue;
        }
        let name = p.to_string_lossy().to_ascii_lowercase();
        let rank = style_rank(&name, bold, italic) * 10 + source_rank(p);
        if best.as_ref().map(|(r, _)| rank > *r).unwrap_or(true) {
            best = Some((rank, p.clone()));
        }
    }
    best.map(|(_, p)| p)
}

/// Resolved outline font for a toolbar family + B/I. `None` falls back to default.
pub fn resolve_family_path(family: &str, bold: bool, italic: bool) -> Option<PathBuf> {
    find_family_file(family, bold, italic)
}

fn font_ex(family: &str, bold: bool, italic: bool) -> Option<&'static fontdue::Font> {
    if family.is_empty() && !bold && !italic {
        return font();
    }
    static CACHE: OnceLock<Mutex<HashMap<(String, bool, bool), Option<&'static fontdue::Font>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (family.to_string(), bold, italic);
    if let Ok(g) = cache.lock() {
        if let Some(hit) = g.get(&key) {
            return *hit;
        }
    }
    let found = find_family_file(family, bold, italic).and_then(|p| load_font(&p));
    let leaked = match found {
        Some(f) => Some(&*Box::leak(Box::new(f))),
        None => font(),
    };
    if let Ok(mut g) = cache.lock() {
        g.insert(key, leaked);
    }
    leaked
}

/// Families we can offer in the toolbar (presets that resolve on this machine).
pub fn available_families() -> Vec<&'static str> {
    FAMILY_PRESETS.iter().map(|(id, _)| *id).collect()
}

pub fn available() -> bool {
    font().is_some()
}

fn measure_picked<'a>(
    text: &str,
    px: f32,
    mut pick: impl FnMut(char) -> Option<&'a fontdue::Font>,
) -> (f32, f32) {
    let fs = px.max(8.0);
    let line_h = fs * 1.25;
    let mut max_w = 0.0_f32;
    let mut h = 0.0_f32;
    let mut any = false;
    for line in text.lines() {
        let mut w = 0.0_f32;
        for ch in line.chars() {
            if let Some(font) = pick(ch) {
                any = true;
                w += font.metrics(ch, fs).advance_width;
            } else {
                w += fs * 0.6;
            }
        }
        max_w = max_w.max(w);
        h += line_h;
    }
    if text.is_empty() {
        h = line_h;
    }
    if !any && font().is_none() {
        let w = text
            .lines()
            .map(|l| l.chars().count() as f32 * fs * 0.6)
            .fold(0.0_f32, f32::max);
        return (w, fs * 1.25 * text.lines().count().max(1) as f32);
    }
    (max_w, h.max(fs))
}

pub fn measure(text: &str, px: f32) -> (f32, f32) {
    measure_picked(text, px, |_| font())
}

/// Layout size for chrome labels (toolbar chips, menus, toasts, prefs).
pub fn measure_chrome(text: &str, px: f32) -> (f32, f32) {
    measure_picked(text, px, pick_chrome)
}

/// Straight-alpha source-over. Destination may be transparent (temp glyph
/// buffers) or already opaque (cards painted in-place).
fn blend(dst: &mut [u8], i: usize, r: u8, g: u8, b: u8, a: u8) {
    if i + 3 >= dst.len() || a == 0 {
        return;
    }
    let src_a = a as u32;
    let dst_a = dst[i + 3] as u32;
    let out_a = src_a + dst_a * (255 - src_a) / 255;
    if out_a == 0 {
        return;
    }
    dst[i] = ((r as u32 * src_a + dst[i] as u32 * dst_a * (255 - src_a) / 255) / out_a) as u8;
    dst[i + 1] =
        ((g as u32 * src_a + dst[i + 1] as u32 * dst_a * (255 - src_a) / 255) / out_a) as u8;
    dst[i + 2] =
        ((b as u32 * src_a + dst[i + 2] as u32 * dst_a * (255 - src_a) / 255) / out_a) as u8;
    dst[i + 3] = out_a as u8;
}

pub fn blit_rgba(
    pixels: &mut [u8],
    stride_px: u32,
    dest_w: u32,
    dest_h: u32,
    text: &str,
    x: f32,
    y: f32,
    px: f32,
    color: Rgba,
    outline: bool,
) {
    let Some(font) = font() else {
        return;
    };
    blit_rgba_font(
        font, pixels, stride_px, dest_w, dest_h, text, x, y, px, color, outline, false, false,
    );
}

fn blit_rgba_picked<'a>(
    mut pick: impl FnMut(char) -> Option<&'a fontdue::Font>,
    pixels: &mut [u8],
    stride_px: u32,
    dest_w: u32,
    dest_h: u32,
    text: &str,
    x: f32,
    y: f32,
    px: f32,
    color: Rgba,
    outline: bool,
    fake_bold: bool,
    fake_italic: bool,
) {
    let px = px.max(8.0);
    let line_h = px * 1.25;
    let shear = if fake_italic { 0.28 } else { 0.0 };
    for (li, line) in text.lines().enumerate() {
        let mut pen_x = x;
        let baseline = y + px + li as f32 * line_h;
        for ch in line.chars() {
            let Some(font) = pick(ch) else {
                pen_x += px * 0.6;
                continue;
            };
            let (metrics, bitmap) = font.rasterize(ch, px);
            let gx = pen_x + metrics.xmin as f32;
            let gy = baseline - metrics.height as f32 - metrics.ymin as f32;
            for by in 0..metrics.height {
                for bx in 0..metrics.width {
                    let cov = bitmap[by * metrics.width + bx];
                    if cov == 0 {
                        continue;
                    }
                    let dy = gy.round() as i32 + by as i32;
                    let slant = ((baseline - (gy + by as f32)) * shear).round() as i32;
                    let dx = gx.round() as i32 + bx as i32 + slant;
                    if dx < 0 || dy < 0 || dx >= dest_w as i32 || dy >= dest_h as i32 {
                        continue;
                    }
                    let di = (dy as u32 * stride_px + dx as u32) as usize * 4;
                    let a = ((cov as u16 * color.a as u16) / 255) as u8;
                    if outline {
                        for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                            let sx = dx + ox;
                            let sy = dy + oy;
                            if sx < 0 || sy < 0 || sx >= dest_w as i32 || sy >= dest_h as i32 {
                                continue;
                            }
                            let si = (sy as u32 * stride_px + sx as u32) as usize * 4;
                            blend(pixels, si, 255, 255, 255, (a as u16 * 180 / 255) as u8);
                        }
                    }
                    blend(pixels, di, color.r, color.g, color.b, a);
                    if fake_bold {
                        let dx2 = dx + 1;
                        if dx2 >= 0 && dx2 < dest_w as i32 {
                            let di2 = (dy as u32 * stride_px + dx2 as u32) as usize * 4;
                            blend(pixels, di2, color.r, color.g, color.b, a);
                        }
                    }
                }
            }
            pen_x += metrics.advance_width;
        }
    }
}

fn blit_rgba_font(
    font: &fontdue::Font,
    pixels: &mut [u8],
    stride_px: u32,
    dest_w: u32,
    dest_h: u32,
    text: &str,
    x: f32,
    y: f32,
    px: f32,
    color: Rgba,
    outline: bool,
    fake_bold: bool,
    fake_italic: bool,
) {
    blit_rgba_picked(
        |_| Some(font),
        pixels,
        stride_px,
        dest_w,
        dest_h,
        text,
        x,
        y,
        px,
        color,
        outline,
        fake_bold,
        fake_italic,
    );
}

/// Chrome labels: CJK + Latin pairing, coverage kept (no crunchy cutoff).
pub fn blit_rgba_chrome(
    pixels: &mut [u8],
    stride_px: u32,
    dest_w: u32,
    dest_h: u32,
    text: &str,
    x: f32,
    y: f32,
    px: f32,
    color: Rgba,
) {
    blit_rgba_picked(
        pick_chrome,
        pixels,
        stride_px,
        dest_w,
        dest_h,
        text,
        x,
        y,
        px,
        color,
        false,
        false,
        false,
    );
}

pub fn blit_pixmap(
    pm: &mut Pixmap,
    text: &str,
    x: f32,
    y: f32,
    px: f32,
    color: Rgba,
    outline: bool,
) {
    blit_pixmap_ex(pm, text, x, y, px, color, outline, "", false, false);
}

pub fn blit_pixmap_ex(
    pm: &mut Pixmap,
    text: &str,
    x: f32,
    y: f32,
    px: f32,
    color: Rgba,
    outline: bool,
    family: &str,
    bold: bool,
    italic: bool,
) {
    if text.is_empty() {
        return;
    }
    let path = find_family_file(family, bold, italic);
    let Some(font) = font_ex(family, bold, italic).or_else(font) else {
        return;
    };
    let name = path
        .as_ref()
        .map(|p| p.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let fake_bold = bold && !file_looks_bold(&name);
    let fake_italic = italic && !file_looks_italic(&name);
    let (tw, th) = measure(text, px);
    let extra = if fake_italic {
        (px * 0.4) as u32 + 8
    } else {
        8
    };
    let tw = (tw.ceil() as u32 + extra).max(1);
    let th = (th.ceil() as u32 + 8).max(1);
    let mut rgba = vec![0u8; tw as usize * th as usize * 4];
    blit_rgba_font(
        font,
        &mut rgba,
        tw,
        tw,
        th,
        text,
        2.0,
        2.0,
        px,
        color,
        outline,
        fake_bold,
        fake_italic,
    );
    let Some(mut src) = Pixmap::new(tw, th) else {
        return;
    };
    for (dst, s) in src.pixels_mut().iter_mut().zip(rgba.chunks_exact(4)) {
        let a = s[3];
        *dst = PremultipliedColorU8::from_rgba(
            ((s[0] as u16 * a as u16) / 255) as u8,
            ((s[1] as u16 * a as u16) / 255) as u8,
            ((s[2] as u16 * a as u16) / 255) as u8,
            a,
        )
        .unwrap_or(PremultipliedColorU8::TRANSPARENT);
    }
    pm.draw_pixmap(
        x as i32 - 2,
        y as i32 - 2,
        src.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
}

pub fn blit_raster(
    img: &mut Raster,
    text: &str,
    x: f32,
    y: f32,
    px: f32,
    color: Rgba,
    outline: bool,
) {
    blit_rgba(
        &mut img.pixels,
        img.width,
        img.width,
        img.height,
        text,
        x,
        y,
        px,
        color,
        outline,
    );
}

/// Text / colour cards matching `pinshot/cards.py`.
pub fn color_card(c: Rgba) -> Raster {
    let w = 300u32;
    let sw = 96u32;
    let lh = 22u32;
    let rows = color_lines(c);
    let h = (sw + 20).max(16 + lh * rows.len() as u32);
    let mut img = Raster::new(w, h);
    img.fill(Rgba::rgb(255, 255, 255));
    for y in 10..h.saturating_sub(10) {
        for x in 10..10 + sw {
            img.set(x as i32, y as i32, c);
        }
    }
    for (i, (k, v)) in rows.iter().enumerate() {
        let y = 10.0 + i as f32 * lh as f32;
        blit_raster(
            &mut img,
            k,
            (sw + 22) as f32,
            y,
            12.0,
            Rgba::rgb(85, 85, 85),
            false,
        );
        blit_raster(
            &mut img,
            v,
            (sw + 92) as f32,
            y,
            12.0,
            Rgba::rgb(17, 17, 17),
            false,
        );
    }
    let _ = Rect::EMPTY;
    img
}

pub fn text_card(text: &str, max_w: u32) -> Raster {
    let text = text.replace('\t', "    ");
    let text: String = text.chars().take(20_000).collect();
    let px = 14.0;
    let (tw, th) = measure(&text, px);
    let w = ((tw + 24.0).min(max_w as f32)).max(80.0) as u32;
    let h = ((th + 20.0).min(1200.0)).max(36.0) as u32;
    let mut img = Raster::new(w, h);
    img.fill(Rgba::rgb(255, 254, 245));
    blit_raster(&mut img, &text, 12.0, 8.0, px, Rgba::rgb(34, 34, 34), false);
    img
}

fn color_lines(c: Rgba) -> Vec<(String, String)> {
    let rf = c.r as f32 / 255.0;
    let gf = c.g as f32 / 255.0;
    let bf = c.b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let v = max;
    let s = if max == 0.0 { 0.0 } else { (max - min) / max };
    let h = if max == min {
        0.0
    } else if max == rf {
        60.0 * (((gf - bf) / (max - min)) % 6.0)
    } else if max == gf {
        60.0 * ((bf - rf) / (max - min) + 2.0)
    } else {
        60.0 * ((rf - gf) / (max - min) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let l = (max + min) / 2.0;
    let sl = if l == 0.0 || l == 1.0 {
        0.0
    } else {
        (max - min) / (1.0 - (2.0 * l - 1.0).abs())
    };
    let i = (rf + gf + bf) / 3.0;
    let si = if i == 0.0 { 0.0 } else { 1.0 - min / i };
    vec![
        ("RGB".into(), format!("{}, {}, {}", c.r, c.g, c.b)),
        ("RGB float".into(), format!("{rf:.3}, {gf:.3}, {bf:.3}")),
        (
            "HSV".into(),
            format!("{:.0}°, {:.0}%, {:.0}%", h, s * 100.0, v * 100.0),
        ),
        (
            "HSL".into(),
            format!("{:.0}°, {:.0}%, {:.0}%", h, sl * 100.0, l * 100.0),
        ),
        (
            "HSI".into(),
            format!("{:.0}°, {:.0}%, {:.0}%", h, si * 100.0, i * 100.0),
        ),
        ("HEX".into(), format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_non_empty() {
        let (w, h) = measure("Hi", 20.0);
        assert!(w > 4.0);
        assert!(h >= 20.0);
    }

    #[test]
    fn cards_have_pixels() {
        let c = color_card(Rgba::RED);
        assert_eq!(c.width, 300);
        assert!(c.pixels.iter().any(|&p| p > 0));
        let t = text_card("hello pinshot", 320);
        assert!(t.width >= 80);
        assert!(t.height >= 36);
    }

    #[test]
    fn blit_pixmap_ex_paints_and_presets_exist() {
        assert!(FAMILY_PRESETS.iter().any(|(id, _)| id.is_empty()));
        assert!(available_families().contains(&"Sans"));
        let mut pm = Pixmap::new(80, 40).unwrap();
        blit_pixmap_ex(
            &mut pm,
            "Hi",
            2.0,
            2.0,
            16.0,
            Rgba::RED,
            false,
            "Sans",
            true,
            false,
        );
        assert!(pm.pixels().iter().any(|p| p.alpha() > 0));
    }

    fn paint_text(family: &str, bold: bool, italic: bool) -> Vec<u8> {
        let mut pm = Pixmap::new(160, 56).unwrap();
        blit_pixmap_ex(
            &mut pm,
            "HiBg",
            8.0,
            8.0,
            28.0,
            Rgba::RED,
            false,
            family,
            bold,
            italic,
        );
        pm.data().to_vec()
    }

    fn ext_ok(p: &Path) -> bool {
        is_outline_font(p)
    }

    #[test]
    fn resolve_prefers_loadable_ttf_not_mono_for_sans() {
        let sans = resolve_family_path("Sans", false, false).expect("sans");
        let serif = resolve_family_path("Serif", false, false).expect("serif");
        let mono = resolve_family_path("Mono", false, false).expect("mono");
        for p in [&sans, &serif, &mono] {
            assert!(ext_ok(p), "not outline: {}", p.display());
            let n = p.to_string_lossy().to_ascii_lowercase();
            assert!(!n.ends_with(".pfb") && !n.ends_with(".pfa") && !n.ends_with(".t1"));
        }
        let sc = compact_name(&sans);
        assert!(
            !sc.contains("mono"),
            "Sans must not be Mono: {}",
            sans.display()
        );
        assert_ne!(compact_name(&sans), compact_name(&mono));
        assert_ne!(compact_name(&serif), compact_name(&sans));
        let sl = sans.to_string_lossy().to_ascii_lowercase();
        assert!(
            sl.contains("liberation")
                || sl.contains("dejavu")
                || sl.contains("segoe")
                || sl.contains("arial")
                || sl.contains("msyh"),
            "prefer Liberation/DejaVu (or Windows segoe/arial), got {}",
            sans.display()
        );
        assert!(file_looks_bold(r"C:\Windows\Fonts\segoeuib.ttf"));
        assert!(file_looks_italic(r"C:\Windows\Fonts\segoeuii.ttf"));
        assert!(!file_looks_bold(r"C:\Windows\Fonts\segoeui.ttf"));
    }

    #[test]
    fn bold_and_italic_change_paint() {
        let regular = paint_text("Sans", false, false);
        let bold = paint_text("Sans", true, false);
        let italic = paint_text("Sans", false, true);
        let serif = paint_text("Serif", false, false);
        let mono = paint_text("Mono", false, false);
        assert_ne!(regular, bold, "Bold must change pixels");
        assert_ne!(regular, italic, "Italic must change pixels");
        assert_ne!(regular, serif, "Serif must differ from Sans");
        assert_ne!(regular, mono, "Mono must differ from Sans");
        let bold_path = resolve_family_path("Sans", true, false).unwrap();
        assert!(
            file_looks_bold(&bold_path.to_string_lossy()),
            "bold resolve {}",
            bold_path.display()
        );
        let def = paint_text("", false, false);
        let def_bold = paint_text("", true, false);
        let def_italic = paint_text("", false, true);
        assert_ne!(def, def_bold, "Default Bold must change pixels");
        assert_ne!(def, def_italic, "Default Italic must change pixels");
    }

    fn count_alpha_bins(pixels: &[u8]) -> (u32, u32, u32) {
        let mut empty = 0u32;
        let mut mid = 0u32;
        let mut solid = 0u32;
        for px in pixels.chunks_exact(4) {
            match px[3] {
                0 => empty += 1,
                1..=239 => mid += 1,
                _ => solid += 1,
            }
        }
        (empty, mid, solid)
    }

    #[test]
    fn chrome_cjk_keeps_coverage_fringes() {
        let (cjk, latin) = chrome_font_paths();
        assert!(
            cjk.is_some() || latin.is_some() || font().is_some(),
            "need a chrome or default face"
        );
        if let Some(p) = &cjk {
            let n = p.to_string_lossy().to_ascii_lowercase();
            assert!(
                n.contains("notosanscjk")
                    || n.contains("sourcehan")
                    || n.contains("droid")
                    || n.contains("wqy")
                    || n.contains("msyh"),
                "chrome CJK should be Noto/Source Han/Droid/WenQuanYi/msyh, got {}",
                p.display()
            );
        }
        let mut rgba = vec![0u8; 220 * 56 * 4];
        blit_rgba_chrome(
            &mut rgba,
            220,
            220,
            56,
            "描边 默认 等宽",
            6.0,
            8.0,
            12.0,
            Rgba::rgb(32, 32, 36),
        );
        let (_empty, mid, solid) = count_alpha_bins(&rgba);
        assert!(
            mid + solid > 80,
            "CJK ink, mid={mid} solid={solid} cjk={cjk:?}"
        );
        assert!(
            mid > 80 && mid > solid,
            "chrome CJK must keep AA coverage (not opaque-only blit), mid={mid} solid={solid} cjk={cjk:?}"
        );
    }

    #[test]
    fn chrome_latin_has_aa_and_pairs_with_cjk() {
        let mut rgba = vec![0u8; 120 * 48 * 4];
        blit_rgba_chrome(
            &mut rgba,
            120,
            120,
            48,
            "BI",
            8.0,
            8.0,
            16.0,
            Rgba::rgb(32, 32, 36),
        );
        let (_empty, mid, solid) = count_alpha_bins(&rgba);
        assert!(solid > 8, "B/I ink, solid={solid}");
        assert!(mid > 8, "B/I glyphs need AA fringes, mid={mid}");
        let (w_cjk, _) = measure_chrome("描边", 12.0);
        let (w_latin, _) = measure_chrome("BI", 12.0);
        assert!(w_cjk > 16.0 && w_latin > 8.0);
    }

    #[test]
    fn blend_on_opaque_still_covers() {
        let mut img = Raster::new(80, 40);
        img.fill(Rgba::WHITE);
        blit_raster(&mut img, "Hi", 4.0, 6.0, 18.0, Rgba::rgb(20, 20, 20), false);
        assert!(img.pixels.iter().any(|&p| p > 0 && p < 255));
    }
}

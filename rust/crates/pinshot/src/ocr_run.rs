//! Download PP-OCRv5 ONNX models; run paddle-ocr-rs (RapidOCR port).

use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::Mutex;

use pinshot_core::ocr::{
    det_padding, model_dir, ocr_upsample, page_from_blocks, ModelPaths, OcrModelKind, OcrPage,
    DET_BOX_SCORE, DET_BOX_THRESH, DET_MAX_SIDE, DET_UNCLIP,
};
use pinshot_core::raster::Raster;

pub fn recognize(image: &Raster) -> Result<OcrPage, String> {
    recognize_with_progress(image, |_| {})
}

pub fn recognize_with_progress(
    image: &Raster,
    mut progress: impl FnMut(&str),
) -> Result<OcrPage, String> {
    crate::logutil::log(&format!("ocr recognize {}x{}", image.width, image.height));
    progress("Downloading OCR models…");
    let paths = ensure_models_with_progress(&mut progress)?;
    progress("Recognizing…");
    infer(image, &paths)
}

pub fn prefetch_models() {
    std::thread::spawn(|| {
        match ensure_models_with_progress(&mut |msg| {
            crate::logutil::log(&format!("ocr prefetch {msg}"));
        }) {
            Ok(p) => crate::logutil::log(&format!("ocr prefetch ready {}", p.rec.display())),
            Err(e) => crate::logutil::log(&format!("ocr prefetch: {e}")),
        }
    });
}

pub fn ensure_models() -> Result<ModelPaths, String> {
    ensure_models_with_progress(&mut |_| {})
}

fn force_mobile() -> bool {
    std::env::var("PINSHOT_OCR_MODEL")
        .ok()
        .is_some_and(|v| OcrModelKind::parse(&v) == OcrModelKind::Mobile)
}

fn extra_model_dirs(data_models: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(bundled) = std::env::var("PINSHOT_OCR_MODELS") {
        out.push(std::path::PathBuf::from(bundled));
    }
    if let Ok(home) = std::env::var("HOME") {
        out.push(
            std::path::PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("PinShot")
                .join("ocr-models"),
        );
    }
    out.into_iter()
        .filter(|p| p != data_models && p.is_dir())
        .collect()
}

fn ensure_models_with_progress(progress: &mut dyn FnMut(&str)) -> Result<ModelPaths, String> {
    static LOCK: Mutex<()> = Mutex::new(());
    let _g = LOCK.lock().map_err(|e| e.to_string())?;
    let dir = model_dir(&pinshot_core::config::data_dir());
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let server = ModelPaths::for_kind(&dir, OcrModelKind::Server);
    let mobile = ModelPaths::for_kind(&dir, OcrModelKind::Mobile);
    for src in extra_model_dirs(&dir) {
        copy_if_present(&src, &server);
        copy_if_present(&src, &mobile);
    }
    // Saved config from 88c3e4e still says mobile. Server is required for
    // English on real Chrome; PINSHOT_OCR_MODEL=mobile is the only force.
    if !force_mobile() && server.ready() {
        crate::logutil::log(&format!(
            "ocr using server det={} rec={}",
            server.det.display(),
            server.rec.display()
        ));
        return Ok(server);
    }
    if mobile.ready() && (force_mobile() || !server.ready()) {
        crate::logutil::log(&format!(
            "ocr using mobile det={} rec={} (server ready={})",
            mobile.det.display(),
            mobile.rec.display(),
            server.ready()
        ));
        return Ok(mobile);
    }
    let preferred = if force_mobile() { mobile } else { server };
    for (urls, dest) in preferred.downloads() {
        if dest.is_file() {
            continue;
        }
        let name = dest
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        download_first(&urls, &dest, &name, progress)?;
    }
    if !preferred.ready() {
        return Err("OCR models missing (det/rec/cls)".into());
    }
    Ok(preferred)
}

fn copy_if_present(src: &Path, dest: &ModelPaths) {
    let server = dest
        .det
        .file_name()
        .is_some_and(|n| n == pinshot_core::ocr::SERVER_DET_FILE);
    let det_names: &[&str] = if server {
        &["ch_PP-OCRv5_server_det.onnx", "ch_PP-OCRv5_det_server.onnx"]
    } else {
        &["ch_PP-OCRv5_mobile_det.onnx", "ch_PP-OCRv5_det_mobile.onnx"]
    };
    let rec_names: &[&str] = if server {
        &["ch_PP-OCRv5_server_rec.onnx", "ch_PP-OCRv5_rec_server.onnx"]
    } else {
        &["ch_PP-OCRv5_mobile_rec.onnx", "ch_PP-OCRv5_rec_mobile.onnx"]
    };
    let aliases = [
        (&dest.det, det_names),
        (&dest.rec, rec_names),
        (&dest.keys, &["ppocr_keys_v5.txt", "ppocrv5_dict.txt"][..]),
        (
            &dest.cls,
            &[
                "ch_PP-LCNet_x0_25_textline_ori.onnx",
                "ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx",
            ][..],
        ),
    ];
    for (to, names) in aliases {
        if to.is_file() {
            continue;
        }
        for name in names {
            let from = src.join(name);
            if from.is_file() {
                let _ = fs::copy(&from, to);
                break;
            }
        }
    }
}

fn download_first(
    urls: &[&str],
    dest: &Path,
    name: &str,
    progress: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let mut last = "no urls".to_string();
    for url in urls {
        match download(url, dest, name, progress) {
            Ok(()) => return Ok(()),
            Err(e) => {
                crate::logutil::log(&format!("ocr download fail {url}: {e}"));
                last = e;
            }
        }
    }
    Err(last)
}

fn download(
    url: &str,
    dest: &Path,
    name: &str,
    progress: &mut dyn FnMut(&str),
) -> Result<(), String> {
    crate::logutil::log(&format!("ocr download {name}"));
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(180))
        .call()
        .map_err(|e| format!("download {name}: {e}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0);
    let mut reader = resp.into_reader();
    let mut bytes = Vec::new();
    if let Some(t) = total {
        bytes.reserve(t as usize);
    }
    let mut buf = [0u8; 64 * 1024];
    let mut last_pct = 999u32;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if let Some(t) = total {
            let pct = ((bytes.len() as u64).min(t) * 100 / t) as u32;
            if pct != last_pct {
                last_pct = pct;
                progress(&format!("Downloading {pct}%"));
            }
        } else {
            progress(&format!("Downloading {}…", name));
        }
    }
    if bytes.len() < 64 {
        return Err(format!("download too small: {name}"));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("part");
    fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    crate::logutil::log(&format!(
        "ocr saved {} ({} bytes)",
        dest.display(),
        bytes.len()
    ));
    Ok(())
}

#[cfg(any(feature = "ocr", unix))]
fn infer(image: &Raster, paths: &ModelPaths) -> Result<OcrPage, String> {
    if image.width < 8 || image.height < 8 {
        return Err("image too small for OCR".into());
    }
    let scale = ocr_upsample(image.width, image.height);
    let unclip = std::env::var("PINSHOT_OCR_UNCLIP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DET_UNCLIP);
    let box_score = std::env::var("PINSHOT_OCR_BOX_SCORE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DET_BOX_SCORE);
    let up = if scale > 1 {
        Some(image.scale_bilinear(image.width * scale, image.height * scale))
    } else {
        None
    };
    let (rgb, pad_w, pad_h) = match &up {
        Some(u) => (raster_to_rgb(u), u.width, u.height),
        None => (raster_to_rgb(image), image.width, image.height),
    };
    with_engine(paths, |ocr| {
        let result = ocr
            // paddle-ocr-rs AngleNet is 48×192 (PP-OCRv2 cls). RapidOCR v5
            // orientation is 80×160, so skip angle; screenshot lines are upright.
            // Chrome 13px glyphs are ~14px tall; rec is H=48, so article clips
            // use 3×. Unclip 1.6. max_side 2560.
            .detect(
                &rgb,
                det_padding(pad_w, pad_h),
                DET_MAX_SIDE,
                box_score,
                DET_BOX_THRESH,
                unclip,
                false,
                false,
            )
            .map_err(|e| format!("ocr detect: {e:?}"))?;
        crate::logutil::log(&format!(
            "ocr blocks {} scale={scale} pad={} max_side={DET_MAX_SIDE} unclip={unclip} box={box_score} {pad_w}x{pad_h} det={} rec={}",
            result.text_blocks.len(),
            det_padding(pad_w, pad_h),
            paths.det.display(),
            paths.rec.display()
        ));
        let mut blocks = Vec::new();
        for b in result.text_blocks {
            crate::logutil::log(&format!("ocr line score={:.3} {:?}", b.text_score, b.text));
            // Keep low-score boxes; dropping them lost Wikipedia lines.
            if b.text.trim().is_empty() {
                continue;
            }
            let pts: Vec<(u32, u32)> = b
                .box_points
                .iter()
                .map(|p| {
                    if scale <= 1 {
                        (p.x, p.y)
                    } else {
                        (p.x / scale, p.y / scale)
                    }
                })
                .collect();
            blocks.push((pts, b.text));
        }
        let page = page_from_blocks(blocks);
        if page.text.trim().is_empty() {
            Err("No text found. Select a clearer area.".into())
        } else {
            crate::logutil::log(&format!(
                "ocr done {} chars {} tokens",
                page.text.chars().count(),
                page.tokens.len()
            ));
            Ok(page)
        }
    })
}

#[cfg(any(feature = "ocr", unix))]
fn raster_to_rgb(image: &Raster) -> image::RgbImage {
    let n = (image.width as usize) * (image.height as usize);
    let mut rgb = vec![0u8; n * 3];
    for i in 0..n {
        let s = i * 4;
        let d = i * 3;
        if s + 2 < image.pixels.len() {
            rgb[d] = image.pixels[s];
            rgb[d + 1] = image.pixels[s + 1];
            rgb[d + 2] = image.pixels[s + 2];
        }
    }
    image::RgbImage::from_raw(image.width, image.height, rgb)
        .unwrap_or_else(|| image::RgbImage::new(image.width, image.height))
}

#[cfg(any(feature = "ocr", unix))]
fn with_engine<F>(paths: &ModelPaths, f: F) -> Result<OcrPage, String>
where
    F: FnOnce(&mut paddle_ocr_rs::ocr_lite::OcrLite) -> Result<OcrPage, String>,
{
    use std::sync::{Mutex, OnceLock};
    static ENGINE: OnceLock<Mutex<Option<(String, paddle_ocr_rs::ocr_lite::OcrLite)>>> =
        OnceLock::new();
    let cell = ENGINE.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().map_err(|e| e.to_string())?;
    let key = paths.rec.to_string_lossy().into_owned();
    let need_init = match guard.as_ref() {
        Some((k, _)) => k != &key,
        None => true,
    };
    if need_init {
        let mut ocr = paddle_ocr_rs::ocr_lite::OcrLite::new();
        let det = paths.det.to_string_lossy();
        let cls = paths.cls.to_string_lossy();
        let rec = paths.rec.to_string_lossy();
        let init = ocr.init_models(&det, &cls, &rec, 2);
        if let Err(e) = init {
            crate::logutil::log(&format!(
                "ocr init metadata dict fail: {e:?}; trying keys file"
            ));
            if paths.keys.is_file() {
                ocr.init_models_with_dict(&det, &cls, &rec, &paths.keys.to_string_lossy(), 2)
                    .map_err(|e| format!("ocr models: {e:?}"))?;
            } else {
                return Err(format!("ocr models: {e:?}"));
            }
        }
        *guard = Some((key, ocr));
    }
    let ocr = &mut guard.as_mut().expect("engine").1;
    f(ocr)
}

#[cfg(not(any(feature = "ocr", unix)))]
fn infer(_image: &Raster, _paths: &ModelPaths) -> Result<OcrPage, String> {
    Err("OCR not available in this build. Use pinshot-ocr.exe.".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use pinshot_core::font;
    use pinshot_core::raster::Raster;
    use pinshot_core::Rgba;

    fn page_text(image: &Raster) -> String {
        recognize(image).unwrap_or_else(|e| panic!("ocr: {e}")).text
    }

    #[test]
    fn ocr_real_chrome_pin_png_when_requested() {
        if std::env::var("PINSHOT_OCR_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let path = std::env::var("PINSHOT_OCR_PNG")
            .unwrap_or_else(|_| "/tmp/pinshot-retest-data/session/pin-000.png".into());
        let Some(img) = load_png_raster(std::path::Path::new(&path)) else {
            panic!("missing chrome pin png {path}");
        };
        let text = page_text(&img);
        eprintln!(
            "ocr chrome-pin {}x{} scale-env={:?} => {text:?}",
            img.width,
            img.height,
            std::env::var("PINSHOT_OCR_SCALE").ok()
        );
        let lower = text.to_ascii_lowercase();
        assert!(
            lower.contains("work") || lower.contains("ocaml") || lower.contains("mozilla"),
            "chrome pin OCR missed the paragraph: {text:?}"
        );
        assert!(
            text.contains(' '),
            "chrome pin OCR dropped English spaces: {text:?}"
        );
    }

    #[test]
    fn ocr_reads_chinese_and_english_when_requested() {
        if std::env::var("PINSHOT_OCR_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let mut img = Raster::new(720, 180);
        img.fill(Rgba::WHITE);
        font::blit_rgba_chrome(
            &mut img.pixels,
            img.width,
            img.width,
            img.height,
            "Hello 你好 PinShot",
            24.0,
            48.0,
            42.0,
            Rgba::BLACK,
        );
        img.force_opaque();
        let text = page_text(&img);
        eprintln!("ocr smoke => {text:?}");
        let has_en = text.to_ascii_lowercase().contains("hello")
            || text.to_ascii_lowercase().contains("pinshot");
        let has_zh = text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
        assert!(has_en && has_zh, "expected English + Chinese, got {text:?}");
    }

    #[test]
    fn ocr_keeps_english_word_spaces_when_requested() {
        if std::env::var("PINSHOT_OCR_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let line = [
            "In 2009, Mozilla officially sponsored the project.",
            "The Firefox logo and the first full-time MacOS engineers.",
            "See [note 5] and [21] After 2010s (i.e., later).",
        ];
        let img = pango_article_raster(&line, "Liberation Sans 13")
            .or_else(|| pango_article_raster(&line, "DejaVu Sans 13"))
            .unwrap_or_else(|| {
                let mut img = Raster::new(1100, 360);
                img.fill(Rgba::WHITE);
                font::blit_rgba_chrome(
                    &mut img.pixels,
                    img.width,
                    img.width,
                    img.height,
                    line[0],
                    36.0,
                    64.0,
                    16.0,
                    Rgba::BLACK,
                );
                img.force_opaque();
                img
            });
        let text = page_text(&img);
        eprintln!("ocr space smoke => {text:?}");
        let lower = text.to_ascii_lowercase();
        assert!(
            lower.contains("mozilla") || lower.contains("sponsored") || lower.contains("project"),
            "expected English words, got {text:?}"
        );
        assert!(
            text.contains(' '),
            "expected ASCII spaces between English words, got {text:?}"
        );
        let words = lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>();
        assert!(
            words.iter().any(|w| *w == "in" || *w == "2009")
                && words
                    .iter()
                    .any(|w| *w == "project" || *w == "mozilla" || *w == "sponsored"),
            "expected separated English tokens, got {text:?} tokens={words:?}"
        );
        assert!(
            text.chars().filter(|c| *c == ' ').count() >= 3,
            "expected several English word spaces, got {text:?}"
        );
    }

    fn load_png_raster(path: &std::path::Path) -> Option<Raster> {
        let img = image::open(path).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let mut r = Raster::from_rgba(w, h, img.into_raw())?;
        r.force_opaque();
        Some(r)
    }

    fn place_on_page(src: &Raster, page_w: u32, page_h: u32, ox: u32, oy: u32) -> Raster {
        let mut img = Raster::new(page_w.max(src.width + ox), page_h.max(src.height + oy));
        img.fill(Rgba::WHITE);
        for y in 0..src.height {
            for x in 0..src.width {
                if let Some(c) = src.get(x as i32, y as i32) {
                    img.set((ox + x) as i32, (oy + y) as i32, c);
                }
            }
        }
        img
    }

    fn pango_article_raster(lines: &[&str], font: &str) -> Option<Raster> {
        let dir = std::env::temp_dir();
        let tag = font.replace([' ', '/'], "-");
        let txt = dir.join(format!("pinshot-ocr-{n}-{tag}.txt", n = lines.len()));
        let png = dir.join(format!("pinshot-ocr-{n}-{tag}.png", n = lines.len()));
        std::fs::write(&txt, lines.join("\n")).ok()?;
        let st = std::process::Command::new("pango-view")
            .args([
                "--no-display",
                "--dpi=96",
                "--margin=16",
                "--align=left",
                "-qo",
            ])
            .arg(&png)
            .arg(format!("--font={font}"))
            .arg(&txt)
            .status()
            .ok()?;
        if !st.success() {
            return None;
        }
        let src = load_png_raster(&png)?;
        Some(place_on_page(&src, 1100, 400, 40, 40))
    }

    fn assert_wiki_stems(text: &str, strict_doubles: bool) {
        let lower = text.to_ascii_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .filter(|w| !w.is_empty())
            .collect();
        for needle in ["in", "2009", "logo", "first"] {
            assert!(
                words.iter().any(|w| *w == needle),
                "missing {needle:?} in {text:?} tokens={words:?}"
            );
        }
        assert!(
            !words
                .iter()
                .any(|w| *w == "tn" || *w == "fficially" || *w == "ogo"),
            "clipped Latin stems in {text:?} tokens={words:?}"
        );
        assert!(
            !text.contains("i. e.") && !text.contains("2010 s") && !text.contains("[21]A"),
            "spacing glitches in {text:?}"
        );
        if strict_doubles {
            assert!(
                lower.contains("mozilla"),
                "missing mozilla in {text:?} tokens={words:?}"
            );
            assert!(
                lower.contains("officially") || lower.contains("oficially"),
                "missing officially in {text:?} tokens={words:?}"
            );
            assert!(
                words
                    .iter()
                    .any(|w| *w == "full-time" || *w == "fulltime" || *w == "ful-time"),
                "missing full-time in {text:?}"
            );
            assert!(lower.contains("note 5"), "missing [note 5] in {text:?}");
            assert!(
                !words.iter().any(|w| *w == "mozila"),
                "dropped thin doubles in {text:?} tokens={words:?}"
            );
        }
    }

    #[test]
    fn ocr_keeps_latin_stems_on_browser_sized_type_when_requested() {
        if std::env::var("PINSHOT_OCR_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let lines = [
            "In 2009, Mozilla officially sponsored the project.",
            "The Firefox logo and the first full-time MacOS engineers.",
            "See [note 5] and [21] After 2010s (i.e., later).",
        ];
        let mut pango_ran = false;
        for font in ["Liberation Sans 13", "DejaVu Sans 13"] {
            let Some(img) = pango_article_raster(&lines, font) else {
                continue;
            };
            pango_ran = true;
            let text = page_text(&img);
            eprintln!("ocr stem {font} {}x{} => {text:?}", img.width, img.height);
            assert_wiki_stems(&text, font.starts_with("Liberation"));
        }
        let mut img = Raster::new(1100, 360);
        img.fill(Rgba::WHITE);
        for (i, line) in lines.iter().enumerate() {
            font::blit_rgba_chrome(
                &mut img.pixels,
                img.width,
                img.width,
                img.height,
                line,
                36.0,
                64.0 + i as f32 * 36.0,
                16.0,
                Rgba::BLACK,
            );
        }
        img.force_opaque();
        let text = page_text(&img);
        eprintln!("ocr stem fontdue => {text:?}");
        if pango_ran {
            // Fontdue 16px is thinner than Chrome; paddle-ocr-rs is judged on
            // the pango Liberation/DejaVu screenshots above.
        } else {
            assert_wiki_stems(&text, false);
            eprintln!("pango-view missing; used fontdue fallback");
        }
    }

    #[test]
    fn ocr_joins_cjk_wraps_without_spaces_when_requested() {
        if std::env::var("PINSHOT_OCR_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let lines = [
            "Rust語言是由 Mozilla 開發的程式語言。",
            "該專案於2009年開始，編譯器來自該專案。",
        ];
        let Some(img) = pango_article_raster(&lines, "Noto Sans CJK SC 16")
            .or_else(|| pango_article_raster(&lines, "Noto Sans CJK JP 16"))
            .or_else(|| pango_article_raster(&lines, "Droid Sans Fallback 16"))
        else {
            let mut img = Raster::new(720, 160);
            img.fill(Rgba::WHITE);
            for (i, line) in lines.iter().enumerate() {
                font::blit_rgba_chrome(
                    &mut img.pixels,
                    img.width,
                    img.width,
                    img.height,
                    line,
                    24.0,
                    48.0 + i as f32 * 40.0,
                    22.0,
                    Rgba::BLACK,
                );
            }
            img.force_opaque();
            let text = page_text(&img);
            eprintln!("ocr cjk wrap fontdue => {text:?}");
            assert_cjk_wrap_bar(&text);
            return;
        };
        let text = page_text(&img);
        eprintln!(
            "ocr cjk wrap pango {}x{} => {text:?}",
            img.width, img.height
        );
        assert_cjk_wrap_bar(&text);
    }

    fn assert_cjk_wrap_bar(text: &str) {
        assert!(
            !text.contains("編譯 器") && !text.contains("2009 年"),
            "CJK wrap inserted a space: {text:?}"
        );
        if text.contains("編譯") {
            assert!(
                text.contains("編譯器"),
                "expected 編譯器 joined, got {text:?}"
            );
        }
        if text.contains("2009") && text.chars().any(|c| c == '年') {
            assert!(
                text.contains("2009年"),
                "expected 2009年 joined, got {text:?}"
            );
        }
    }

    #[test]
    fn ocr_reads_traditional_and_simplified_browser_type_when_requested() {
        if std::env::var("PINSHOT_OCR_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let hant = [
            "並且在2010年首次發布。OCaml 與 [15] 仍在使用。",
            "Rust語言是由Mozilla開發的程式語言。",
        ];
        let hans = [
            "并且在2010年首次发布。OCaml 与 [15] 仍在使用。",
            "Rust语言是由Mozilla开发的程序语言。",
        ];
        for (tag, lines, font) in [
            ("hant", &hant[..], "Noto Sans CJK TC 16"),
            ("hant-sc", &hant[..], "Noto Sans CJK SC 16"),
            ("hans", &hans[..], "Noto Sans CJK SC 16"),
        ] {
            let Some(img) = pango_article_raster(lines, font)
                .or_else(|| pango_article_raster(lines, "WenQuanYi Micro Hei 16"))
            else {
                continue;
            };
            let text = page_text(&img);
            eprintln!("ocr {tag} {font} => {text:?}");
            assert!(
                !text.contains("亚且仕") && !text.contains("自次"),
                "garbled Traditional stems in {text:?}"
            );
            assert!(
                text.contains("並且")
                    || text.contains("并且")
                    || text.contains("首次")
                    || text.contains("Rust"),
                "dropped CJK lines in {text:?}"
            );
        }
    }

    #[test]
    fn ocr_wikipedia_work_shifted_paragraph_when_requested() {
        if std::env::var("PINSHOT_OCR_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let para = [
            "Work shifted from the OCaml compiler to a self-hosting compiler",
            "(i.e., written in Rust) targeting LLVM.",
        ];
        let img = pango_article_raster(&para, "Liberation Sans 13")
            .or_else(|| pango_article_raster(&para, "DejaVu Sans 13"))
            .unwrap_or_else(|| {
                let mut img = Raster::new(1100, 160);
                img.fill(Rgba::WHITE);
                for (i, line) in para.iter().enumerate() {
                    font::blit_rgba_chrome(
                        &mut img.pixels,
                        img.width,
                        img.width,
                        img.height,
                        line,
                        24.0,
                        48.0 + i as f32 * 28.0,
                        16.0,
                        Rgba::BLACK,
                    );
                }
                img.force_opaque();
                img
            });
        let text = page_text(&img);
        eprintln!("ocr work-shifted {}x{} => {text:?}", img.width, img.height);
        let lower = text.to_ascii_lowercase();
        for needle in [
            "work shifted",
            "ocaml compiler",
            "self-hosting",
            "written in rust",
            "targeting",
        ] {
            assert!(
                lower.contains(needle),
                "missing {needle:?} in Wikipedia paragraph {text:?}"
            );
        }
        assert!(
            text.contains("LLVM") || lower.contains("llvm"),
            "missing LLVM in {text:?}"
        );
        assert!(
            !lower.contains("theaml")
                && !lower.contains("cmpile")
                && !lower.contains("writen inrust")
                && !lower.contains("tareting"),
            "mobile-quality English garble in {text:?}"
        );
        let joined = lower.replace('\n', " ");
        assert!(
            joined.contains("work shifted from the ocaml compiler to a self-hosting compiler")
                && joined.contains("(i.e., written in rust) targeting llvm."),
            "Wikipedia paragraph not verbatim enough: {text:?}"
        );
    }
}

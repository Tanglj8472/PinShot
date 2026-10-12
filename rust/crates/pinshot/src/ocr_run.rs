//! Download PP-OCRv5 ONNX models; run RapidOCR-style det/cls/rec.

use std::fs;
use std::io::Read;
use std::path::Path;

use pinshot_core::config::data_dir;
use pinshot_core::ocr::{
    ctc_decode_fracs, db_boxes, join_ocr_parts, latin_space_tweak, load_keys, model_dir,
    rec_resize_spec, text_crop_pad, ModelPaths, TextBox, REC_H,
};
use pinshot_core::raster::Raster;

pub fn recognize(image: &Raster) -> Result<String, String> {
    crate::logutil::log(&format!(
        "ocr recognize {}x{}",
        image.width, image.height
    ));
    let paths = ensure_models()?;
    infer(image, &paths)
}

pub fn ensure_models() -> Result<ModelPaths, String> {
    let dir = model_dir(&data_dir());
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let paths = ModelPaths::in_dir(&dir);
    if let Ok(bundled) = std::env::var("PINSHOT_OCR_MODELS") {
        copy_if_present(Path::new(&bundled), &paths);
    }
    if !paths.ready() {
        for (urls, dest) in paths.downloads() {
            if dest.is_file() {
                continue;
            }
            download_first(&urls, &dest)?;
        }
    }
    if !paths.ready() {
        return Err("OCR models missing (det/rec/keys)".into());
    }
    Ok(paths)
}

fn copy_if_present(src: &Path, dest: &ModelPaths) {
    for (from, to) in [
        (src.join(dest.det.file_name().unwrap_or_default()), &dest.det),
        (src.join(dest.rec.file_name().unwrap_or_default()), &dest.rec),
        (src.join(dest.keys.file_name().unwrap_or_default()), &dest.keys),
        (src.join(dest.cls.file_name().unwrap_or_default()), &dest.cls),
    ] {
        if from.is_file() && !to.is_file() {
            let _ = fs::copy(from, to);
        }
    }
}

fn download_first(urls: &[&str], dest: &Path) -> Result<(), String> {
    let mut last = "no urls".to_string();
    for url in urls {
        match download(url, dest) {
            Ok(()) => return Ok(()),
            Err(e) => {
                crate::logutil::log(&format!("ocr download fail {url}: {e}"));
                last = e;
            }
        }
    }
    Err(last)
}

fn download(url: &str, dest: &Path) -> Result<(), String> {
    crate::logutil::log(&format!("ocr download {url}"));
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(180))
        .call()
        .map_err(|e| format!("download {url}: {e}"))?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() < 64 {
        return Err(format!("download too small: {url}"));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("part");
    fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    crate::logutil::log(&format!("ocr saved {} ({} bytes)", dest.display(), bytes.len()));
    Ok(())
}

#[cfg(any(feature = "ocr", unix))]
fn infer(image: &Raster, paths: &ModelPaths) -> Result<String, String> {
    use ort::session::Session;

    if image.width < 8 || image.height < 8 {
        return Err("image too small for OCR".into());
    }
    let mut rec = Session::builder()
        .map_err(|e| e.to_string())?
        .commit_from_file(&paths.rec)
        .map_err(|e| format!("rec model: {e}"))?;
    let keys = load_keys(&fs::read_to_string(&paths.keys).map_err(|e| e.to_string())?);
    if keys.is_empty() {
        return Err("empty OCR alphabet".into());
    }
    let mut cls = if paths.cls.is_file() {
        Session::builder()
            .ok()
            .and_then(|b| b.commit_from_file(&paths.cls).ok())
    } else {
        None
    };
    let boxes = detect_boxes(image, paths).unwrap_or_else(|e| {
        crate::logutil::log(&format!("ocr det fallback: {e}"));
        vec![TextBox {
            x: 0,
            y: 0,
            w: image.width,
            h: image.height,
            score: 1.0,
        }]
    });
    crate::logutil::log(&format!("ocr boxes {}", boxes.len()));
    let mut parts: Vec<(TextBox, String)> = Vec::new();
    for b in boxes {
        crate::logutil::log(&format!("ocr box {}x{} at {},{}", b.w, b.h, b.x, b.y));
        // Tight DBNet AABBs clip the first/last Latin stem (`In`→`Tn`, `logo`→`ogo`).
        let (pad_x, pad_y) = text_crop_pad(b.h);
        let mut crop = image.crop_replicate(
            b.x as i32 - pad_x,
            b.y as i32 - pad_y,
            b.w + (pad_x as u32) * 2,
            b.h + (pad_y as u32) * 2,
        );
        if crop.width < 4 || crop.height < 4 {
            continue;
        }
        // Only rotate true vertical lines. Word boxes that are merely a bit
        // taller than wide must stay upright (Wikipedia captions, etc.).
        if crop.height as f32 > crop.width as f32 * 2.2 {
            crop = crop.rotate90(1);
        }
        if let Some(session) = cls.as_mut() {
            if cls_upside_down(session, &crop).unwrap_or(false) {
                crop = crop.rotate90(2);
            }
        }
        let (full, _fracs) = rec_line_ex(&mut rec, &crop, &keys).unwrap_or_default();
        // Spaces come from the rec model's space token (and CTC blank runs).
        // Do not split on ink gaps or a greedy English dictionary.
        let spaced = latin_space_tweak(&full);
        crate::logutil::log(&format!("ocr line raw={:?} spaced={:?}", full, spaced));
        if !spaced.trim().is_empty() {
            parts.push((b, spaced));
        } else {
            crate::logutil::log("ocr rec empty");
        }
    }
    let joined = join_ocr_parts(&parts);
    if joined.is_empty() {
        Err("OCR produced no text".into())
    } else {
        Ok(joined)
    }
}

#[cfg(any(feature = "ocr", unix))]
fn detect_boxes(image: &Raster, paths: &ModelPaths) -> Result<Vec<TextBox>, String> {
    use ort::session::Session;
    let mut det = Session::builder()
        .map_err(|e| e.to_string())?
        .commit_from_file(&paths.det)
        .map_err(|e| format!("det model: {e}"))?;
    let packed = pack_det(image, 960)?;
    let outputs = det
        .run(ort::inputs![packed.tensor])
        .map_err(|e| e.to_string())?;
    if outputs.len() == 0 {
        return Err("det empty".into());
    }
    let (shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    let dims: Vec<i64> = shape.iter().copied().collect();
    let (oh, ow) = match dims.as_slice() {
        [_, _, h, w] => (*h as u32, *w as u32),
        [_, h, w] => (*h as u32, *w as u32),
        [h, w] => (*h as u32, *w as u32),
        _ => (packed.nh, packed.nw),
    };
    let hw = (ow * oh) as usize;
    if data.len() < hw {
        return Err(format!("det output short {} < {hw}", data.len()));
    }
    // Use the last channel if the model emits (N, C, H, W) with C>1.
    let offset = data.len() - hw;
    let boxes = db_boxes(&data[offset..], ow, oh, image.width, image.height);
    if boxes.is_empty() {
        Err("no text boxes".into())
    } else {
        Ok(boxes)
    }
}

#[cfg(any(feature = "ocr", unix))]
struct Packed {
    tensor: ort::value::Value,
    nw: u32,
    nh: u32,
}

/// Python PinShot RapidOCR: `Det.limit_type=max`, `limit_side_len=960`.
/// RapidOCR's default `min`/736 upscales short screenshots ~5× and smears 13px type.
const DET_LIMIT: f32 = 960.0;

#[cfg(any(feature = "ocr", unix))]
fn pack_det(image: &Raster, max_side: u32) -> Result<Packed, String> {
    let w = image.width.max(1);
    let h = image.height.max(1);
    let limit = (max_side as f32).max(32.0).min(DET_LIMIT);
    let max_wh = w.max(h) as f32;
    let ratio = if max_wh > limit { limit / max_wh } else { 1.0 };
    // Ceil to 32 so 13px Wikipedia type is not squashed (round(108/32)*32 = 96).
    let mut nw = ((w as f32 * ratio / 32.0).ceil() as u32 * 32).max(32);
    let mut nh = ((h as f32 * ratio / 32.0).ceil() as u32 * 32).max(32);
    if nw == 0 {
        nw = 32;
    }
    if nh == 0 {
        nh = 32;
    }
    let resized = image.scale_linear(nw, nh);
    let tensor = nchw_bgr05(&resized)?;
    Ok(Packed {
        tensor,
        nw,
        nh,
    })
}

#[cfg(any(feature = "ocr", unix))]
fn nchw_bgr05(image: &Raster) -> Result<ort::value::Value, String> {
    let nw = image.width;
    let nh = image.height;
    let mut chw = vec![0f32; (3 * nw * nh) as usize];
    let plane = (nw * nh) as usize;
    for y in 0..nh {
        for x in 0..nw {
            let c = image.get(x as i32, y as i32).unwrap_or(pinshot_core::Rgba::BLACK);
            let i = (y * nw + x) as usize;
            // RapidOCR / OpenCV BGR, mean/std 0.5.
            chw[i] = (c.b as f32 / 255.0 - 0.5) / 0.5;
            chw[plane + i] = (c.g as f32 / 255.0 - 0.5) / 0.5;
            chw[2 * plane + i] = (c.r as f32 / 255.0 - 0.5) / 0.5;
        }
    }
    let t = ort::value::Tensor::from_array((vec![1i64, 3, nh as i64, nw as i64], chw))
        .map_err(|e| e.to_string())?;
    Ok(t.into())
}

#[cfg(any(feature = "ocr", unix))]
fn pack_rec(crop: &Raster) -> Result<ort::value::Value, String> {
    pack_rec_shaped(crop, rec_resize_spec(crop.width, crop.height))
}

#[cfg(any(feature = "ocr", unix))]
fn pack_rec_shaped(crop: &Raster, (resized_w, canvas_w): (u32, u32)) -> Result<ort::value::Value, String> {
    let nh = REC_H;
    // Align-corners bilinear keeps thin `l`/`i` apart better than OpenCV
    // half-pixel mapping when upscaling 13–16px Wikipedia type to H=48.
    let resized = crop.scale_bilinear(resized_w.max(1), nh);
    let mut chw = vec![0f32; (3 * canvas_w * nh) as usize];
    let plane = (canvas_w * nh) as usize;
    let copy_w = resized_w.min(canvas_w);
    for y in 0..nh {
        for x in 0..copy_w {
            let c = resized
                .get(x as i32, y as i32)
                .unwrap_or(pinshot_core::Rgba::WHITE);
            let i = (y * canvas_w + x) as usize;
            chw[i] = (c.b as f32 / 255.0 - 0.5) / 0.5;
            chw[plane + i] = (c.g as f32 / 255.0 - 0.5) / 0.5;
            chw[2 * plane + i] = (c.r as f32 / 255.0 - 0.5) / 0.5;
        }
    }
    let t = ort::value::Tensor::from_array((vec![1i64, 3, nh as i64, canvas_w as i64], chw))
        .map_err(|e| e.to_string())?;
    Ok(t.into())
}

#[cfg(any(feature = "ocr", unix))]
fn rec_line_ex(
    session: &mut ort::session::Session,
    crop: &Raster,
    keys: &[String],
) -> Result<(String, Vec<f32>), String> {
    let t = pack_rec(crop)?;
    let outputs = session.run(ort::inputs![t]).map_err(|e| e.to_string())?;
    if outputs.len() == 0 {
        return Err("rec empty".into());
    }
    let (shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    let dims: Vec<i64> = shape.iter().copied().collect();
    let nclass = keys.len();
    let (tlen, classes, transposed) = match dims.as_slice() {
        [_, t, c] if (*c as usize) >= nclass => (*t as usize, *c as usize, false),
        [_, c, t] if (*c as usize) >= nclass => (*t as usize, *c as usize, true),
        [_, _, t, c] if (*c as usize) >= nclass => (*t as usize, *c as usize, false),
        [_, _, c, t] if (*c as usize) >= nclass => (*t as usize, *c as usize, true),
        d if d.len() >= 3 => (
            d[d.len() - 2] as usize,
            d[d.len() - 1] as usize,
            false,
        ),
        d if d.len() == 2 => (d[0] as usize, d[1] as usize, false),
        _ => return Err("unexpected rec shape".into()),
    };
    crate::logutil::log(&format!(
        "ocr rec shape={dims:?} t={tlen} classes={classes} keys={} transposed={transposed}",
        keys.len()
    ));
    let mut idx = Vec::with_capacity(tlen);
    for ti in 0..tlen {
        let mut best = 0usize;
        let mut bv = f32::NEG_INFINITY;
        for c in 0..classes {
            let v = if transposed {
                data[c * tlen + ti]
            } else {
                data[ti * classes + c]
            };
            if v > bv {
                bv = v;
                best = c;
            }
        }
        idx.push(best);
    }
    Ok(decode_ctc_ex(&idx, keys, classes))
}

#[cfg(any(feature = "ocr", unix))]
fn decode_ctc_ex(idx: &[usize], keys: &[String], classes: usize) -> (String, Vec<f32>) {
    let norm = |s: String| s.replace('\u{3000}', " ").replace('\u{00a0}', " ");
    let (standard, fracs) = ctc_decode_fracs(idx, keys, 0);
    let standard = norm(standard);
    if !standard.trim().is_empty() {
        return (standard, fracs);
    }
    let (alt, alt_fracs) = ctc_decode_fracs(idx, keys, classes.saturating_sub(1));
    let alt = norm(alt);
    if alt.trim().is_empty() {
        (standard, fracs)
    } else {
        (alt, alt_fracs)
    }
}

#[cfg(any(feature = "ocr", unix))]
fn cls_upside_down(session: &mut ort::session::Session, crop: &Raster) -> Result<bool, String> {
    let (rw, _) = rec_resize_spec(crop.width, crop.height);
    let t = pack_rec_shaped(crop, (rw.min(192), 192))?;
    let outputs = session.run(ort::inputs![t]).map_err(|e| e.to_string())?;
    if outputs.len() == 0 {
        return Ok(false);
    }
    let (_shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    if data.len() < 2 {
        return Ok(false);
    }
    // Typical PP-OCR cls: [0° , 180°]. Rotate when 180° wins with margin.
    Ok(data[1] > data[0] && data[1] > 0.9)
}

#[cfg(not(any(feature = "ocr", unix)))]
fn infer(_image: &Raster, _paths: &ModelPaths) -> Result<String, String> {
    Err("OCR not available in this build. Use pinshot-ocr.exe.".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use pinshot_core::font;
    use pinshot_core::raster::Raster;
    use pinshot_core::Rgba;

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
        let text = recognize(&img).unwrap_or_else(|e| panic!("ocr: {e}"));
        eprintln!("ocr smoke => {text:?}");
        let has_en = text.to_ascii_lowercase().contains("hello")
            || text.to_ascii_lowercase().contains("pinshot");
        let has_zh = text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
        assert!(
            has_en && has_zh,
            "expected English + Chinese, got {text:?}"
        );
    }

    #[test]
    fn ocr_keeps_english_word_spaces_when_requested() {
        if std::env::var("PINSHOT_OCR_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let mut img = Raster::new(900, 120);
        img.fill(Rgba::WHITE);
        font::blit_rgba_chrome(
            &mut img.pixels,
            img.width,
            img.width,
            img.height,
            "In 2009, Mozilla officially sponsored the project.",
            16.0,
            36.0,
            28.0,
            Rgba::BLACK,
        );
        img.force_opaque();
        let text = recognize(&img).unwrap_or_else(|e| panic!("ocr: {e}"));
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
                && words.iter().any(|w| *w == "project" || *w == "mozilla" || *w == "sponsored"),
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
        let txt = dir.join(format!("pinshot-ocr-stem-{tag}.txt"));
        let png = dir.join(format!("pinshot-ocr-stem-{tag}.png"));
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
            !words.iter().any(|w| *w == "tn" || *w == "fficially" || *w == "ogo"),
            "clipped Latin stems in {text:?} tokens={words:?}"
        );
        assert!(
            !text.contains("i. e.") && !text.contains("2010 s") && !text.contains("[21]A"),
            "spacing glitches in {text:?}"
        );
        if strict_doubles {
            for needle in ["mozilla", "officially"] {
                assert!(
                    words.iter().any(|w| *w == needle),
                    "missing {needle:?} in {text:?} tokens={words:?}"
                );
            }
            assert!(
                words.iter().any(|w| *w == "full-time" || *w == "fulltime"),
                "missing full-time in {text:?}"
            );
            assert!(
                lower.contains("note 5"),
                "missing [note 5] in {text:?}"
            );
            assert!(
                !words.iter().any(|w| *w == "mozila" || *w == "ful-time"),
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
            let text = recognize(&img).unwrap_or_else(|e| panic!("ocr {font}: {e}"));
            eprintln!("ocr stem {font} {}x{} => {text:?}", img.width, img.height);
            assert_wiki_stems(&text, font.starts_with("Liberation"));
        }
        // Fontdue 16px is thinner than Chrome; still require first-letter + spacing.
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
        let text = recognize(&img).unwrap_or_else(|e| panic!("ocr: {e}"));
        eprintln!("ocr stem fontdue => {text:?}");
        assert_wiki_stems(&text, false);
        if !pango_ran {
            eprintln!("pango-view missing; skipped Liberation/DejaVu screenshots");
        }
    }
}

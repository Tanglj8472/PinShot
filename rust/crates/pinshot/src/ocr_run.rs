//! Download PP-OCRv5 ONNX models; run RapidOCR-style det/cls/rec.

use std::fs;
use std::io::Read;
use std::path::Path;

use pinshot_core::config::data_dir;
use pinshot_core::ocr::{
    ctc_greedy, ctc_greedy_blank, db_boxes, join_lines, load_keys, model_dir, ModelPaths, TextBox,
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
    let mut lines = Vec::new();
    for b in boxes {
        let pad = 3i32;
        let mut crop = image.crop(pinshot_core::geom::Rect::from_i32(
            b.x as i32 - pad,
            b.y as i32 - pad,
            b.w as i32 + pad * 2,
            b.h as i32 + pad * 2,
        ));
        if crop.width < 8 || crop.height < 8 {
            continue;
        }
        if crop.height as f32 > crop.width as f32 * 1.5 {
            crop = crop.rotate90(1);
        }
        if let Some(session) = cls.as_mut() {
            if cls_upside_down(session, &crop).unwrap_or(false) {
                crop = crop.rotate90(2);
            }
        }
        match rec_line(&mut rec, &crop, &keys) {
            Ok(text) if !text.trim().is_empty() => {
                crate::logutil::log(&format!("ocr line {:?}", text));
                lines.push(text);
            }
            Ok(_) => {}
            Err(e) => crate::logutil::log(&format!("ocr rec: {e}")),
        }
    }
    let joined = join_lines(&lines);
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
    let boxes = db_boxes(&data[offset..], ow, oh, image.width, image.height, packed.scale);
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
    scale: f32,
}

#[cfg(any(feature = "ocr", unix))]
fn pack_det(image: &Raster, max_side: u32) -> Result<Packed, String> {
    let max_wh = image.width.max(image.height).max(1);
    let scale = (max_side as f32 / max_wh as f32).min(1.0);
    let rw = ((image.width as f32 * scale).round() as u32).max(1);
    let rh = ((image.height as f32 * scale).round() as u32).max(1);
    let nw = rw.div_ceil(32) * 32;
    let nh = rh.div_ceil(32) * 32;
    let resized = image.scale_nn(rw, rh);
    let mut canvas = Raster::new(nw, nh);
    canvas.fill(pinshot_core::Rgba::BLACK);
    for y in 0..rh {
        for x in 0..rw {
            if let Some(c) = resized.get(x as i32, y as i32) {
                canvas.set(x as i32, y as i32, c);
            }
        }
    }
    let tensor = nchw_imagenet(&canvas)?;
    Ok(Packed {
        tensor,
        nw,
        nh,
        scale,
    })
}

#[cfg(any(feature = "ocr", unix))]
fn nchw_imagenet(image: &Raster) -> Result<ort::value::Value, String> {
    let nw = image.width;
    let nh = image.height;
    let mut chw = vec![0f32; (3 * nw * nh) as usize];
    let mean = [0.485f32, 0.456, 0.406];
    let std = [0.229f32, 0.224, 0.225];
    let plane = (nw * nh) as usize;
    for y in 0..nh {
        for x in 0..nw {
            let c = image.get(x as i32, y as i32).unwrap_or(pinshot_core::Rgba::BLACK);
            let i = (y * nw + x) as usize;
            chw[i] = (c.r as f32 / 255.0 - mean[0]) / std[0];
            chw[plane + i] = (c.g as f32 / 255.0 - mean[1]) / std[1];
            chw[2 * plane + i] = (c.b as f32 / 255.0 - mean[2]) / std[2];
        }
    }
    let t = ort::value::Tensor::from_array((vec![1i64, 3, nh as i64, nw as i64], chw))
        .map_err(|e| e.to_string())?;
    Ok(t.into())
}

#[cfg(any(feature = "ocr", unix))]
fn pack_rec(crop: &Raster) -> Result<ort::value::Value, String> {
    let h = crop.height.max(1) as f32;
    let mut nw = ((crop.width as f32) * (48.0 / h)).round() as u32;
    nw = (nw.clamp(16, 1280) / 8 * 8).max(8);
    let nh = 48u32;
    let resized = crop.scale_nn(nw, nh);
    let mut chw = vec![0f32; (3 * nw * nh) as usize];
    let plane = (nw * nh) as usize;
    for y in 0..nh {
        for x in 0..nw {
            let c = resized
                .get(x as i32, y as i32)
                .unwrap_or(pinshot_core::Rgba::WHITE);
            let i = (y * nw + x) as usize;
            chw[i] = (c.r as f32 / 255.0 - 0.5) / 0.5;
            chw[plane + i] = (c.g as f32 / 255.0 - 0.5) / 0.5;
            chw[2 * plane + i] = (c.b as f32 / 255.0 - 0.5) / 0.5;
        }
    }
    let t = ort::value::Tensor::from_array((vec![1i64, 3, nh as i64, nw as i64], chw))
        .map_err(|e| e.to_string())?;
    Ok(t.into())
}

#[cfg(any(feature = "ocr", unix))]
fn rec_line(
    session: &mut ort::session::Session,
    crop: &Raster,
    keys: &[String],
) -> Result<String, String> {
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
    let standard = ctc_greedy(&idx, keys);
    if !standard.trim().is_empty() {
        return Ok(standard);
    }
    let alt = ctc_greedy_blank(&idx, keys, classes.saturating_sub(1));
    Ok(if alt.trim().is_empty() { standard } else { alt })
}

#[cfg(any(feature = "ocr", unix))]
fn cls_upside_down(session: &mut ort::session::Session, crop: &Raster) -> Result<bool, String> {
    let t = pack_rec(crop)?;
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
}

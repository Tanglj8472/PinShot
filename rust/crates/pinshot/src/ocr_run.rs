//! Download PP-OCRv5 ONNX models; run them when built with `--features ocr`.

use std::fs;
use std::io::Read;
use std::path::Path;

use pinshot_core::config::data_dir;
use pinshot_core::ocr::{ctc_greedy, join_lines, load_keys, model_dir, ModelPaths};
use pinshot_core::raster::Raster;

pub fn recognize(image: &Raster) -> Result<String, String> {
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
        for (url, dest) in paths.downloads() {
            if dest.is_file() {
                continue;
            }
            download(url, &dest)?;
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
    ] {
        if from.is_file() && !to.is_file() {
            let _ = fs::copy(from, to);
        }
    }
}

fn download(url: &str, dest: &Path) -> Result<(), String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(120))
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
    Ok(())
}

#[cfg(feature = "ocr")]
fn infer(image: &Raster, paths: &ModelPaths) -> Result<String, String> {
    use ort::session::Session;

    let mut rec = Session::builder()
        .map_err(|e| e.to_string())?
        .commit_from_file(&paths.rec)
        .map_err(|e| e.to_string())?;
    let keys = load_keys(&fs::read_to_string(&paths.keys).map_err(|e| e.to_string())?);
    if keys.is_empty() {
        return Err("empty OCR alphabet".into());
    }
    let mut lines = Vec::new();
    let boxes = detect_boxes(image, paths).unwrap_or_else(|_| vec![(0, 0, image.width, image.height)]);
    for (x, y, w, h) in boxes {
        let crop = image.crop(pinshot_core::geom::Rect::from_i32(
            x as i32, y as i32, w as i32, h as i32,
        ));
        if crop.width < 8 || crop.height < 8 {
            continue;
        }
        if let Ok(text) = rec_line(&mut rec, &crop, &keys) {
            if !text.is_empty() {
                lines.push(text);
            }
        }
    }
    let joined = join_lines(&lines);
    if joined.is_empty() {
        Err("OCR produced no text".into())
    } else {
        Ok(joined)
    }
}

#[cfg(feature = "ocr")]
fn detect_boxes(image: &Raster, paths: &ModelPaths) -> Result<Vec<(u32, u32, u32, u32)>, String> {
    use ort::session::Session;
    let mut det = Session::builder()
        .map_err(|e| e.to_string())?
        .commit_from_file(&paths.det)
        .map_err(|e| e.to_string())?;
    let (tensor, nw, nh, scale) = pack_nchw(image, 960, false)?;
    let outputs = det.run(ort::inputs![tensor]).map_err(|e| e.to_string())?;
    if outputs.len() == 0 {
        return Err("det empty".into());
    }
    let (_shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    let mut row_on = vec![false; nh as usize];
    let hw = (nw * nh) as usize;
    if data.len() < hw {
        return Err("det output short".into());
    }
    for y in 0..nh as usize {
        let mut s = 0.0f32;
        for x in 0..nw as usize {
            s += data[y * nw as usize + x];
        }
        row_on[y] = (s / nw as f32) > 0.3;
    }
    let mut boxes = Vec::new();
    let mut y = 0;
    while y < row_on.len() {
        if !row_on[y] {
            y += 1;
            continue;
        }
        let y0 = y;
        while y < row_on.len() && row_on[y] {
            y += 1;
        }
        let src_y0 = ((y0 as f32) / scale).floor().max(0.0) as u32;
        let src_y1 = ((y as f32) / scale).ceil().min(image.height as f32) as u32;
        if src_y1 > src_y0 + 4 {
            boxes.push((0, src_y0, image.width, src_y1 - src_y0));
        }
    }
    if boxes.is_empty() {
        Err("no text bands".into())
    } else {
        Ok(boxes)
    }
}

#[cfg(feature = "ocr")]
fn pack_nchw(image: &Raster, max_side: u32, gray: bool) -> Result<(ort::value::Value, u32, u32, f32), String> {
    let scale = (max_side as f32 / image.width.max(image.height).max(1) as f32).min(1.0);
    let nw = ((image.width as f32 * scale) as u32).max(32) / 32 * 32;
    let nh = if gray {
        48
    } else {
        ((image.height as f32 * scale) as u32).max(32) / 32 * 32
    };
    let nw = if gray {
        (((image.width as f32) * (48.0 / image.height.max(1) as f32)).round() as u32)
            .clamp(16, 640)
            / 8
            * 8
    } else {
        nw
    };
    let resized = image.scale_nn(nw, nh);
    let mut chw = vec![0f32; (3 * nw * nh) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let c = resized
                .get(x as i32, y as i32)
                .unwrap_or(if gray {
                    pinshot_core::Rgba::WHITE
                } else {
                    pinshot_core::Rgba::BLACK
                });
            let i = (y * nw + x) as usize;
            if gray {
                let v = (0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32) / 255.0;
                let n = (v - 0.5) / 0.5;
                chw[i] = n;
                chw[(nw * nh) as usize + i] = n;
                chw[(2 * nw * nh) as usize + i] = n;
            } else {
                let mean = [0.485f32, 0.456, 0.406];
                let std = [0.229f32, 0.224, 0.225];
                chw[i] = (c.r as f32 / 255.0 - mean[0]) / std[0];
                chw[(nw * nh) as usize + i] = (c.g as f32 / 255.0 - mean[1]) / std[1];
                chw[(2 * nw * nh) as usize + i] = (c.b as f32 / 255.0 - mean[2]) / std[2];
            }
        }
    }
    let t = ort::value::Tensor::from_array((vec![1i64, 3, nh as i64, nw as i64], chw))
        .map_err(|e| e.to_string())?;
    Ok((t.into(), nw, nh, scale))
}

#[cfg(feature = "ocr")]
fn rec_line(
    session: &mut ort::session::Session,
    crop: &Raster,
    keys: &[String],
) -> Result<String, String> {
    let (t, _w, _h, _) = pack_nchw(crop, 640, true)?;
    let outputs = session.run(ort::inputs![t]).map_err(|e| e.to_string())?;
    if outputs.len() == 0 {
        return Err("rec empty".into());
    }
    let (shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    let dims: Vec<i64> = shape.iter().copied().collect();
    let (tlen, classes) = if dims.len() >= 3 {
        (dims[dims.len() - 2] as usize, dims[dims.len() - 1] as usize)
    } else if dims.len() == 2 {
        (dims[0] as usize, dims[1] as usize)
    } else {
        return Err("unexpected rec shape".into());
    };
    let mut idx = Vec::with_capacity(tlen);
    for ti in 0..tlen {
        let row = &data[ti * classes..(ti + 1) * classes];
        let mut best = 0usize;
        let mut bv = f32::NEG_INFINITY;
        for (i, v) in row.iter().enumerate() {
            if *v > bv {
                bv = *v;
                best = i;
            }
        }
        idx.push(best);
    }
    Ok(ctc_greedy(&idx, keys))
}

#[cfg(not(feature = "ocr"))]
fn infer(_image: &Raster, _paths: &ModelPaths) -> Result<String, String> {
    Err("OCR not available in this build. Use pinshot-ocr.exe.".into())
}

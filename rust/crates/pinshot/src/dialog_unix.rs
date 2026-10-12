//! Save / confirm dialogs on Linux (zenity when present, else Pictures + Y default).

use pinshot_core::color::{parse_color_text, Rgba};
use pinshot_core::i18n::tr;
use pinshot_core::raster::Raster;
use std::process::Command;

#[allow(dead_code)]
pub fn pick_color(_parent: (), current: Rgba) -> Option<Rgba> {
    let color = format!("#{:02x}{:02x}{:02x}", current.r, current.g, current.b);
    if let Ok(out) = Command::new("zenity")
        .args([
            "--color-selection",
            "--show-palette",
            &format!("--title={}", tr("Annotation color")),
            &format!("--color={color}"),
        ])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(c) = parse_color_text(&s) {
                return Some(c);
            }
            if let Some(c) = Rgba::from_hex(&s) {
                return Some(c);
            }
        }
    }
    None
}

pub fn confirm_abort(_parent: ()) -> bool {
    if let Ok(st) = Command::new("zenity")
        .args(["--question", "--title=PinShot", &format!("--text={}", tr("Abort snipping?"))])
        .status()
    {
        return st.success();
    }
    true
}

pub fn open_image_path() -> Option<String> {
    if let Ok(out) = Command::new("zenity")
        .args([
            "--file-selection",
            "--title",
            &tr("Replace from file"),
            "--file-filter=Images | *.png *.jpg *.jpeg *.bmp *.gif *.webp",
        ])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
        return None;
    }
    None
}

pub fn open_folder() -> Option<String> {
    if let Ok(out) = Command::new("zenity")
        .args([
            "--file-selection",
            "--directory",
            "--title",
            &tr("Quick-save folder"),
        ])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
        return None;
    }
    None
}

pub fn save_path(suggested: &str) -> Option<String> {
    if let Ok(out) = Command::new("zenity")
        .args([
            "--file-selection",
            "--save",
            "--confirm-overwrite",
            "--title",
            &tr("Save image as"),
            "--filename",
            suggested,
        ])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
        return None;
    }
    let pictures = std::env::var("XDG_PICTURES_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME").ok().map(|h| std::path::PathBuf::from(h).join("Pictures"))
        })
        .unwrap_or_else(|| std::env::temp_dir());
    let _ = std::fs::create_dir_all(&pictures);
    let name = format!(
        "pinshot-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    Some(pictures.join(name).display().to_string())
}

pub fn save_raster(img: &Raster, path: &str, jpeg_q: u8) -> Result<(), String> {
    let rgba = image::RgbaImage::from_raw(img.width, img.height, img.pixels.clone())
        .ok_or("invalid raster")?;
    let dynimg = image::DynamicImage::ImageRgba8(rgba);
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        let rgb = dynimg.to_rgb8();
        let mut f = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, jpeg_q);
        enc.encode(rgb.as_raw(), img.width, img.height, image::ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())?;
    } else {
        dynimg.save(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

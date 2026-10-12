use pinshot_core::i18n::tr;
use windows::Win32::Foundation::{HWND, MAX_PATH};
use windows::Win32::UI::Controls::Dialogs::{
    GetSaveFileNameW, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO};

use crate::win32::{pcw, wide};

pub fn pick_color(_parent: HWND, _current: pinshot_core::color::Rgba) -> Option<pinshot_core::color::Rgba> {
    None
}

pub fn confirm_abort(parent: HWND) -> bool {
    let title = wide("PinShot");
    let body = wide(&tr("Abort snipping?"));
    unsafe {
        MessageBoxW(
            parent,
            pcw(&body),
            pcw(&title),
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES
    }
}

pub fn save_path(suggested: &str) -> Option<String> {
    let mut file: Vec<u16> = wide(suggested);
    file.resize(MAX_PATH as usize, 0);
    let filter = wide("PNG (*.png)\0*.png\0JPEG (*.jpg)\0*.jpg;*.jpeg\0\0");
    let title = wide(&tr("Save image as"));
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: pcw(&filter),
        lpstrFile: windows::core::PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrTitle: pcw(&title),
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST,
        ..Default::default()
    };
    unsafe {
        if GetSaveFileNameW(&mut ofn).as_bool() {
            let end = file.iter().position(|&c| c == 0).unwrap_or(file.len());
            Some(String::from_utf16_lossy(&file[..end]))
        } else {
            None
        }
    }
}

pub fn open_image_path() -> Option<String> {
    let mut file: Vec<u16> = wide("pinshot.png");
    file.resize(MAX_PATH as usize, 0);
    let filter = wide("Images\0*.png;*.jpg;*.jpeg;*.bmp;*.gif;*.webp\0\0");
    let title = wide(&tr("Replace from file"));
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: pcw(&filter),
        lpstrFile: windows::core::PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrTitle: pcw(&title),
        Flags: OFN_PATHMUSTEXIST,
        ..Default::default()
    };
    unsafe {
        if windows::Win32::UI::Controls::Dialogs::GetOpenFileNameW(&mut ofn).as_bool() {
            let end = file.iter().position(|&c| c == 0).unwrap_or(file.len());
            Some(String::from_utf16_lossy(&file[..end]))
        } else {
            None
        }
    }
}

pub fn open_folder() -> Option<String> {
    None
}

pub fn save_raster(img: &pinshot_core::raster::Raster, path: &str, jpeg_q: u8) -> Result<(), String> {
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

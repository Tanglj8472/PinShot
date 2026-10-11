use arboard::{Clipboard, ImageData};
use pinshot_core::color::{parse_color_text, Rgba};
use pinshot_core::raster::Raster;

pub enum ClipContent {
    Image(Raster),
    Color(Rgba),
    Text(String),
    Empty,
}

pub fn write_image(img: &Raster) -> Result<(), String> {
    let mut cb = Clipboard::new().map_err(|e| e.to_string())?;
    let data = ImageData {
        width: img.width as usize,
        height: img.height as usize,
        bytes: img.pixels.clone().into(),
    };
    cb.set_image(data).map_err(|e| e.to_string())
}

pub fn write_text(s: &str) -> Result<(), String> {
    let mut cb = Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(s.to_string()).map_err(|e| e.to_string())
}

pub fn read_best() -> ClipContent {
    let Ok(mut cb) = Clipboard::new() else {
        return ClipContent::Empty;
    };
    if let Ok(img) = cb.get_image() {
        if let Some(r) = Raster::from_rgba(
            img.width as u32,
            img.height as u32,
            img.bytes.into_owned(),
        ) {
            return ClipContent::Image(r);
        }
    }
    if let Ok(text) = cb.get_text() {
        if let Some(c) = parse_color_text(&text) {
            return ClipContent::Color(c);
        }
        if !text.trim().is_empty() {
            return ClipContent::Text(text);
        }
    }
    ClipContent::Empty
}

pub fn color_card(c: Rgba, _w: u32, _h: u32) -> Raster {
    pinshot_core::font::color_card(c)
}

pub fn text_card(text: &str, _w: u32, _h: u32) -> Raster {
    pinshot_core::font::text_card(text, 640)
}

//! Portable text blit (fontdue + system font). Replaces the GDI host path.

use pinshot_core::color::Rgba;
use tiny_skia::{Pixmap, PremultipliedColorU8, Transform};

pub fn measure(text: &str, px: i32, _bold: bool) -> (u32, u32) {
    let (w, h) = pinshot_core::font::measure(text, px.max(8) as f32);
    ((w.ceil() as u32) + 8, (h.ceil() as u32) + 6)
}

pub fn blit_text(
    pixels: &mut [u8],
    stride: u32,
    dest_w: u32,
    dest_h: u32,
    text: &str,
    x: i32,
    y: i32,
    px: i32,
    color: Rgba,
    _bold: bool,
) {
    pinshot_core::font::blit_rgba(
        pixels,
        stride,
        dest_w,
        dest_h,
        text,
        x as f32,
        y as f32,
        px.max(8) as f32,
        color,
        false,
    );
}

pub fn blit_into_pm(pm: &mut Pixmap, text: &str, x: f32, y: f32, px: i32, color: Rgba, _bold: bool) {
    if text.is_empty() {
        return;
    }
    let (tw, th) = measure(text, px, false);
    if tw == 0 || th == 0 {
        return;
    }
    let mut rgba = vec![0u8; tw as usize * th as usize * 4];
    blit_text(&mut rgba, tw, tw, th, text, 0, 0, px, color, false);
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
        x as i32,
        y as i32,
        src.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        None,
    );
}

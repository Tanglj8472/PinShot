//! Portable chrome text blit (fontdue + CJK/Latin pairing).
//! Annotation canvas text stays on `pinshot_core::font::blit_pixmap_ex`.

use pinshot_core::color::Rgba;
use tiny_skia::{Pixmap, PremultipliedColorU8, Transform};

pub fn measure(text: &str, px: i32, _bold: bool) -> (u32, u32) {
    let (w, h) = pinshot_core::font::measure_chrome(text, px.max(8) as f32);
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
    pinshot_core::font::blit_rgba_chrome(
        pixels,
        stride,
        dest_w,
        dest_h,
        text,
        x as f32,
        y as f32,
        px.max(8) as f32,
        color,
    );
}

/// 2× rasterize + 2×2 box filter so 12px CJK/Latin chrome is smooth, not crunchy.
pub fn blit_into_pm(
    pm: &mut Pixmap,
    text: &str,
    x: f32,
    y: f32,
    px: i32,
    color: Rgba,
    _bold: bool,
) {
    if text.is_empty() {
        return;
    }
    let px = px.max(8);
    const SS: i32 = 2;
    let px2 = px * SS;
    let (w, h) = pinshot_core::font::measure_chrome(text, px2 as f32);
    let tw = (w.ceil() as u32 + 16).max(1);
    let th = (h.ceil() as u32 + 16).max(1);
    let mut rgba = vec![0u8; tw as usize * th as usize * 4];
    pinshot_core::font::blit_rgba_chrome(&mut rgba, tw, tw, th, text, 4.0, 4.0, px2 as f32, color);
    let dw = tw.div_ceil(2);
    let dh = th.div_ceil(2);
    let Some(mut src) = Pixmap::new(dw, dh) else {
        return;
    };
    downsample_box2(&rgba, tw, th, src.pixels_mut(), dw, dh);
    pm.draw_pixmap(
        x as i32 - 2,
        y as i32 - 2,
        src.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        None,
    );
}

fn downsample_box2(
    src: &[u8],
    sw: u32,
    sh: u32,
    dst: &mut [PremultipliedColorU8],
    dw: u32,
    dh: u32,
) {
    for y in 0..dh {
        for x in 0..dw {
            let mut sr = 0u32;
            let mut sg = 0u32;
            let mut sb = 0u32;
            let mut sa = 0u32;
            for oy in 0..2 {
                for ox in 0..2 {
                    let sx = (x * 2 + ox).min(sw.saturating_sub(1));
                    let sy = (y * 2 + oy).min(sh.saturating_sub(1));
                    let i = ((sy * sw + sx) * 4) as usize;
                    if i + 3 >= src.len() {
                        continue;
                    }
                    let a = src[i + 3] as u32;
                    sr += src[i] as u32 * a;
                    sg += src[i + 1] as u32 * a;
                    sb += src[i + 2] as u32 * a;
                    sa += a;
                }
            }
            let idx = (y * dw + x) as usize;
            if sa == 0 {
                dst[idx] = PremultipliedColorU8::TRANSPARENT;
                continue;
            }
            dst[idx] = PremultipliedColorU8::from_rgba(
                (sr / 4 / 255).min(255) as u8,
                (sg / 4 / 255).min(255) as u8,
                (sb / 4 / 255).min(255) as u8,
                (sa / 4).min(255) as u8,
            )
            .unwrap_or(PremultipliedColorU8::TRANSPARENT);
        }
    }
}

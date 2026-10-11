//! System-font rasterizer (fontdue). Used for annotation text, numbered markers,
//! clipboard cards, and host chrome when GDI is not available.

use crate::color::Rgba;
use crate::geom::Rect;
use crate::raster::Raster;
use std::sync::OnceLock;
use tiny_skia::{Pixmap, PremultipliedColorU8};

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

fn font() -> Option<&'static fontdue::Font> {
    static FONT: OnceLock<Option<fontdue::Font>> = OnceLock::new();
    FONT.get_or_init(|| {
        let settings = fontdue::FontSettings {
            collection_index: 0,
            scale: 40.0,
            ..Default::default()
        };
        for p in candidates() {
            let Ok(bytes) = std::fs::read(&p) else {
                continue;
            };
            if let Ok(f) = fontdue::Font::from_bytes(bytes, settings) {
                return Some(f);
            }
        }
        None
    })
    .as_ref()
}

pub fn available() -> bool {
    font().is_some()
}

pub fn measure(text: &str, px: f32) -> (f32, f32) {
    let Some(font) = font() else {
        let fs = px.max(8.0);
        let w = text
            .lines()
            .map(|l| l.chars().count() as f32 * fs * 0.6)
            .fold(0.0_f32, f32::max);
        let lines = text.lines().count().max(1) as f32;
        return (w, fs * 1.25 * lines);
    };
    let mut max_w = 0.0_f32;
    let mut h = 0.0_f32;
    let line_h = px * 1.25;
    for line in text.lines() {
        let mut w = 0.0_f32;
        for ch in line.chars() {
            let m = font.metrics(ch, px);
            w += m.advance_width;
        }
        max_w = max_w.max(w);
        h += line_h;
    }
    if text.is_empty() {
        h = line_h;
    }
    (max_w, h.max(px))
}

fn blend(dst: &mut [u8], i: usize, r: u8, g: u8, b: u8, a: u8) {
    if i + 3 >= dst.len() || a == 0 {
        return;
    }
    let ia = 255 - a as u16;
    dst[i] = ((r as u16 * a as u16 + dst[i] as u16 * ia) / 255) as u8;
    dst[i + 1] = ((g as u16 * a as u16 + dst[i + 1] as u16 * ia) / 255) as u8;
    dst[i + 2] = ((b as u16 * a as u16 + dst[i + 2] as u16 * ia) / 255) as u8;
    dst[i + 3] = 255;
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
    let px = px.max(8.0);
    let line_h = px * 1.25;
    for (li, line) in text.lines().enumerate() {
        let mut pen_x = x;
        let baseline = y + px + li as f32 * line_h;
        for ch in line.chars() {
            let (metrics, bitmap) = font.rasterize(ch, px);
            let gx = pen_x + metrics.xmin as f32;
            let gy = baseline - metrics.height as f32 - metrics.ymin as f32;
            for by in 0..metrics.height {
                for bx in 0..metrics.width {
                    let cov = bitmap[by * metrics.width + bx];
                    if cov < 8 {
                        continue;
                    }
                    let dx = gx as i32 + bx as i32;
                    let dy = gy as i32 + by as i32;
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
                }
            }
            pen_x += metrics.advance_width;
        }
    }
}

pub fn blit_pixmap(pm: &mut Pixmap, text: &str, x: f32, y: f32, px: f32, color: Rgba, outline: bool) {
    if text.is_empty() {
        return;
    }
    let (tw, th) = measure(text, px);
    let tw = (tw.ceil() as u32 + 8).max(1);
    let th = (th.ceil() as u32 + 8).max(1);
    let mut rgba = vec![0u8; tw as usize * th as usize * 4];
    blit_rgba(&mut rgba, tw, tw, th, text, 2.0, 2.0, px, color, outline);
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

pub fn blit_raster(img: &mut Raster, text: &str, x: f32, y: f32, px: f32, color: Rgba, outline: bool) {
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
        blit_raster(&mut img, k, (sw + 22) as f32, y, 12.0, Rgba::rgb(85, 85, 85), false);
        blit_raster(&mut img, v, (sw + 92) as f32, y, 12.0, Rgba::rgb(17, 17, 17), false);
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
        (
            "RGB float".into(),
            format!("{rf:.3}, {gf:.3}, {bf:.3}"),
        ),
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
}

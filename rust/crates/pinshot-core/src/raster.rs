//! Owned RGBA8 image used as the capture / compose buffer.

use crate::color::Rgba;
use crate::geom::Rect;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Raster {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        if pixels.len() != width as usize * height as usize * 4 {
            return None;
        }
        Some(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn from_bgra(width: u32, height: u32, bgra: &[u8], stride: usize) -> Self {
        Self::from_bgra_ex(width, height, bgra, stride, false)
    }

    /// Desktop capture: GDI `GetDIBits` / DIBSection leaves alpha = 0, which later
    /// premultiplies to a solid black pixmap. Force opaque unless the source is RGBA.
    pub fn from_bgra_opaque(width: u32, height: u32, bgra: &[u8], stride: usize) -> Self {
        Self::from_bgra_ex(width, height, bgra, stride, true)
    }

    fn from_bgra_ex(width: u32, height: u32, bgra: &[u8], stride: usize, force_opaque: bool) -> Self {
        let mut out = Self::new(width, height);
        if stride == 0 {
            return out;
        }
        for y in 0..height as usize {
            let row = y * stride;
            if row >= bgra.len() {
                break;
            }
            let src = &bgra[row..];
            for x in 0..width as usize {
                let i = x * 4;
                if i + 3 >= src.len() {
                    break;
                }
                let dst = (y * width as usize + x) * 4;
                out.pixels[dst] = src[i + 2];
                out.pixels[dst + 1] = src[i + 1];
                out.pixels[dst + 2] = src[i];
                let a = src[i + 3];
                out.pixels[dst + 3] = if force_opaque || a == 0 { 255 } else { a };
            }
        }
        out
    }

    pub fn force_opaque(&mut self) {
        for px in self.pixels.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }

    /// Sample a 16×16 grid (same idea as Python `_is_blank`): true if every sample is near-black.
    pub fn is_visually_blank(&self) -> bool {
        if self.width == 0 || self.height == 0 {
            return true;
        }
        let nx = 16u32.min(self.width);
        let ny = 16u32.min(self.height);
        for iy in 0..ny {
            let y = iy * (self.height - 1) / (ny - 1).max(1);
            for ix in 0..nx {
                let x = ix * (self.width - 1) / (nx - 1).max(1);
                if let Some(c) = self.get(x as i32, y as i32) {
                    if c.r > 8 || c.g > 8 || c.b > 8 {
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn fill(&mut self, c: Rgba) {
        for px in self.pixels.chunks_exact_mut(4) {
            px[0] = c.r;
            px[1] = c.g;
            px[2] = c.b;
            px[3] = c.a;
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<Rgba> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        Some(Rgba::rgba(
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ))
    }

    pub fn get_clamped(&self, x: i32, y: i32) -> Rgba {
        let x = x.clamp(0, self.width.saturating_sub(1) as i32);
        let y = y.clamp(0, self.height.saturating_sub(1) as i32);
        self.get(x, y).unwrap_or(Rgba::BLACK)
    }

    pub fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        self.pixels[i] = c.r;
        self.pixels[i + 1] = c.g;
        self.pixels[i + 2] = c.b;
        self.pixels[i + 3] = c.a;
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width as f32, self.height as f32)
    }

    pub fn crop(&self, r: Rect) -> Self {
        let (x, y, w, h) = r.intersect(self.bounds()).to_i32();
        let w = w.max(0) as u32;
        let h = h.max(0) as u32;
        let mut out = Self::new(w, h);
        for yy in 0..h {
            for xx in 0..w {
                if let Some(c) = self.get(x + xx as i32, y + yy as i32) {
                    out.set(xx as i32, yy as i32, c);
                }
            }
        }
        out
    }

    /// Block-average mosaic over the whole image (used as an effect source).
    pub fn mosaic(&self, block: u32) -> Self {
        let b = block.max(2);
        let mut out = self.clone();
        let w = self.width;
        let h = self.height;
        let mut y = 0u32;
        while y < h {
            let mut x = 0u32;
            while x < w {
                let x1 = (x + b).min(w);
                let y1 = (y + b).min(h);
                let mut sr = 0u64;
                let mut sg = 0u64;
                let mut sb = 0u64;
                let mut sa = 0u64;
                let mut n = 0u64;
                for yy in y..y1 {
                    for xx in x..x1 {
                        if let Some(c) = self.get(xx as i32, yy as i32) {
                            sr += c.r as u64;
                            sg += c.g as u64;
                            sb += c.b as u64;
                            sa += c.a as u64;
                            n += 1;
                        }
                    }
                }
                if n > 0 {
                    let avg = Rgba::rgba(
                        (sr / n) as u8,
                        (sg / n) as u8,
                        (sb / n) as u8,
                        (sa / n) as u8,
                    );
                    for yy in y..y1 {
                        for xx in x..x1 {
                            out.set(xx as i32, yy as i32, avg);
                        }
                    }
                }
                x += b;
            }
            y += b;
        }
        out
    }

    /// Cheap box-ish blur via downsample + upsample (Python EffectCache "blur").
    pub fn blur(&self, size: u32) -> Self {
        let f = size.max(2);
        self.scale_nn((self.width / f).max(1), (self.height / f).max(1))
            .scale_nn(self.width, self.height)
    }

    pub fn scale_nn(&self, nw: u32, nh: u32) -> Self {
        let mut out = Self::new(nw, nh);
        if self.width == 0 || self.height == 0 {
            return out;
        }
        for y in 0..nh {
            let sy = y * self.height / nh;
            for x in 0..nw {
                let sx = x * self.width / nw;
                if let Some(c) = self.get(sx as i32, sy as i32) {
                    out.set(x as i32, y as i32, c);
                }
            }
        }
        out
    }

    /// Crop `x,y,w,h` (may extend past the image) replicating edge pixels
    /// (RapidOCR `BORDER_REPLICATE` on text-line crops).
    pub fn crop_replicate(&self, x: i32, y: i32, w: u32, h: u32) -> Self {
        let mut out = Self::new(w, h);
        if self.width == 0 || self.height == 0 {
            return out;
        }
        for yy in 0..h {
            for xx in 0..w {
                let c = self.get_clamped(x + xx as i32, y + yy as i32);
                out.set(xx as i32, yy as i32, c);
            }
        }
        out
    }

    /// OpenCV INTER_LINEAR mapping: `(x + 0.5) * scale - 0.5`.
    pub fn scale_linear(&self, nw: u32, nh: u32) -> Self {
        let mut out = Self::new(nw, nh);
        if self.width == 0 || self.height == 0 || nw == 0 || nh == 0 {
            return out;
        }
        let x_scale = self.width as f32 / nw as f32;
        let y_scale = self.height as f32 / nh as f32;
        let lerp = |a: u8, b: u8, t: f32| -> u8 {
            (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
        };
        let sample = |fx: f32, fy: f32| -> Rgba {
            let x0 = fx.floor() as i32;
            let y0 = fy.floor() as i32;
            let x1 = x0 + 1;
            let y1 = y0 + 1;
            let tx = fx - x0 as f32;
            let ty = fy - y0 as f32;
            let c00 = self.get_clamped(x0, y0);
            let c10 = self.get_clamped(x1, y0);
            let c01 = self.get_clamped(x0, y1);
            let c11 = self.get_clamped(x1, y1);
            Rgba {
                r: lerp(lerp(c00.r, c10.r, tx), lerp(c01.r, c11.r, tx), ty),
                g: lerp(lerp(c00.g, c10.g, tx), lerp(c01.g, c11.g, tx), ty),
                b: lerp(lerp(c00.b, c10.b, tx), lerp(c01.b, c11.b, tx), ty),
                a: lerp(lerp(c00.a, c10.a, tx), lerp(c01.a, c11.a, tx), ty),
            }
        };
        for y in 0..nh {
            let fy = (y as f32 + 0.5) * y_scale - 0.5;
            for x in 0..nw {
                let fx = (x as f32 + 0.5) * x_scale - 0.5;
                out.set(x as i32, y as i32, sample(fx, fy));
            }
        }
        out
    }

    /// Bilinear resize for OCR rec (NN at H=48 smears thin Latin strokes).
    pub fn scale_bilinear(&self, nw: u32, nh: u32) -> Self {
        let mut out = Self::new(nw, nh);
        if self.width == 0 || self.height == 0 || nw == 0 || nh == 0 {
            return out;
        }
        let x_ratio = if nw <= 1 {
            0.0
        } else {
            (self.width.saturating_sub(1) as f32) / (nw - 1) as f32
        };
        let y_ratio = if nh <= 1 {
            0.0
        } else {
            (self.height.saturating_sub(1) as f32) / (nh - 1) as f32
        };
        let lerp = |a: u8, b: u8, t: f32| -> u8 {
            (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
        };
        for y in 0..nh {
            let fy = y as f32 * y_ratio;
            let y0 = fy.floor() as i32;
            let y1 = (y0 + 1).min(self.height as i32 - 1);
            let ty = fy - y0 as f32;
            for x in 0..nw {
                let fx = x as f32 * x_ratio;
                let x0 = fx.floor() as i32;
                let x1 = (x0 + 1).min(self.width as i32 - 1);
                let tx = fx - x0 as f32;
                let c00 = self.get(x0, y0).unwrap_or(Rgba::WHITE);
                let c10 = self.get(x1, y0).unwrap_or(Rgba::WHITE);
                let c01 = self.get(x0, y1).unwrap_or(Rgba::WHITE);
                let c11 = self.get(x1, y1).unwrap_or(Rgba::WHITE);
                out.set(
                    x as i32,
                    y as i32,
                    Rgba {
                        r: lerp(lerp(c00.r, c10.r, tx), lerp(c01.r, c11.r, tx), ty),
                        g: lerp(lerp(c00.g, c10.g, tx), lerp(c01.g, c11.g, tx), ty),
                        b: lerp(lerp(c00.b, c10.b, tx), lerp(c01.b, c11.b, tx), ty),
                        a: lerp(lerp(c00.a, c10.a, tx), lerp(c01.a, c11.a, tx), ty),
                    },
                );
            }
        }
        out
    }

    pub fn to_xrgb(&self) -> Vec<u32> {
        self.pixels
            .chunks_exact(4)
            .map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32)
            .collect()
    }

    pub fn to_skia(&self) -> Option<tiny_skia::Pixmap> {
        let mut pm = tiny_skia::Pixmap::new(self.width, self.height)?;
        for (dst, src) in pm.pixels_mut().iter_mut().zip(self.pixels.chunks_exact(4)) {
            // GDI 32-bit DIBs leave alpha 0 while RGB is valid — treat that as opaque.
            let a = if src[3] == 0 && (src[0] | src[1] | src[2]) != 0 {
                255
            } else {
                src[3]
            };
            *dst = tiny_skia::PremultipliedColorU8::from_rgba(
                ((src[0] as u16 * a as u16) / 255) as u8,
                ((src[1] as u16 * a as u16) / 255) as u8,
                ((src[2] as u16 * a as u16) / 255) as u8,
                a,
            )
            .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
        }
        Some(pm)
    }

    /// 90° steps. `turns > 0` is clockwise (Python `QTransform().rotate(+90)`).
    pub fn rotate90(&self, turns: i32) -> Self {
        let t = ((turns % 4) + 4) % 4;
        if t == 0 {
            return self.clone();
        }
        if t == 2 {
            let mut out = Self::new(self.width, self.height);
            for y in 0..self.height {
                for x in 0..self.width {
                    if let Some(c) = self.get(x as i32, y as i32) {
                        out.set((self.width - 1 - x) as i32, (self.height - 1 - y) as i32, c);
                    }
                }
            }
            return out;
        }
        let (nw, nh) = (self.height, self.width);
        let mut out = Self::new(nw, nh);
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(c) = self.get(x as i32, y as i32) {
                    if t == 1 {
                        out.set((self.height - 1 - y) as i32, x as i32, c);
                    } else {
                        out.set(y as i32, (self.width - 1 - x) as i32, c);
                    }
                }
            }
        }
        out
    }

    pub fn grayscale(&self) -> Self {
        let mut out = self.clone();
        for px in out.pixels.chunks_exact_mut(4) {
            let y = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32).round() as u8;
            px[0] = y;
            px[1] = y;
            px[2] = y;
        }
        out
    }

    pub fn invert_color(&self) -> Self {
        let mut out = self.clone();
        for px in out.pixels.chunks_exact_mut(4) {
            px[0] = 255 - px[0];
            px[1] = 255 - px[1];
            px[2] = 255 - px[2];
        }
        out
    }

    pub fn flip(&self, horizontal: bool) -> Self {
        let mut out = Self::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(c) = self.get(x as i32, y as i32) {
                    if horizontal {
                        out.set((self.width - 1 - x) as i32, y as i32, c);
                    } else {
                        out.set(x as i32, (self.height - 1 - y) as i32, c);
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_and_get() {
        let mut r = Raster::new(10, 8);
        r.set(3, 4, Rgba::BLUE);
        let c = r.crop(Rect::new(2.0, 3.0, 4.0, 3.0));
        assert_eq!(c.width, 4);
        assert_eq!(c.height, 3);
        assert_eq!(c.get(1, 1), Some(Rgba::BLUE));
    }

    #[test]
    fn mosaic_flattens_block() {
        let mut r = Raster::new(4, 4);
        r.set(0, 0, Rgba::rgb(0, 0, 0));
        r.set(1, 0, Rgba::rgb(254, 0, 0));
        let m = r.mosaic(2);
        let a = m.get(0, 0).unwrap();
        let b = m.get(1, 0).unwrap();
        assert_eq!(a, b);
        assert!(a.r > 0);
    }

    #[test]
    fn bgra_swizzle() {
        let bgra = [10u8, 20, 30, 255];
        let r = Raster::from_bgra(1, 1, &bgra, 4);
        assert_eq!(r.get(0, 0), Some(Rgba::rgba(30, 20, 10, 255)));
    }

    #[test]
    fn gdi_zero_alpha_stays_visible() {
        let bgra = [10u8, 20, 30, 0];
        let r = Raster::from_bgra_opaque(1, 1, &bgra, 4);
        assert_eq!(r.get(0, 0), Some(Rgba::rgba(30, 20, 10, 255)));
        let pm = r.to_skia().unwrap();
        let c = pm.pixel(0, 0).unwrap().demultiply();
        assert_eq!((c.red(), c.green(), c.blue(), c.alpha()), (30, 20, 10, 255));
    }

    #[test]
    fn blank_detection() {
        let mut r = Raster::new(32, 32);
        assert!(r.is_visually_blank());
        r.set(16, 16, Rgba::BLUE);
        assert!(!r.is_visually_blank());
    }

    #[test]
    fn to_skia_zero_alpha_with_color_is_visible() {
        let mut r = Raster::new(1, 1);
        r.set(0, 0, Rgba::rgba(30, 20, 10, 0));
        let pm = r.to_skia().unwrap();
        let c = pm.pixel(0, 0).unwrap().demultiply();
        assert_eq!((c.red(), c.green(), c.blue(), c.alpha()), (30, 20, 10, 255));
    }

    #[test]
    fn grayscale_and_invert() {
        let mut r = Raster::new(1, 1);
        r.set(0, 0, Rgba::rgb(255, 0, 0));
        let g = r.grayscale().get(0, 0).unwrap();
        assert_eq!(g.r, g.g);
        assert_eq!(g.g, g.b);
        let inv = r.invert_color().get(0, 0).unwrap();
        assert_eq!(inv, Rgba::rgb(0, 255, 255));
    }

    #[test]
    fn rotate_and_flip() {
        let mut r = Raster::new(2, 1);
        r.set(0, 0, Rgba::RED);
        r.set(1, 0, Rgba::BLUE);
        let cw = r.rotate90(1);
        assert_eq!(cw.width, 1);
        assert_eq!(cw.height, 2);
        assert_eq!(cw.get(0, 0), Some(Rgba::RED));
        assert_eq!(cw.get(0, 1), Some(Rgba::BLUE));
        let h = r.flip(true);
        assert_eq!(h.get(0, 0), Some(Rgba::BLUE));
        assert_eq!(h.get(1, 0), Some(Rgba::RED));
    }

    #[test]
    fn short_bgra_buffer_does_not_panic() {
        let r = Raster::from_bgra_opaque(4, 2, &[], 16);
        assert_eq!(r.width, 4);
        assert!(r.is_visually_blank());
    }

    #[test]
    fn bilinear_keeps_corners() {
        let mut r = Raster::new(2, 2);
        r.set(0, 0, Rgba::rgb(0, 0, 0));
        r.set(1, 0, Rgba::rgb(255, 0, 0));
        r.set(0, 1, Rgba::rgb(0, 255, 0));
        r.set(1, 1, Rgba::rgb(0, 0, 255));
        let s = r.scale_bilinear(4, 4);
        assert_eq!(s.width, 4);
        assert_eq!(s.get(0, 0), Some(Rgba::rgb(0, 0, 0)));
        assert_eq!(s.get(3, 0).unwrap().r, 255);
    }

    #[test]
    fn scale_linear_matches_opencv_center() {
        let mut r = Raster::new(2, 1);
        r.set(0, 0, Rgba::rgb(0, 0, 0));
        r.set(1, 0, Rgba::rgb(255, 0, 0));
        let s = r.scale_linear(4, 1);
        assert_eq!(s.width, 4);
        assert_eq!(s.get(0, 0), Some(Rgba::rgb(0, 0, 0)));
        assert_eq!(s.get(3, 0).unwrap().r, 255);
    }

    #[test]
    fn crop_replicate_pads_with_edge() {
        let mut r = Raster::new(2, 2);
        r.set(0, 0, Rgba::rgb(10, 0, 0));
        r.set(1, 0, Rgba::rgb(20, 0, 0));
        r.set(0, 1, Rgba::rgb(30, 0, 0));
        r.set(1, 1, Rgba::rgb(40, 0, 0));
        let c = r.crop_replicate(-1, -1, 4, 3);
        assert_eq!(c.width, 4);
        assert_eq!(c.get(0, 0), Some(Rgba::rgb(10, 0, 0)));
        assert_eq!(c.get(1, 1), Some(Rgba::rgb(10, 0, 0)));
        assert_eq!(c.get(2, 1), Some(Rgba::rgb(20, 0, 0)));
    }
}

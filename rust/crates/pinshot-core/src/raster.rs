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
        let mut out = Self::new(width, height);
        for y in 0..height as usize {
            let src = &bgra[y * stride..];
            for x in 0..width as usize {
                let i = x * 4;
                if i + 3 >= src.len() {
                    break;
                }
                let dst = (y * width as usize + x) * 4;
                out.pixels[dst] = src[i + 2];
                out.pixels[dst + 1] = src[i + 1];
                out.pixels[dst + 2] = src[i];
                out.pixels[dst + 3] = src[i + 3];
            }
        }
        out
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

    pub fn to_xrgb(&self) -> Vec<u32> {
        self.pixels
            .chunks_exact(4)
            .map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32)
            .collect()
    }

    pub fn to_skia(&self) -> Option<tiny_skia::Pixmap> {
        let mut pm = tiny_skia::Pixmap::new(self.width, self.height)?;
        for (dst, src) in pm.pixels_mut().iter_mut().zip(self.pixels.chunks_exact(4)) {
            *dst = tiny_skia::PremultipliedColorU8::from_rgba(
                ((src[0] as u16 * src[3] as u16) / 255) as u8,
                ((src[1] as u16 * src[3] as u16) / 255) as u8,
                ((src[2] as u16 * src[3] as u16) / 255) as u8,
                src[3],
            )
            .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
        }
        Some(pm)
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
}

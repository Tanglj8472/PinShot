//! tiny-skia overlay / pin / toolbar painter.
//!
//! Overlay frames are composited incrementally: the 6k-class virtual desktop is
//! *not* rebuilt on every mouse move. Empty-annot tiles restore from the
//! pre-dimmed freeze. Ink is painted on the undimmed capture first, then the
//! dim is applied *outside* the hole (参考工具: strokes in the shadow sit
//! under the dim). Chrome (ring / loupe / toolbar) is last and stays bright.

#[cfg(windows)]
use std::ffi::c_void;

use pinshot_core::annotation::{Annotator, Tool};
use pinshot_core::color::Rgba;
use pinshot_core::geom::{handle_points, Point, Rect, HANDLE_SIZE};
use pinshot_core::i18n::tr;
use pinshot_core::icons::{draw_icon, IconKind};
use pinshot_core::magnifier::{layout_magnifier, Magnifier, MAG_HINT_PX, MAG_N, MAG_ZOOM};
use pinshot_core::raster::Raster;
use pinshot_core::render::{compose, paint_shapes};
use pinshot_core::selection::{SelState, Selection};
use tiny_skia::{
    BlendMode, FillRule, Paint, Path, PathBuilder, Pixmap, PixmapPaint, PremultipliedColorU8,
    Stroke, Transform,
};
#[cfg(windows)]
use windows::Win32::Foundation::HWND;
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    RGBQUAD, SRCCOPY,
};

use crate::native::NativeWin;
use crate::toolbar::{ExtraKind, TbAction, TbExtra, Toolbar};

/// Fallback / prefs / pin-menu chrome colours (Python `theme.py` light/dark).
#[derive(Clone, Copy, Debug)]
pub struct UiTheme {
    pub bg: Rgba,
    pub panel: Rgba,
    pub hover: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub border: Rgba,
    pub accent: Rgba,
}

impl UiTheme {
    pub fn resolve(name: &str) -> Self {
        let dark = match name {
            "dark" => true,
            "light" => false,
            _ => {
                std::env::var("GTK_THEME")
                    .map(|t| t.to_ascii_lowercase().contains("dark"))
                    .unwrap_or(false)
                    || std::env::var("GTK_APPLICATION_PREFER_DARK_THEME")
                        .map(|v| v == "1")
                        .unwrap_or(false)
            }
        };
        if dark {
            Self {
                bg: Rgba::rgb(36, 38, 44),
                panel: Rgba::rgb(50, 52, 60),
                hover: Rgba::rgb(68, 92, 128),
                text: Rgba::rgb(236, 237, 240),
                muted: Rgba::rgb(164, 166, 174),
                border: Rgba::rgb(82, 84, 92),
                accent: Rgba::rgb(80, 150, 220),
            }
        } else {
            Self {
                bg: Rgba::rgb(252, 252, 253),
                panel: Rgba::rgb(243, 244, 247),
                hover: Rgba::rgb(236, 241, 247),
                text: Rgba::rgb(36, 36, 40),
                muted: Rgba::rgb(108, 110, 118),
                border: Rgba::rgb(206, 208, 214),
                accent: Rgba::rgb(61, 143, 209),
            }
        }
    }
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c.to_skia());
    p.anti_alias = true;
    p
}

pub fn fill_rect(pm: &mut Pixmap, r: Rect, c: Rgba) {
    if r.is_empty() {
        return;
    }
    if let Some(sr) = tiny_skia::Rect::from_xywh(r.x, r.y, r.w.max(0.5), r.h.max(0.5)) {
        pm.fill_rect(sr, &paint(c), Transform::identity(), None);
    }
}

/// Python `QPainter.drawRoundedRect` (cubic circle approximation).
pub fn fill_rounded_rect(pm: &mut Pixmap, r: Rect, radius: f32, c: Rgba) {
    if r.is_empty() {
        return;
    }
    if let Some(path) = rounded_path(r, radius) {
        pm.fill_path(
            &path,
            &paint(c),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

pub fn stroke_rounded_rect(pm: &mut Pixmap, r: Rect, radius: f32, c: Rgba, width: f32) {
    if r.is_empty() {
        return;
    }
    if let Some(path) = rounded_path(r, radius) {
        pm.stroke_path(
            &path,
            &paint(c),
            &Stroke {
                width,
                ..Stroke::default()
            },
            Transform::identity(),
            None,
        );
    }
}

fn rounded_path(r: Rect, radius: f32) -> Option<Path> {
    let rad = radius.min(r.w / 2.0).min(r.h / 2.0);
    if rad < 0.5 {
        let sr = tiny_skia::Rect::from_xywh(r.x, r.y, r.w.max(0.5), r.h.max(0.5))?;
        return Some(PathBuilder::from_rect(sr));
    }
    let k = rad * 0.552_284_75;
    let x0 = r.x;
    let y0 = r.y;
    let x1 = r.right();
    let y1 = r.bottom();
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + rad, y0);
    pb.line_to(x1 - rad, y0);
    pb.cubic_to(x1 - rad + k, y0, x1, y0 + rad - k, x1, y0 + rad);
    pb.line_to(x1, y1 - rad);
    pb.cubic_to(x1, y1 - rad + k, x1 - rad + k, y1, x1 - rad, y1);
    pb.line_to(x0 + rad, y1);
    pb.cubic_to(x0 + rad - k, y1, x0, y1 - rad + k, x0, y1 - rad);
    pb.line_to(x0, y0 + rad);
    pb.cubic_to(x0, y0 + rad - k, x0 + rad - k, y0, x0 + rad, y0);
    pb.close();
    pb.finish()
}

/// Compact light chrome used by the annotation bar / pin menu (not theme-tinted).
const CHROME_BG: Rgba = Rgba {
    r: 252,
    g: 252,
    b: 253,
    a: 255,
};
const CHROME_BORDER: Rgba = Rgba {
    r: 206,
    g: 208,
    b: 214,
    a: 255,
};
const CHROME_HOVER: Rgba = Rgba {
    r: 236,
    g: 241,
    b: 247,
    a: 255,
};
const CHROME_SELECTED: Rgba = Rgba {
    r: 53,
    g: 148,
    b: 242,
    a: 255,
};
const CHROME_SELECTED_EDGE: Rgba = Rgba {
    r: 40,
    g: 122,
    b: 210,
    a: 255,
};
const CHROME_CHIP: Rgba = Rgba {
    r: 243,
    g: 244,
    b: 247,
    a: 255,
};
const CHROME_CHIP_ON: Rgba = Rgba {
    r: 61,
    g: 143,
    b: 209,
    a: 255,
};
const CHROME_TEXT: Rgba = Rgba {
    r: 36,
    g: 36,
    b: 40,
    a: 255,
};
const CHROME_SEP: Rgba = Rgba {
    r: 220,
    g: 222,
    b: 226,
    a: 255,
};
const CHROME_ICON: Rgba = Rgba {
    r: 48,
    g: 50,
    b: 56,
    a: 255,
};

fn paint_soft_shadow(pm: &mut Pixmap, r: Rect, radius: f32, steps: i32) {
    for i in (1..=steps).rev() {
        let t = i as f32 / steps as f32;
        let a = ((20.0 * t * t) as u8).max(2);
        let grow = i as f32 * 0.55;
        let rr = Rect::new(
            r.x - grow * 0.45,
            r.y + i as f32 * 0.35,
            r.w + grow * 0.9,
            r.h + grow * 0.55,
        );
        fill_rounded_rect(pm, rr, radius + i as f32 * 0.25, Rgba::rgba(0, 0, 0, a));
    }
}

fn paint_chrome_surface(pm: &mut Pixmap, r: Rect, radius: f32) {
    paint_soft_shadow(pm, r, radius, 5);
    fill_rounded_rect(pm, r, radius, CHROME_BG);
    stroke_rounded_rect(pm, r, radius, CHROME_BORDER, 1.0);
}

fn hover_at(cursor: Option<Point>, r: Rect) -> bool {
    cursor.map(|p| r.contains(p)).unwrap_or(false)
}

pub fn paint_check_tick(pm: &mut Pixmap, x: f32, y: f32, on: bool) {
    if !on {
        stroke_rounded_rect(
            pm,
            Rect::new(x, y, 12.0, 12.0),
            2.0,
            Rgba::rgb(180, 182, 188),
            1.0,
        );
        return;
    }
    fill_rounded_rect(pm, Rect::new(x, y, 12.0, 12.0), 2.5, CHROME_CHIP_ON);
    let mut pb = PathBuilder::new();
    pb.move_to(x + 2.5, y + 6.2);
    pb.line_to(x + 5.0, y + 9.0);
    pb.line_to(x + 9.6, y + 3.2);
    if let Some(path) = pb.finish() {
        pm.stroke_path(
            &path,
            &paint(Rgba::WHITE),
            &Stroke {
                width: 1.6,
                line_cap: tiny_skia::LineCap::Round,
                line_join: tiny_skia::LineJoin::Round,
                ..Stroke::default()
            },
            Transform::identity(),
            None,
        );
    }
}

pub fn stroke_rect(pm: &mut Pixmap, r: Rect, c: Rgba, width: f32) {
    if r.is_empty() {
        return;
    }
    if let Some(sr) = tiny_skia::Rect::from_xywh(r.x, r.y, r.w.max(0.5), r.h.max(0.5)) {
        pm.stroke_path(
            &PathBuilder::from_rect(sr),
            &paint(c),
            &Stroke {
                width,
                ..Stroke::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// Figma-style nub: white disc, colored rim, eight around the box.
pub fn paint_handle_dot(pm: &mut Pixmap, c: Point, stroke: Rgba) {
    let s = HANDLE_SIZE;
    let r = Rect::new(c.x - s * 0.5, c.y - s * 0.5, s, s);
    fill_rounded_rect(pm, r, s * 0.5, Rgba::WHITE);
    stroke_rounded_rect(pm, r, s * 0.5, stroke, 1.25);
}

fn paint_snip_ring(pm: &mut Pixmap, r: Rect, ring: Rgba, edge: Rgba) {
    // Opaque pixel borders stay *inside* the hole. AA `stroke_rect` hung onto
    // the dim mask (stain) and sat under the loupe (the ring vanished).
    if r.is_empty() {
        return;
    }
    let x = r.x.floor() as i32;
    let y = r.y.floor() as i32;
    let w = (r.right().ceil() as i32 - x).max(1);
    let h = (r.bottom().ceil() as i32 - y).max(1);
    if w < 8 || h < 8 {
        fill_px_border(pm, x, y, w, h, 1, edge);
        fill_px_border(pm, x, y, w, h, 1, ring);
        return;
    }
    fill_px_border(pm, x + 1, y + 1, w - 2, h - 2, 1, edge);
    fill_px_border(pm, x + 2, y + 2, w - 4, h - 4, 2, ring);
}

/// Four thin dirty tiles for the hole ring. Unioning the hole AABB with the
/// loupe would recopy a 1700×900 window on every mouse pixel.
fn ring_strips(hole: Rect) -> [Rect; 4] {
    let r = hole.adjusted(0.0, 0.0, -1.0, -1.0);
    let t = 6.0;
    [
        Rect::new(r.x, r.y, r.w, t),
        Rect::new(r.x, r.bottom() - t, r.w, t),
        Rect::new(r.x, r.y, t, r.h),
        Rect::new(r.right() - t, r.y, t, r.h),
    ]
}

fn clip_rect(r: Rect, bounds: Rect) -> Rect {
    r.intersect(bounds)
}

/// Pixel-aligned dirty rect so hover-leave copies cover the same samples as `dimmed`.
fn align_dirty(r: Rect) -> Rect {
    if r.is_empty() {
        return Rect::EMPTY;
    }
    let x0 = r.x.floor();
    let y0 = r.y.floor();
    let x1 = r.right().ceil();
    let y1 = r.bottom().ceil();
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

fn copy_rect(dst: &mut Pixmap, src: &Pixmap, r: Rect) {
    let bounds = Rect::new(0.0, 0.0, dst.width() as f32, dst.height() as f32).intersect(Rect::new(
        0.0,
        0.0,
        src.width() as f32,
        src.height() as f32,
    ));
    let r = clip_rect(r, bounds);
    if r.is_empty() {
        return;
    }
    let x0 = r.x.floor().max(0.0) as u32;
    let y0 = r.y.floor().max(0.0) as u32;
    let x1 = r
        .right()
        .ceil()
        .min(dst.width() as f32)
        .min(src.width() as f32) as u32;
    let y1 = r
        .bottom()
        .ceil()
        .min(dst.height() as f32)
        .min(src.height() as f32) as u32;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let dw = dst.width() as usize;
    let sw = src.width() as usize;
    let src_data = src.data();
    let dst_data = dst.data_mut();
    let row_bytes = (x1 - x0) as usize * 4;
    for y in y0..y1 {
        let s = (y as usize * sw + x0 as usize) * 4;
        let d = (y as usize * dw + x0 as usize) * 4;
        dst_data[d..d + row_bytes].copy_from_slice(&src_data[s..s + row_bytes]);
    }
}

pub fn blit_raster(pm: &mut Pixmap, img: &Raster, dest: Rect) {
    blit_raster_ex(pm, img, dest, false);
}

pub fn blit_raster_ex(pm: &mut Pixmap, img: &Raster, dest: Rect, smooth: bool) {
    let dx = dest.x.round() as i32;
    let dy = dest.y.round() as i32;
    let dw = dest.w.round().max(0.0) as u32;
    let dh = dest.h.round().max(0.0) as u32;
    if dw == img.width && dh == img.height {
        blit_raster_copy(pm, img, dx, dy);
        return;
    }
    let Some(src) = img.to_skia() else {
        return;
    };
    let sx = if dest.w > 0.0 {
        dest.w / img.width as f32
    } else {
        1.0
    };
    let sy = if dest.h > 0.0 {
        dest.h / img.height as f32
    } else {
        1.0
    };
    let t = Transform::from_translate(dest.x, dest.y).pre_scale(sx, sy);
    let mut paint = tiny_skia::PixmapPaint::default();
    paint.quality = if smooth {
        tiny_skia::FilterQuality::Bilinear
    } else {
        tiny_skia::FilterQuality::Nearest
    };
    pm.draw_pixmap(0, 0, src.as_ref(), &paint, t, None);
}

fn blit_raster_copy(pm: &mut Pixmap, img: &Raster, x: i32, y: i32) {
    let pw = pm.width() as i32;
    let ph = pm.height() as i32;
    let iw = img.width as i32;
    let ih = img.height as i32;
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + iw).min(pw);
    let y1 = (y + ih).min(ph);
    if x0 >= x1 || y0 >= y1 {
        return;
    };
    let dst = pm.data_mut();
    let sw = pw as usize * 4;
    let src_w = img.width as usize * 4;
    let row = (x1 - x0) as usize * 4;
    let src_x = (x0 - x) as usize * 4;
    for yy in y0..y1 {
        let so = ((yy - y) as usize) * src_w + src_x;
        let doff = (yy as usize) * sw + (x0 as usize) * 4;
        if so + row <= img.pixels.len() && doff + row <= dst.len() {
            dst[doff..doff + row].copy_from_slice(&img.pixels[so..so + row]);
        }
    }
}

/// Persistent overlay compositor: cached capture + dirty GDI present.
pub struct OverlayFront {
    shot: Pixmap,
    /// Frozen dim. Restores copy this — re-dimming from `shot` on each
    /// dirty tile is why moving the mouse washed the rest of the freeze.
    dimmed: Pixmap,
    frame: Pixmap,
    #[cfg(windows)]
    gdi: GdiBits,
    dim: u8,
    last_hole: Rect,
    last_chrome: Vec<Rect>,
    last_size_label: Rect,
    last_mag: Rect,
    primed: bool,
    effect_shapes: Vec<pinshot_core::annotation::Shape>,
    effect_frame: Option<Pixmap>,
}

#[cfg(windows)]
struct GdiBits {
    hdc: HDC,
    hbmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
    stride: i32,
}

#[cfg(windows)]
impl GdiBits {
    fn new(w: u32, h: u32) -> Result<Self, String> {
        unsafe {
            let hdc_screen = GetDC(HWND::default());
            let hdc = CreateCompatibleDC(hdc_screen);
            if hdc.is_invalid() {
                ReleaseDC(HWND::default(), hdc_screen);
                return Err("CreateCompatibleDC failed".into());
            }
            let mut bits: *mut c_void = std::ptr::null_mut();
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                bmiColors: [RGBQUAD::default()],
            };
            let hbmp = match CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(h) if !bits.is_null() => h,
                _ => {
                    let _ = DeleteDC(hdc);
                    ReleaseDC(HWND::default(), hdc_screen);
                    return Err("CreateDIBSection failed".into());
                }
            };
            let old = SelectObject(hdc, HGDIOBJ(hbmp.0));
            ReleaseDC(HWND::default(), hdc_screen);
            Ok(Self {
                hdc,
                hbmp,
                old,
                bits: bits as *mut u8,
                w: w as i32,
                h: h as i32,
                stride: w as i32 * 4,
            })
        }
    }

    fn write_rect(&self, pm: &Pixmap, r: Rect) {
        let bounds = Rect::new(0.0, 0.0, self.w as f32, self.h as f32).intersect(Rect::new(
            0.0,
            0.0,
            pm.width() as f32,
            pm.height() as f32,
        ));
        let r = clip_rect(r, bounds);
        if r.is_empty() || self.bits.is_null() {
            return;
        }
        let x0 = r.x.floor().max(0.0) as i32;
        let y0 = r.y.floor().max(0.0) as i32;
        let x1 = r.right().ceil().min(self.w as f32) as i32;
        let y1 = r.bottom().ceil().min(self.h as f32) as i32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let pw = pm.width() as usize;
        let src = pm.data();
        // The first frame is the whole 4160x1400 desktop. Keep the hot
        // full-frame path in packed u32 form so LLVM can vectorize the
        // RGBA -> BGRA swap; the small dirty-rect path below stays simple.
        if x0 == 0 && y0 == 0 && x1 == self.w && y1 == self.h && x1 == pm.width() as i32 {
            let pixels = (self.w as usize) * (self.h as usize);
            let src32 = src.as_ptr().cast::<u32>();
            let dst32 = self.bits.cast::<u32>();
            for i in 0..pixels {
                let p = unsafe { *src32.add(i) };
                unsafe {
                    *dst32.add(i) = (p & 0xff00_ff00)
                        | ((p & 0x00ff_0000) >> 16)
                        | ((p & 0x0000_00ff) << 16);
                }
            }
            return;
        }
        unsafe {
            for y in y0..y1 {
                let srow = (y as usize * pw + x0 as usize) * 4;
                let drow = (y * self.stride + x0 * 4) as isize;
                let dst = self.bits.offset(drow);
                let cols = (x1 - x0) as usize;
                for i in 0..cols {
                    // tiny-skia RGBA premul → GDI BGRA
                    *dst.add(i * 4) = src[srow + i * 4 + 2];
                    *dst.add(i * 4 + 1) = src[srow + i * 4 + 1];
                    *dst.add(i * 4 + 2) = src[srow + i * 4];
                    *dst.add(i * 4 + 3) = 255;
                }
            }
        }
    }

    fn blit_rect(&self, hwnd: HWND, r: Rect) {
        let r = clip_rect(r, Rect::new(0.0, 0.0, self.w as f32, self.h as f32));
        if r.is_empty() {
            return;
        }
        let x = r.x.floor() as i32;
        let y = r.y.floor() as i32;
        let w = (r.right().ceil() as i32 - x).max(1);
        let h = (r.bottom().ceil() as i32 - y).max(1);
        unsafe {
            let hdc_wnd = GetDC(hwnd);
            if !hdc_wnd.is_invalid() {
                let _ = BitBlt(hdc_wnd, x, y, w, h, self.hdc, x, y, SRCCOPY);
                ReleaseDC(hwnd, hdc_wnd);
            }
        }
    }
}

#[cfg(windows)]
impl Drop for GdiBits {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.hbmp.0));
            let _ = DeleteDC(self.hdc);
        }
    }
}

fn dim_pixmap(src: &Pixmap, dim: u8) -> Option<Pixmap> {
    let mut frame = Pixmap::new(src.width(), src.height())?;
    dim_rect(
        &mut frame,
        src,
        Rect::new(0.0, 0.0, src.width() as f32, src.height() as f32),
        dim,
    );
    Some(frame)
}

/// Darken `r` of `src` into `dst` with the same multiply as [`dim_pixmap`].
/// Call only on undimmed pixels (shot + ink). Never on already-dimmed tiles.
fn dim_rect(dst: &mut Pixmap, src: &Pixmap, r: Rect, dim: u8) {
    let bounds = Rect::new(0.0, 0.0, dst.width() as f32, dst.height() as f32).intersect(Rect::new(
        0.0,
        0.0,
        src.width() as f32,
        src.height() as f32,
    ));
    let r = clip_rect(align_dirty(r), bounds);
    if r.is_empty() {
        return;
    }
    let k = 255u16.saturating_sub(dim as u16);
    let x0 = r.x as u32;
    let y0 = r.y as u32;
    let x1 = r.right() as u32;
    let y1 = r.bottom() as u32;
    let sw = src.width() as usize;
    let dw = dst.width() as usize;
    let s = src.data();
    let d = dst.data_mut();
    for y in y0..y1 {
        let srow = (y as usize * sw + x0 as usize) * 4;
        let drow = (y as usize * dw + x0 as usize) * 4;
        let cols = (x1 - x0) as usize;
        for i in 0..cols {
            let si = srow + i * 4;
            let di = drow + i * 4;
            d[di] = ((s[si] as u16 * k) / 255) as u8;
            d[di + 1] = ((s[si + 1] as u16 * k) / 255) as u8;
            d[di + 2] = ((s[si + 2] as u16 * k) / 255) as u8;
            d[di + 3] = s[si + 3];
        }
    }
}

fn dim_rect_in_place(pm: &mut Pixmap, r: Rect, dim: u8) {
    let bounds = Rect::new(0.0, 0.0, pm.width() as f32, pm.height() as f32);
    let r = clip_rect(align_dirty(r), bounds);
    if r.is_empty() {
        return;
    }
    let k = 255u16.saturating_sub(dim as u16);
    let x0 = r.x as u32;
    let y0 = r.y as u32;
    let x1 = r.right() as u32;
    let y1 = r.bottom() as u32;
    let w = pm.width() as usize;
    let d = pm.data_mut();
    for y in y0..y1 {
        let row = (y as usize * w + x0 as usize) * 4;
        let cols = (x1 - x0) as usize;
        for i in 0..cols {
            let di = row + i * 4;
            d[di] = ((d[di] as u16 * k) / 255) as u8;
            d[di + 1] = ((d[di + 1] as u16 * k) / 255) as u8;
            d[di + 2] = ((d[di + 2] as u16 * k) / 255) as u8;
        }
    }
}

/// Four-quad dim: pixels in `dirty` except `hole` are darkened in place.
/// `pm` must still be undimmed (shot + ink) inside `dirty`.
fn dim_outside_hole(pm: &mut Pixmap, dirty: Rect, hole: Rect, dim: u8) {
    let bounds = Rect::new(0.0, 0.0, pm.width() as f32, pm.height() as f32);
    let dirty = clip_rect(align_dirty(dirty), bounds);
    if dirty.is_empty() {
        return;
    }
    let hole = clip_rect(align_dirty(hole), bounds);
    let cover = hole.intersect(dirty);
    if cover.is_empty() {
        dim_rect_in_place(pm, dirty, dim);
        return;
    }
    let top_h = cover.y - dirty.y;
    if top_h > 0.0 {
        dim_rect_in_place(pm, Rect::new(dirty.x, dirty.y, dirty.w, top_h), dim);
    }
    let bot_y = cover.bottom();
    if bot_y < dirty.bottom() {
        dim_rect_in_place(
            pm,
            Rect::new(dirty.x, bot_y, dirty.w, dirty.bottom() - bot_y),
            dim,
        );
    }
    let mid_h = cover.h;
    if mid_h > 0.0 {
        let left_w = cover.x - dirty.x;
        if left_w > 0.0 {
            dim_rect_in_place(pm, Rect::new(dirty.x, cover.y, left_w, mid_h), dim);
        }
        let right_w = dirty.right() - cover.right();
        if right_w > 0.0 {
            dim_rect_in_place(pm, Rect::new(cover.right(), cover.y, right_w, mid_h), dim);
        }
    }
}

impl OverlayFront {
    pub fn new(shot: &Raster, dim: u8) -> Result<Self, String> {
        let t0 = std::time::Instant::now();
        let shot_pm = shot.to_skia().ok_or("capture pixmap")?;
        crate::logutil::ms("overlay to_skia", t0);
        let t_dim = std::time::Instant::now();
        let dimmed = dim_pixmap(&shot_pm, dim).ok_or("dim pixmap")?;
        crate::logutil::ms("overlay dim", t_dim);
        let frame = dimmed.clone();
        #[cfg(windows)]
        let t_gdi = std::time::Instant::now();
        #[cfg(windows)]
        let gdi = GdiBits::new(frame.width(), frame.height())?;
        #[cfg(windows)]
        crate::logutil::ms("overlay gdi dib", t_gdi);
        Ok(Self {
            shot: shot_pm,
            dimmed,
            frame,
            #[cfg(windows)]
            gdi,
            dim,
            last_hole: Rect::EMPTY,
            last_chrome: Vec::new(),
            last_size_label: Rect::EMPTY,
            last_mag: Rect::EMPTY,
            primed: false,
            effect_shapes: Vec::new(),
            effect_frame: None,
        })
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.frame
    }

    fn bounds(&self) -> Rect {
        Rect::new(
            0.0,
            0.0,
            self.frame.width() as f32,
            self.frame.height() as f32,
        )
    }

    /// Swap the frozen desktop (history replay). Same virtual size expected.
    pub fn replace_shot(&mut self, shot: &Raster, hwnd: NativeWin) -> Result<(), String> {
        let _hwnd = hwnd;
        let shot_pm = shot.to_skia().ok_or("capture pixmap")?;
        if shot_pm.width() != self.frame.width() || shot_pm.height() != self.frame.height() {
            *self = Self::new(shot, self.dim)?;
            return Ok(());
        }
        self.shot = shot_pm;
        self.dimmed = dim_pixmap(&self.shot, self.dim).ok_or("dim pixmap")?;
        self.frame = self.dimmed.clone();
        self.last_hole = Rect::EMPTY;
        self.last_chrome.clear();
        self.last_size_label = Rect::EMPTY;
        self.last_mag = Rect::EMPTY;
        self.primed = false;
        #[cfg(windows)]
        {
            let bounds = self.bounds();
            self.gdi.write_rect(&self.frame, bounds);
            self.gdi.blit_rect(hwnd, bounds);
        }
        Ok(())
    }

    fn restore(&mut self, r: Rect) {
        let r = clip_rect(align_dirty(r), self.bounds());
        if r.is_empty() {
            return;
        }
        copy_rect(&mut self.frame, &self.dimmed, r);
    }

    pub fn paint(
        &mut self,
        hwnd: NativeWin,
        sel: &Selection,
        hover: Rect,
        cursor: Point,
        dpr: f32,
        hex: bool,
        mag_on: bool,
        annot: &Annotator,
        toolbar: Option<&Toolbar>,
        toast: Option<&str>,
        show_cursor: bool,
        shot: &Raster,
        confirm_dont: Option<bool>,
        picker: Option<&pinshot_core::ColorPicker>,
    ) {
        let first_paint = !self.primed;
        let t_pixels = std::time::Instant::now();
        let t_setup = std::time::Instant::now();
        let t_bounds = std::time::Instant::now();
        let bounds = self.bounds();
        if first_paint {
            crate::logutil::ms("overlay paint setup bounds", t_bounds);
        }
        let t_hole = std::time::Instant::now();
        let hole = if sel.has_sel() {
            sel.normalized()
        } else if sel.state == SelState::Idle {
            hover
        } else {
            sel.normalized()
        };
        let hole = clip_rect(hole, bounds);
        if first_paint {
            crate::logutil::ms("overlay paint setup hole", t_hole);
        }
        let fw = self.frame.width();
        let fh = self.frame.height();
        let t_label = std::time::Instant::now();
        let size_text = if hole.is_empty() {
            String::new()
        } else {
            Rect::new(hole.x * dpr, hole.y * dpr, hole.w * dpr, hole.h * dpr).size_label()
        };
        let size_label = if hole.is_empty() {
            Rect::EMPTY
        } else {
            size_label_dirty(hole, &size_text, fw, fh).intersect(bounds)
        };
        if first_paint {
            crate::logutil::ms("overlay paint setup label", t_label);
        }
        let t_pad = std::time::Instant::now();
        let hole_pad = hole.inflated(10.0).union(size_label).intersect(bounds);
        if first_paint {
            crate::logutil::ms("overlay paint setup pad", t_pad);
        }
        if first_paint {
            crate::logutil::ms("overlay paint setup", t_setup);
        }
        let t_mag = std::time::Instant::now();
        let mag_layout = if mag_on {
            let (color, phys) = pinshot_core::sample_color(shot, cursor, dpr);
            Some(layout_magnifier(cursor, bounds, color, phys))
        } else {
            None
        };
        if first_paint {
            crate::logutil::ms("overlay paint magnifier", t_mag);
        }
        let mag_dirty = mag_layout
            .map(|m| m.grid.union(m.info).inflated(14.0))
            .unwrap_or(Rect::EMPTY);

        // Incremental restore. If the hover/sel hole did not change, do
        // *not* recopy a 1700×900 window on every mouse pixel — only the
        // loupe and chrome. First frame still copies all.
        let hole_changed = !self.primed || self.last_hole != hole;
        let mut restore_r = if !self.primed {
            bounds
        } else {
            let mut d = self.last_mag.union(mag_dirty);
            for c in &self.last_chrome {
                d = d.union(*c);
            }
            d = d.union(self.last_size_label).union(size_label);
            if hole_changed {
                d = d.union(self.last_hole.inflated(12.0)).union(hole_pad);
            }
            d.intersect(bounds)
        };
        if restore_r.area() > bounds.area() * 0.55 {
            restore_r = bounds;
        }
        let t_shapes = std::time::Instant::now();
        let shapes = annot.visible_shapes();
        let mut annot_dirty = Rect::EMPTY;
        for sh in &shapes {
            let pad = sh.style.width.max(8) as f32 + 8.0;
            annot_dirty = annot_dirty.union(sh.bounds().inflated(pad));
        }
        annot_dirty = annot_dirty.intersect(bounds);
        if first_paint {
            crate::logutil::ms("overlay paint shapes", t_shapes);
        }
        if !annot_dirty.is_empty() {
            restore_r = restore_r.union(annot_dirty).intersect(bounds);
            if restore_r.area() > bounds.area() * 0.55 {
                restore_r = bounds;
            }
        }

        // Hole interior: undimmed capture. Ink is painted on the freeze first,
        // then the dim covers everything except the hole (参考工具).
        let hole_refresh = if hole.is_empty() {
            Rect::EMPTY
        } else if hole_changed {
            hole
        } else {
            hole.intersect(restore_r)
        };
        if shapes.is_empty() {
            let t_restore = std::time::Instant::now();
            self.restore(restore_r);
            if first_paint {
                crate::logutil::ms("overlay paint restore", t_restore);
            }
            if !hole_refresh.is_empty() {
                let t_hole = std::time::Instant::now();
                copy_rect(&mut self.frame, &self.shot, hole_refresh);
                if first_paint {
                    crate::logutil::ms("overlay paint hole", t_hole);
                }
            }
        } else {
            copy_rect(&mut self.frame, &self.shot, restore_r);
            if hole_changed && !hole.is_empty() {
                copy_rect(&mut self.frame, &self.shot, hole);
            }
            let needs_fx = shapes
                .iter()
                .any(|s| matches!(s.kind, Tool::Mosaic | Tool::Eraser));
            if needs_fx {
                if self.effect_frame.is_none() || self.effect_shapes != shapes {
                    self.effect_frame = compose(shot, &shapes).to_skia();
                    self.effect_shapes = shapes.clone();
                }
                if let Some(src) = self.effect_frame.as_ref() {
                    let ink = restore_r.union(hole).intersect(bounds);
                    copy_rect(&mut self.frame, src, ink);
                }
            } else {
                paint_shapes(&mut self.frame, &shapes);
            }
            dim_outside_hole(&mut self.frame, restore_r, hole, self.dim);
        }
        if first_paint {
            crate::logutil::ms("overlay paint pixels", t_pixels);
        }

        let t_chrome = std::time::Instant::now();
        let mut chrome: Vec<Rect> = Vec::new();
        for sh in &annot.visible_shapes() {
            let pad = sh.style.width.max(8) as f32 + 8.0;
            chrome.push(sh.bounds().inflated(pad).intersect(bounds));
        }

        // Python / 参考工具: handles only for `selected`. Text commit leaves
        // width_target but must not keep a blue box and 8 nubs.
        if let Some(i) = annot.selected {
            if let Some(sh) = annot.shapes.get(i) {
                let handles = annot.selected_handle_points();
                // Lines are a segment between endpoints, not an AABB with a
                // diagonal. Skip the blue box so dragging does not look like
                // rubber-banding a selection rect.
                if !sh.is_line_stroke() {
                    let hb = sh.bounds().adjusted(-2.0, -2.0, 2.0, 2.0);
                    stroke_rect(&mut self.frame, hb, Rgba::BLUE, 1.0);
                    chrome.push(hb.inflated(HANDLE_SIZE));
                }
                for (_, c) in handles {
                    paint_handle_dot(&mut self.frame, c, Rgba::SNIP_RING);
                    chrome.push(
                        Rect::new(
                            c.x - HANDLE_SIZE / 2.0,
                            c.y - HANDLE_SIZE / 2.0,
                            HANDLE_SIZE,
                            HANDLE_SIZE,
                        )
                        .inflated(2.0),
                    );
                }
                if let Some(r) = annot.selected_text_delete_rect() {
                    paint_text_delete(&mut self.frame, r);
                    chrome.push(r.inflated(2.0));
                }
            }
        }

        if let Some(mag) = mag_layout {
            chrome.push(paint_magnifier(
                &mut self.frame,
                shot,
                cursor,
                dpr,
                hex,
                mag,
            ));
        }

        // Ring after the loupe so the preselect / sel border is never buried
        // under the 180px grid. Four thin strips keep GDI off the hole memcpy.
        if !hole.is_empty() {
            paint_snip_ring(
                &mut self.frame,
                hole.adjusted(0.0, 0.0, -1.0, -1.0),
                Rgba::SNIP_RING,
                Rgba::SNIP_RING_EDGE,
            );
            if sel.has_sel() && annot.tool.is_none() && annot.selected.is_none() {
                for (_, c) in handle_points(hole) {
                    paint_handle_dot(&mut self.frame, c, Rgba::SNIP_RING);
                }
            }
            chrome.push(paint_size_label(&mut self.frame, hole, &size_text, fw, fh));
        }

        if let Some(tb) = toolbar {
            if tb.visible {
                chrome.push(paint_toolbar(&mut self.frame, tb, Some(cursor)));
            }
        }

        if let Some(t) = toast {
            chrome.push(paint_toast(&mut self.frame, t, fw, fh));
        }

        if show_cursor {
            chrome.push(paint_pointer(&mut self.frame, cursor));
        }
        if confirm_dont.is_none() && picker.is_none() {
            let mut hint = annot.cursor_hint(Point::new(cursor.x * dpr, cursor.y * dpr));
            hint.over_chrome = toolbar.is_some_and(|t| t.contains(cursor));
            if let Some(width) = crate::cursor::brush_width(annot, pinshot_core::cursor::cursor_for(hint)) {
                chrome.push(crate::cursor::paint_brush(&mut self.frame, cursor, width / dpr.max(0.1)));
            }
        }

        if let Some(dont) = confirm_dont {
            chrome.push(paint_confirm_abort(&mut self.frame, fw, fh, dont));
        }

        if let Some(picker) = picker {
            chrome.push(paint_color_picker(&mut self.frame, fw, fh, picker));
        }

        if let Some(ed) = &annot.text_edit {
            if let Some(r) = annot.text_edit_rect() {
                fill_rect(&mut self.frame, r, Rgba::rgba(255, 255, 255, 90));
                let st = annot.text_edit_style();
                stroke_rect(&mut self.frame, r, st.color, 1.0);
                pinshot_core::font::blit_pixmap_ex(
                    &mut self.frame,
                    &ed.text,
                    r.x + 2.0,
                    r.y + 2.0,
                    st.font_size.max(8) as f32,
                    st.color,
                    st.outline,
                    &st.family,
                    st.bold,
                    st.italic,
                );
                if annot.caret_visible() {
                    let caret_x = r.x + 2.0 + annot.caret_x();
                    let ch = (r.h - 4.0).max(8.0);
                    // Hairline plus a light edge so a dark caret shows on dark shots
                    // (it used to appear only after the user picked a bright colour).
                    fill_rect(
                        &mut self.frame,
                        Rect::new(caret_x - 1.0, r.y + 1.0, 4.0, ch + 2.0),
                        Rgba::rgba(255, 255, 255, 200),
                    );
                    fill_rect(
                        &mut self.frame,
                        Rect::new(caret_x, r.y + 2.0, 2.0, ch),
                        st.color,
                    );
                }
                chrome.push(r.inflated(2.0));
            }
        }
        if first_paint {
            crate::logutil::ms("overlay paint chrome", t_chrome);
        }

        // Write only the dirty tiles into the persistent DIB, then BitBlt
        // the whole DIB. Partial BitBlt of the loupe alone is what stained
        // the freeze; a full blit of an incrementally-updated DIB does not.
        #[cfg(windows)]
        {
            let t_gdi = std::time::Instant::now();
            // Tile writes. Unioning loupe + size chip + ring strips becomes
            // the hole AABB and recopies a maximized window on every move.
            if !self.primed || restore_r.area() > bounds.area() * 0.55 {
                self.gdi.write_rect(&self.frame, bounds);
            } else {
                self.gdi.write_rect(&self.frame, restore_r);
                for c in &chrome {
                    self.gdi.write_rect(&self.frame, *c);
                }
                if !hole.is_empty() {
                    for strip in ring_strips(hole) {
                        self.gdi.write_rect(&self.frame, strip.intersect(bounds));
                    }
                }
            }
            // Always blit the whole DIB. Partial BitBlt of the loupe is what
            // left the staircase shadow on the freeze; the hitch was the
            // CPU hole memcpy, not this SRCCOPY.
            self.gdi.blit_rect(hwnd, bounds);
            if first_paint {
                crate::logutil::ms("overlay paint gdi", t_gdi);
            }
        }
        self.last_hole = hole;
        self.last_chrome = chrome;
        self.last_size_label = size_label;
        self.last_mag = mag_dirty;
        self.primed = true;
        let _ = FillRule::Winding;
        let _ = hwnd;
    }

    /// Replay the already-composited DIB (WM_PAINT / uncover). No software rebuild.
    pub fn present_full(&self, hwnd: NativeWin) {
        #[cfg(windows)]
        if self.primed {
            self.gdi.blit_rect(hwnd, self.bounds());
        }
        let _ = hwnd;
    }
}

fn fill_px_border(pm: &mut Pixmap, x: i32, y: i32, w: i32, h: i32, t: i32, c: Rgba) {
    if t <= 0 || w <= 0 || h <= 0 {
        return;
    }
    fill_px(pm, x, y, w, t, c);
    fill_px(pm, x, y + h - t, w, t, c);
    fill_px(pm, x, y, t, h, c);
    fill_px(pm, x + w - t, y, t, h, c);
}

fn fill_px(pm: &mut Pixmap, x: i32, y: i32, w: i32, h: i32, c: Rgba) {
    let pw = pm.width() as i32;
    let ph = pm.height() as i32;
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(pw);
    let y1 = (y + h).min(ph);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let a = c.a as u16;
    let px = PremultipliedColorU8::from_rgba(
        ((c.r as u16 * a) / 255) as u8,
        ((c.g as u16 * a) / 255) as u8,
        ((c.b as u16 * a) / 255) as u8,
        c.a,
    )
    .unwrap_or(PremultipliedColorU8::TRANSPARENT);
    let stride = pw as usize;
    let pixels = pm.pixels_mut();
    for yy in y0..y1 {
        let row = yy as usize * stride;
        for xx in x0..x1 {
            pixels[row + xx as usize] = px;
        }
    }
}

/// Chip that sits just above the hole (or just inside if there is no room).
fn size_label_box(hole: Rect, label: &str, ow: u32, oh: u32) -> Rect {
    let (tw, th) = crate::text::measure(label, 12, false);
    let lw = (tw as f32 + 12.0).max(56.0);
    let lh = 20.0f32.max(th as f32);
    let ly = if hole.y >= lh + 2.0 {
        hole.y - lh - 2.0
    } else {
        hole.y + 2.0
    };
    let lx = if hole.x + lw < ow as f32 {
        hole.x
    } else {
        (ow as f32 - lw).max(0.0)
    };
    let _ = oh;
    Rect::new(lx, ly, lw, lh)
}

fn size_label_dirty(hole: Rect, label: &str, ow: u32, oh: u32) -> Rect {
    size_label_box(hole, label, ow, oh).inflated(20.0)
}

fn paint_size_label(pm: &mut Pixmap, hole: Rect, label: &str, ow: u32, oh: u32) -> Rect {
    let boxr = size_label_box(hole, label, ow, oh);
    fill_rounded_rect(pm, boxr, 4.0, Rgba::rgba(0, 0, 0, 170));
    blit_into(
        pm,
        label,
        boxr.x as i32 + 6,
        boxr.y as i32 + 3,
        12,
        Rgba::WHITE,
        false,
    );
    // blit_into_pm starts at (x-2,y-2) and the 2× atlas can hang past the
    // chip. OverlayFront tracks this dirty rect across frames.
    size_label_dirty(hole, label, ow, oh)
}

fn paint_magnifier(
    pm: &mut Pixmap,
    shot: &Raster,
    cursor: Point,
    dpr: f32,
    hex: bool,
    mag: Magnifier,
) -> Rect {
    let gx = mag.grid.x.floor() as i32;
    let gy = mag.grid.y.floor() as i32;
    let n = MAG_N;
    let z = MAG_ZOOM as i32;
    fill_px(pm, gx, gy, n * z, n * z, Rgba::BLACK);
    let cx = (cursor.x * dpr).round() as i32;
    let cy = (cursor.y * dpr).round() as i32;
    for j in 0..n {
        for i in 0..n {
            let c = shot.get_clamped(cx + i - n / 2, cy + j - n / 2);
            fill_px(pm, gx + i * z, gy + j * z, z, z, c);
        }
    }
    // Opaque 1px chrome — AA `stroke_rect` / `fill_rect` SrcOver onto the
    // already-dimmed freeze leaves a darker halo that dirty-rects miss.
    fill_px_border(pm, gx, gy, n * z, n * z, 1, Rgba::WHITE);
    // 参考工具: blue crosshair through the whole preview, white box on the
    // centre source pixel.
    let mid = n / 2;
    let cell_x = gx + mid * z;
    let cell_y = gy + mid * z;
    let cross = Rgba::rgb(30, 144, 255);
    let hy = cell_y + z / 2 - 1;
    let hx = cell_x + z / 2 - 1;
    fill_px(pm, gx, hy, n * z, 2, cross);
    fill_px(pm, hx, gy, 2, n * z, cross);
    fill_px_border(pm, cell_x, cell_y, z, z, 1, Rgba::WHITE);

    let ix = mag.info.x.floor() as i32;
    let iy = mag.info.y.floor() as i32;
    let iw = mag.info.w.round().max(1.0) as u32;
    let ih = mag.info.h.round().max(1.0) as u32;
    if let Some(mut panel) = Pixmap::new(iw, ih) {
        fill_px(
            &mut panel,
            0,
            0,
            iw as i32,
            ih as i32,
            Rgba::rgb(22, 22, 22),
        );
        let local = Rect::new(0.0, 0.0, iw as f32, ih as f32);
        let hint = MAG_HINT_PX;
        let coord = format!("({}, {})", mag.phys.x as i32, mag.phys.y as i32);
        blit_centered(&mut panel, &coord, local, 6.0, hint, Rgba::WHITE);
        let hex_s = mag.color.format(hex);
        let (hex_w, _) = pinshot_core::font::measure_chrome(&hex_s, hint as f32);
        let sw = 12.0;
        let gap = 6.0;
        let row_w = sw + gap + hex_w;
        let row_x = ((iw as f32 - row_w) * 0.5).round();
        fill_px(
            &mut panel,
            row_x as i32,
            28,
            sw as i32,
            sw as i32,
            mag.color,
        );
        blit_into(
            &mut panel,
            &hex_s,
            (row_x + sw + gap) as i32,
            26,
            hint,
            Rgba::WHITE,
            false,
        );
        let hint_c = Rgba::rgb(220, 220, 220);
        blit_centered(
            &mut panel,
            &tr("Press C to copy color"),
            local,
            48.0,
            hint,
            hint_c,
        );
        blit_centered(
            &mut panel,
            &tr("Shift: switch RGB/HEX"),
            local,
            66.0,
            hint,
            hint_c,
        );
        let mut p = PixmapPaint::default();
        p.blend_mode = BlendMode::Source;
        pm.draw_pixmap(ix, iy, panel.as_ref(), &p, Transform::identity(), None);
    }
    align_dirty(mag.grid.union(mag.info).inflated(8.0))
}

fn blit_centered(pm: &mut Pixmap, text: &str, boxr: Rect, y: f32, px: i32, color: Rgba) {
    let (tw, _) = pinshot_core::font::measure_chrome(text, px as f32);
    let x = boxr.x + ((boxr.w - tw) * 0.5).round();
    blit_into(pm, text, x as i32, y as i32, px, color, false);
}

pub fn paint_toolbar(pm: &mut Pixmap, tb: &Toolbar, cursor: Option<Point>) -> Rect {
    paint_chrome_surface(pm, tb.bar, 6.0);
    let tool_end = tb
        .buttons
        .iter()
        .position(|b| {
            !matches!(
                b.action,
                TbAction::Tool(_) | TbAction::Shape | TbAction::LineStroke
            )
        })
        .unwrap_or(tb.buttons.len());
    if tool_end > 0 && tool_end < tb.buttons.len() {
        let left = tb.buttons[tool_end - 1].rect.right();
        let right = tb.buttons[tool_end].rect.x;
        let sx = (left + right) * 0.5;
        fill_rect(
            pm,
            Rect::new(sx, tb.bar.y + 7.0, 1.0, tb.bar.h - 14.0),
            CHROME_SEP,
        );
    }
    for b in &tb.buttons {
        let hot = hover_at(cursor, b.rect);
        if b.checked || hot {
            let fill = if b.checked {
                CHROME_SELECTED
            } else {
                CHROME_HOVER
            };
            fill_rounded_rect(pm, b.rect.inflated(-1.0), 4.0, fill);
            if b.checked {
                stroke_rounded_rect(pm, b.rect.inflated(-1.0), 4.0, CHROME_SELECTED_EDGE, 1.0);
            }
        }
        let ink = if b.checked { Rgba::WHITE } else { CHROME_ICON };
        draw_icon(pm, b.icon, b.rect.adjusted(3.0, 3.0, -3.0, -3.0), Some(ink));
    }
    let mut dirty = tb.bar;
    if let Some(pr) = tb.panel {
        paint_chrome_surface(pm, pr, 5.0);
        if !tb.width_label.is_empty() {
            paint_width_chip(pm, tb.width_label, tb.width_value);
        }
        for extra in &tb.extras {
            paint_extra(pm, extra, cursor);
        }
        for sep in &tb.seps {
            fill_rect(pm, *sep, CHROME_SEP);
        }
        if let Some(menu) = &tb.menu {
            paint_chrome_surface(pm, menu.rect, 5.0);
            for b in &menu.items {
                let hot = hover_at(cursor, b.rect);
                if b.checked || hot {
                    fill_rounded_rect(
                        pm,
                        b.rect,
                        3.0,
                        if b.checked {
                            CHROME_SELECTED
                        } else {
                            CHROME_HOVER
                        },
                    );
                }
                let ink = if b.checked { Rgba::WHITE } else { CHROME_ICON };
                if matches!(b.action, crate::toolbar::TbAction::Family(_) | crate::toolbar::TbAction::FontMenuScroll(_)) {
                    let label = if b.tip.is_empty() { pinshot_core::i18n::tr("Default") } else { b.tip.to_string() };
                    let label = fit_toolbar_text(&label, b.rect.w - 12.0, 12.0);
                    crate::text::blit_into_pm(pm, &label, b.rect.x + 6.0, b.rect.y + 4.0, 12, ink, b.checked);
                } else {
                    draw_icon(pm, b.icon, b.rect.adjusted(4.0, 2.0, -4.0, -2.0), Some(ink));
                }
            }
            if let Some((track, thumb)) = menu.scrollbar {
                fill_rounded_rect(pm, track, 2.0, CHROME_SEP);
                fill_rounded_rect(pm, thumb, 2.0, CHROME_ICON);
            }
            dirty = dirty.union(menu.rect);
        }
        if !tb.custom_color.is_empty() {
            fill_rounded_rect(pm, tb.custom_color, 3.0, tb.custom_value);
            stroke_rounded_rect(pm, tb.custom_color, 3.0, Rgba::rgb(40, 40, 44), 1.0);
            paint_color_nub(pm, tb.custom_color);
        }
        for (r, c, _) in &tb.swatches {
            fill_rounded_rect(pm, *r, 2.0, *c);
            let selected =
                c.r == tb.custom_value.r && c.g == tb.custom_value.g && c.b == tb.custom_value.b;
            if selected {
                stroke_rounded_rect(pm, r.inflated(1.0), 2.5, CHROME_SELECTED_EDGE, 1.5);
            } else {
                stroke_rounded_rect(pm, *r, 2.0, Rgba::rgb(170, 172, 176), 1.0);
            }
        }
        if !tb.slider.is_empty() {
            paint_slider(pm, tb.slider, tb.slider_value, tb.slider_min, tb.slider_max);
        }
        dirty = dirty.union(pr);
    }
    if let Some(cursor) = cursor {
        // Exact hit first: hover cells abut, so inflated(3) on the first
        // icon used to steal 保存 / 复制 / 关闭.
        let hit = tb
            .buttons
            .iter()
            .find(|b| b.rect.contains(cursor))
            .or_else(|| {
                tb.buttons
                    .iter()
                    .find(|b| b.rect.inflated(3.0).contains(cursor))
            });
        if let Some(b) = hit {
            let tip = tr(b.tip);
            let (tw, th) = crate::text::measure(&tip, 11, false);
            let tw = tw as f32 + 12.0;
            let th = th as f32 + 8.0;
            let mut tx = b.rect.x;
            let panel_below = tb
                .panel
                .is_some_and(|pr| pr.y + 1.0 >= tb.bar.bottom() - 2.0);
            // Idle pin hover bar sits on the bottom edge; keep the tip above.
            let hover_only = tb.panel.is_none();
            let mut ty = if panel_below || hover_only {
                b.rect.y - th - 4.0
            } else {
                b.rect.bottom() + 4.0
            };
            if ty < 2.0 {
                ty = b.rect.bottom() + 4.0;
            }
            if ty + th > pm.height() as f32 {
                ty = (b.rect.y - th - 4.0).max(2.0);
            }
            tx = tx.min(pm.width() as f32 - tw - 2.0).max(2.0);
            let tip_r = Rect::new(tx, ty, tw, th);
            fill_rounded_rect(pm, tip_r, 4.0, Rgba::rgba(20, 20, 24, 230));
            crate::text::blit_into_pm(pm, &tip, tx + 6.0, ty + 3.0, 11, Rgba::WHITE, false);
            dirty = dirty.union(tip_r.inflated(2.0));
        }
    }
    let _ = IconKind::Rect;
    dirty.inflated(8.0)
}

pub fn paint_text_delete(pm: &mut Pixmap, r: Rect) {
    fill_rounded_rect(pm, r, 3.0, Rgba::WHITE);
    stroke_rounded_rect(pm, r, 3.0, Rgba::BLUE, 1.0);
    draw_icon(pm, IconKind::Close, r, Some(Rgba::BLUE));
}

fn paint_width_chip(pm: &mut Pixmap, r: Rect, value: i32) {
    fill_rounded_rect(pm, r, 4.0, CHROME_SELECTED);
    paint_width_content(pm, r, value, Rgba::WHITE, true);
}

fn paint_width_content(pm: &mut Pixmap, r: Rect, value: i32, ink: Rgba, bold: bool) {
    // Keep four pixels of padding on each side of the circular preview.
    let max_diameter = (r.w.min(r.h) - 8.0).max(0.0);
    let diameter = value.max(1) as f32;
    if diameter > max_diameter && value > 0 {
        let s = format!("{value}");
        crate::text::blit_into_pm(pm, &s, r.x + 6.0, r.y + 4.0, 12, ink, bold);
    } else {
        let d = diameter.min(max_diameter);
        let cx = r.x + r.w * 0.5 - d * 0.5;
        let cy = r.y + r.h * 0.5 - d * 0.5;
        fill_rounded_rect(pm, Rect::new(cx, cy, d, d), d * 0.5, ink);
    }
}

fn fit_toolbar_text(text: &str, max_width: f32, size: f32) -> String {
    if pinshot_core::font::measure_chrome(text, size).0 <= max_width { return text.to_string(); }
    let mut label = text.to_string();
    while !label.is_empty() {
        label.pop();
        let shortened = format!("{label}…");
        if pinshot_core::font::measure_chrome(&shortened, size).0 <= max_width { return shortened; }
    }
    String::new()
}

fn paint_extra(pm: &mut Pixmap, extra: &TbExtra, cursor: Option<Point>) {
    let hot = hover_at(cursor, extra.rect);
    match extra.kind {
        ExtraKind::Combo => {
            fill_rounded_rect(pm, extra.rect, 3.0, Rgba::WHITE);
            stroke_rounded_rect(pm, extra.rect, 3.0, CHROME_BORDER, 1.0);
            if let Some(icon) = extra.icon {
                draw_icon(
                    pm,
                    icon,
                    Rect::new(extra.rect.x + 4.0, extra.rect.y + 2.0, 18.0, 18.0),
                    Some(CHROME_ICON),
                );
            }
            if !extra.label.is_empty() {
                let label = fit_toolbar_text(&extra.label, extra.rect.w - 22.0, 11.0);
                crate::text::blit_into_pm(
                    pm,
                    &label,
                    extra.rect.x + 6.0,
                    extra.rect.y + 4.0,
                    11,
                    CHROME_TEXT,
                    false,
                );
            }
            paint_chevron(pm, extra.rect.right() - 12.0, extra.rect.y + 9.0);
        }
        ExtraKind::Icon | ExtraKind::BrushSize(_) => {
            let (fill, ink) = if extra.checked {
                (CHROME_SELECTED, Rgba::WHITE)
            } else if hot {
                (CHROME_HOVER, CHROME_TEXT)
            } else {
                (Rgba::WHITE, CHROME_TEXT)
            };
            fill_rounded_rect(pm, extra.rect, 3.0, fill);
            if extra.checked {
                stroke_rounded_rect(pm, extra.rect, 3.0, CHROME_SELECTED_EDGE, 1.0);
            }
            if let ExtraKind::BrushSize(value) = extra.kind {
                paint_width_content(pm, extra.rect, value, ink, extra.checked);
            } else if let Some(icon) = extra.icon {
                draw_icon(
                    pm,
                    icon,
                    extra.rect.adjusted(2.0, 2.0, -2.0, -2.0),
                    Some(if extra.checked {
                        Rgba::WHITE
                    } else {
                        CHROME_ICON
                    }),
                );
            } else {
                crate::text::blit_into_pm(
                    pm,
                    &extra.label,
                    extra.rect.x + 6.0,
                    extra.rect.y + 4.0,
                    12,
                    ink,
                    extra.checked,
                );
            }
        }
    }
}

fn paint_color_nub(pm: &mut Pixmap, r: Rect) {
    let mut pb = PathBuilder::new();
    pb.move_to(r.x + 1.0, r.bottom() - 1.0);
    pb.line_to(r.x + 8.0, r.bottom() - 1.0);
    pb.line_to(r.x + 1.0, r.bottom() - 8.0);
    pb.close();
    if let Some(path) = pb.finish() {
        pm.fill_path(
            &path,
            &paint(Rgba::rgb(30, 30, 34)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn paint_slider(pm: &mut Pixmap, r: Rect, value: i32, min: i32, max: i32) {
    let track = Rect::new(r.x, r.y + r.h * 0.5 - 1.5, r.w, 3.0);
    fill_rounded_rect(pm, track, 1.5, Rgba::rgb(200, 204, 210));
    let t = if max <= min {
        0.0
    } else {
        ((value - min) as f32 / (max - min) as f32).clamp(0.0, 1.0)
    };
    let kx = r.x + t * r.w;
    fill_rounded_rect(
        pm,
        Rect::new(kx - 5.0, r.y - 1.0, 10.0, r.h + 2.0),
        5.0,
        CHROME_SELECTED,
    );
}

fn paint_chevron(pm: &mut Pixmap, x: f32, y: f32) {
    let mut pb = PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + 6.0, y);
    pb.line_to(x + 3.0, y + 4.0);
    pb.close();
    if let Some(path) = pb.finish() {
        pm.fill_path(
            &path,
            &paint(CHROME_ICON),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

pub fn paint_toast(pm: &mut Pixmap, text: &str, w: u32, h: u32) -> Rect {
    let (tw, th) = crate::text::measure(text, 16, true);
    let box_w = (tw as f32 + 28.0).max(80.0);
    let box_h = (th as f32 + 16.0).max(44.0);
    let boxr = Rect::new(
        (w as f32 - box_w) / 2.0,
        h as f32 / 2.0 - box_h / 2.0,
        box_w,
        box_h,
    );
    fill_rounded_rect(pm, boxr, 8.0, Rgba::rgba(20, 20, 20, 220));
    blit_into(
        pm,
        text,
        boxr.x as i32 + 14,
        boxr.y as i32 + 12,
        16,
        Rgba::WHITE,
        true,
    );
    // Inflate past glyph overflow so expire restores the leftover pixels.
    boxr.inflated(12.0)
}

/// Paint the compact keyboard hint in the lower-left shadow margin.
pub fn paint_shortcut_hint(pm: &mut Pixmap, text: &str, w: u32, h: u32) -> Rect {
    let (tw, th) = crate::text::measure(text, 11, false);
    let r = Rect::new(6.0, h as f32 - th as f32 - 10.0, tw as f32 + 12.0, th as f32 + 6.0);
    fill_rounded_rect(pm, r, 3.0, Rgba::rgba(18, 18, 20, 190));
    crate::text::blit_into_pm(pm, text, r.x + 6.0, r.y + 3.0, 11, Rgba::rgba(235, 235, 240, 235), false);
    r.inflated(4.0)
}

fn blit_into(pm: &mut Pixmap, text: &str, x: i32, y: i32, px: i32, color: Rgba, bold: bool) {
    crate::text::blit_into_pm(pm, text, x as f32, y as f32, px, color, bold);
}

/// Arrow pointer baked into the freeze when `` ` `` toggles `show_cursor`.
pub fn paint_pointer(pm: &mut Pixmap, at: Point) -> Rect {
    let mut pb = PathBuilder::new();
    pb.move_to(at.x, at.y);
    pb.line_to(at.x, at.y + 16.0);
    pb.line_to(at.x + 4.5, at.y + 12.5);
    pb.line_to(at.x + 10.0, at.y + 18.0);
    pb.line_to(at.x + 12.0, at.y + 16.0);
    pb.line_to(at.x + 6.5, at.y + 11.0);
    pb.line_to(at.x + 11.0, at.y + 11.0);
    pb.close();
    if let Some(path) = pb.finish() {
        let mut fill = paint(Rgba::WHITE);
        fill.anti_alias = true;
        pm.fill_path(&path, &fill, FillRule::Winding, Transform::identity(), None);
        pm.stroke_path(
            &path,
            &paint(Rgba::rgb(20, 20, 20)),
            &Stroke {
                width: 1.2,
                ..Stroke::default()
            },
            Transform::identity(),
            None,
        );
    }
    Rect::new(at.x - 2.0, at.y - 2.0, 18.0, 24.0)
}

pub fn pixmap_to_raster(pm: &Pixmap) -> Option<Raster> {
    let w = pm.width();
    let h = pm.height();
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for (i, px) in pm.pixels().iter().enumerate() {
        let a = px.alpha();
        if a == 0 {
            continue;
        }
        let o = i * 4;
        rgba[o] = px.red();
        rgba[o + 1] = px.green();
        rgba[o + 2] = px.blue();
        rgba[o + 3] = a;
    }
    Raster::from_rgba(w, h, rgba)
}

pub fn paint_confirm_abort(pm: &mut Pixmap, w: u32, h: u32, dont: bool) -> Rect {
    let lay = pinshot_core::confirm_abort_layout(w as f32, h as f32);
    paint_chrome_surface(pm, lay.frame, 8.0);
    crate::text::blit_into_pm(
        pm,
        &tr("Abort snipping?"),
        lay.frame.x + 16.0,
        lay.frame.y + 14.0,
        14,
        CHROME_TEXT,
        true,
    );
    paint_check_tick(pm, lay.dont.x, lay.dont.y + 3.0, dont);
    crate::text::blit_into_pm(
        pm,
        &tr("Do not show again"),
        lay.dont.x + 20.0,
        lay.dont.y + 4.0,
        12,
        CHROME_TEXT,
        false,
    );
    for (r, label) in [(lay.yes, tr("Yes")), (lay.no, tr("No"))] {
        fill_rounded_rect(pm, r, 5.0, CHROME_CHIP);
        stroke_rounded_rect(pm, r, 5.0, CHROME_BORDER, 1.0);
        crate::text::blit_into_pm(pm, &label, r.x + 22.0, r.y + 5.0, 12, CHROME_TEXT, true);
    }
    lay.frame.inflated(8.0)
}

pub fn paint_color_picker(
    pm: &mut Pixmap,
    w: u32,
    h: u32,
    picker: &pinshot_core::ColorPicker,
) -> Rect {
    let lay = pinshot_core::color_picker_layout(w as f32, h as f32);
    paint_chrome_surface(pm, lay.frame, 8.0);
    crate::text::blit_into_pm(
        pm,
        &tr("Custom color"),
        lay.frame.x + 14.0,
        lay.frame.y + 10.0,
        13,
        CHROME_TEXT,
        true,
    );
    blit_hsv_sv(pm, lay.sv, picker.hsv.0);
    stroke_rect(pm, lay.sv, Rgba::rgb(40, 40, 40), 1.0);
    let marker = Point::new(
        lay.sv.x + picker.hsv.1 * lay.sv.w,
        lay.sv.y + (1.0 - picker.hsv.2) * lay.sv.h,
    );
    fill_rect(
        pm,
        Rect::new(marker.x - 3.0, marker.y - 3.0, 6.0, 6.0),
        Rgba::WHITE,
    );
    stroke_rect(
        pm,
        Rect::new(marker.x - 3.0, marker.y - 3.0, 6.0, 6.0),
        Rgba::BLACK,
        1.0,
    );
    blit_hue_strip(pm, lay.hue);
    stroke_rect(pm, lay.hue, Rgba::rgb(40, 40, 40), 1.0);
    let hy = lay.hue.y + (picker.hsv.0 / 360.0) * lay.hue.h;
    fill_rect(
        pm,
        Rect::new(lay.hue.x - 2.0, hy - 2.0, lay.hue.w + 4.0, 4.0),
        Rgba::WHITE,
    );
    stroke_rect(
        pm,
        Rect::new(lay.hue.x - 2.0, hy - 2.0, lay.hue.w + 4.0, 4.0),
        Rgba::BLACK,
        1.0,
    );
    fill_rect(pm, lay.preview, picker.current);
    stroke_rect(pm, lay.preview, Rgba::rgb(40, 40, 40), 1.0);
    fill_rect(pm, lay.initial, picker.initial);
    stroke_rect(pm, lay.initial, Rgba::rgb(40, 40, 40), 1.0);
    crate::text::blit_into_pm(
        pm,
        &picker.current.hex(),
        lay.preview.x,
        lay.initial.bottom() + 6.0,
        11,
        Rgba::rgb(20, 20, 20),
        false,
    );
    for (i, r) in lay.palette.iter().enumerate() {
        fill_rect(pm, *r, pinshot_core::palette_colors()[i]);
        stroke_rect(pm, *r, Rgba::rgb(60, 60, 60), 1.0);
    }
    for (r, label) in [(lay.ok, tr("OK")), (lay.cancel, tr("Cancel"))] {
        fill_rounded_rect(pm, r, 5.0, CHROME_CHIP);
        stroke_rounded_rect(pm, r, 5.0, CHROME_BORDER, 1.0);
        crate::text::blit_into_pm(pm, &label, r.x + 18.0, r.y + 5.0, 12, CHROME_TEXT, true);
    }
    lay.frame.inflated(8.0)
}

fn blit_hsv_sv(pm: &mut Pixmap, r: Rect, hue: f32) {
    let bw = pm.width();
    let bh = pm.height();
    let x0 = r.x.round().max(0.0) as u32;
    let y0 = r.y.round().max(0.0) as u32;
    let x1 = (r.right().round() as i32).max(0) as u32;
    let y1 = (r.bottom().round() as i32).max(0) as u32;
    let x1 = x1.min(bw);
    let y1 = y1.min(bh);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let ww = (x1 - x0) as f32;
    let hh = (y1 - y0) as f32;
    let pixels = pm.pixels_mut();
    let stride = bw as usize;
    for y in y0..y1 {
        let v = 1.0 - (y - y0) as f32 / hh.max(1.0);
        for x in x0..x1 {
            let s = (x - x0) as f32 / ww.max(1.0);
            let c = pinshot_core::hsv_to_rgb(hue, s, v);
            if let Some(px) = PremultipliedColorU8::from_rgba(c.r, c.g, c.b, 255) {
                pixels[y as usize * stride + x as usize] = px;
            }
        }
    }
}

fn blit_hue_strip(pm: &mut Pixmap, r: Rect) {
    let bw = pm.width();
    let bh = pm.height();
    let x0 = r.x.round().max(0.0) as u32;
    let y0 = r.y.round().max(0.0) as u32;
    let x1 = (r.right().round() as i32).max(0) as u32;
    let y1 = (r.bottom().round() as i32).max(0) as u32;
    let x1 = x1.min(bw);
    let y1 = y1.min(bh);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let hh = (y1 - y0) as f32;
    let pixels = pm.pixels_mut();
    let stride = bw as usize;
    for y in y0..y1 {
        let hue = (y - y0) as f32 / hh.max(1.0) * 359.9;
        let c = pinshot_core::hsv_to_rgb(hue, 1.0, 1.0);
        if let Some(px) = PremultipliedColorU8::from_rgba(c.r, c.g, c.b, 255) {
            for x in x0..x1 {
                pixels[y as usize * stride + x as usize] = px;
            }
        }
    }
}

pub const PIN_SHADOW_M: f32 = 12.0;
pub const PIN_THUMB: u32 = 72;

pub fn paint_pin(
    img: &Raster,
    annot: &Annotator,
    scale: f32,
    click_through: bool,
    glow: f32,
    shadow: bool,
    thumb: bool,
) -> Pixmap {
    paint_pin_ex(
        img,
        annot,
        scale,
        click_through,
        glow,
        shadow,
        thumb,
        true,
        false,
        false,
    )
}

pub fn paint_pin_ex(
    img: &Raster,
    annot: &Annotator,
    scale: f32,
    click_through: bool,
    glow: f32,
    shadow: bool,
    thumb: bool,
    smooth: bool,
    grayscale: bool,
    invert: bool,
) -> Pixmap {
    let shapes = annot.visible_shapes();
    let mut owned = if shapes.is_empty() {
        None
    } else {
        Some(compose(img, &shapes))
    };
    if grayscale {
        let mut c = owned.unwrap_or_else(|| img.clone());
        c = c.grayscale();
        owned = Some(c);
    }
    if invert {
        let mut c = owned.unwrap_or_else(|| img.clone());
        c = c.invert_color();
        owned = Some(c);
    }
    let photo = owned.as_ref().unwrap_or(img);
    let (content, w, h) = if thumb {
        let s = photo.width.min(photo.height) as f32;
        let src = Rect::new(
            (photo.width as f32 - s) / 2.0,
            (photo.height as f32 - s) / 2.0,
            s,
            s,
        );
        let cropped = photo.crop(src).scale_nn(PIN_THUMB, PIN_THUMB);
        (Some(cropped), PIN_THUMB, PIN_THUMB)
    } else {
        let w = ((img.width as f32) * scale).round().max(8.0) as u32;
        let h = ((img.height as f32) * scale).round().max(8.0) as u32;
        (None, w, h)
    };
    let photo = content.as_ref().unwrap_or(photo);
    let m = if shadow { PIN_SHADOW_M } else { 0.0 };
    let pw = (w as f32 + 2.0 * m).round().max(8.0) as u32;
    let ph = (h as f32 + 2.0 * m).round().max(8.0) as u32;
    let mut pm = Pixmap::new(pw, ph).unwrap_or_else(|| Pixmap::new(8, 8).unwrap());
    let img_r = Rect::new(m, m, w as f32, h as f32);
    if shadow {
        paint_pin_shadow(&mut pm, img_r, m);
    }
    // Image over the shadow (Python clips to `img_rect`) so the +2 drop
    // offset cannot darken the photo's top/left edge.
    blit_raster_ex(&mut pm, photo, img_r, smooth);
    // Python `QRectF(r).adjusted(-0.5, -0.5, -0.5, -0.5)` — hairline on the
    // image box, not an inset that eats the first pixel row/column.
    let stroke_r = Rect::new(m - 0.5, m - 0.5, w as f32, h as f32);
    if glow > 0.0 {
        stroke_rect(
            &mut pm,
            stroke_r,
            Rgba::rgba(40, 220, 90, (255.0 * glow) as u8),
            3.0,
        );
    } else if click_through {
        stroke_rect(&mut pm, stroke_r, Rgba::rgba(180, 40, 220, 200), 2.0);
        fill_rect(
            &mut pm,
            Rect::new(m + w as f32 - 88.0, m + 2.0, 86.0, 18.0),
            Rgba::rgba(180, 40, 220, 200),
        );
        blit_into(
            &mut pm,
            &tr("CLICK-THRU"),
            (m + w as f32 - 84.0) as i32,
            (m + 4.0) as i32,
            10,
            Rgba::WHITE,
            true,
        );
    } else {
        let col = if annot.tool.is_some() {
            Rgba::rgb(255, 140, 0)
        } else {
            Rgba::rgba(30, 144, 255, 150)
        };
        stroke_rect(&mut pm, stroke_r, col, 1.0);
    }
    let _ = Tool::Rect;
    pm
}

fn paint_pin_shadow(pm: &mut Pixmap, img_r: Rect, m: f32) {
    if m < 1.0 {
        return;
    }
    let pw = pm.width() as i32;
    let ph = pm.height() as i32;
    let x0 = img_r.x;
    let y0 = img_r.y;
    let x1 = img_r.x + img_r.w;
    let y1 = img_r.y + img_r.h;
    let drop = 2.0;
    let data = pm.data_mut();
    let stride = pw as usize * 4;
    for y in 0..ph {
        for x in 0..pw {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            if fx >= x0 && fx < x1 && fy >= y0 && fy < y1 {
                continue;
            }
            let dx = if fx < x0 {
                x0 - fx
            } else if fx > x1 {
                fx - x1
            } else {
                0.0
            };
            let dy = if fy < y0 + drop {
                y0 + drop - fy
            } else if fy > y1 + drop {
                fy - (y1 + drop)
            } else {
                0.0
            };
            let d = dx.max(dy);
            if d <= 0.0 || d > m {
                continue;
            }
            let t = 1.0 - d / m;
            let a = ((46.0 * t * t) as u8).saturating_add(2);
            let i = y as usize * stride + x as usize * 4;
            if i + 3 < data.len() {
                data[i] = 0;
                data[i + 1] = 0;
                data[i + 2] = 0;
                data[i + 3] = a;
            }
        }
    }
}

/// Softbuffer / XRGB: drop alpha. Overlay chrome is opaque.
pub fn present(buf: &mut [u32], pm: &Pixmap) {
    present_ex(buf, pm, false);
}

/// Softbuffer ARGB (premultiplied). Pin shadow needs this under picom.
pub fn present_argb(buf: &mut [u32], pm: &Pixmap) {
    present_ex(buf, pm, true);
}

fn present_ex(buf: &mut [u32], pm: &Pixmap, argb: bool) {
    let data = pm.data();
    let n = buf.len().min(data.len() / 4);
    if n == 0 {
        return;
    }
    let src = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u32, n) };
    for i in 0..n {
        let v = src[i];
        let r = v & 0xff;
        let g = (v >> 8) & 0xff;
        let b = (v >> 16) & 0xff;
        let a = (v >> 24) & 0xff;
        buf[i] = if argb {
            a << 24 | r << 16 | g << 8 | b
        } else {
            r << 16 | g << 8 | b
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_hwnd() -> crate::native::NativeWin {
        #[cfg(windows)]
        {
            windows::Win32::Foundation::HWND::default()
        }
        #[cfg(not(windows))]
        {
            ()
        }
    }

    fn gray_shot(w: u32, h: u32, v: u8) -> Raster {
        let mut px = vec![0u8; (w * h * 4) as usize];
        for p in px.chunks_exact_mut(4) {
            p[0] = v;
            p[1] = v;
            p[2] = v;
            p[3] = 255;
        }
        Raster::from_rgba(w, h, px).expect("raster")
    }

    fn paint_sel(front: &mut OverlayFront, shot: &Raster, sel: &Selection) {
        front.paint(
            dummy_hwnd(),
            sel,
            Rect::EMPTY,
            Point::new(8.0, 8.0),
            1.0,
            false,
            false,
            &Annotator::new(),
            None,
            None,
            false,
            shot,
            None,
            None,
        );
    }

    #[test]
    fn size_label_restore_covers_measured_glyphs() {
        let hole = Rect::new(40.0, 80.0, 220.0, 120.0);
        let label = "6800 × 3780 px";
        let boxr = size_label_box(hole, label, 1280, 800);
        let dirty = size_label_dirty(hole, label, 1280, 800);
        let (tw, _th) = crate::text::measure(label, 12, false);
        assert!(
            dirty.w >= tw as f32 + 16.0,
            "size-label restore {} too narrow for glyphs {}",
            dirty.w,
            tw
        );
        assert!(dirty.x <= boxr.x && dirty.y <= boxr.y);
        assert!(boxr.y + boxr.h <= hole.y + 0.5, "chip sits above the hole");
    }

    #[test]
    fn size_label_drag_clears_old_chip() {
        let w = 640u32;
        let h = 400u32;
        let shot = gray_shot(w, h, 210);
        let mut front = OverlayFront::new(&shot, 110).expect("front");
        let bounds = Rect::new(0.0, 0.0, w as f32, h as f32);
        let mut sel = Selection::new();
        let a = Rect::new(80.0, 90.0, 180.0, 100.0);
        sel.set_rect(a, bounds);
        paint_sel(&mut front, &shot, &sel);

        let chip = size_label_box(a, &a.size_label(), w, h);
        let sx = (chip.x + 18.0) as u32;
        let sy = (chip.y + chip.h * 0.5) as u32;
        let p0 = front.pixmap().pixel(sx, sy).expect("px").demultiply();
        assert!(
            p0.red() < 100,
            "size chip should be a dark box, got {},{},{}",
            p0.red(),
            p0.green(),
            p0.blue()
        );

        let b = Rect::new(300.0, 220.0, 180.0, 100.0);
        sel.set_rect(b, bounds);
        paint_sel(&mut front, &shot, &sel);

        let p1 = front.pixmap().pixel(sx, sy).expect("px").demultiply();
        assert!(
            p1.red() > 110 && (p1.red() as i16 - p1.green() as i16).abs() < 10,
            "old size chip must be restored to the dimmed capture, got {},{},{}",
            p1.red(),
            p1.green(),
            p1.blue()
        );
        let ghost_luma = (p1.red() as u16 + p1.green() as u16 + p1.blue() as u16) / 3;
        assert!(
            ghost_luma > 110,
            "leftover size-label trail at {sx},{sy} luma={ghost_luma}"
        );
    }

    #[test]
    fn size_label_grow_clears_old_digits() {
        let w = 640u32;
        let h = 400u32;
        let shot = gray_shot(w, h, 210);
        let mut front = OverlayFront::new(&shot, 110).expect("front");
        let bounds = Rect::new(0.0, 0.0, w as f32, h as f32);
        let mut sel = Selection::new();
        let small = Rect::new(80.0, 90.0, 40.0, 30.0);
        sel.set_rect(small, bounds);
        paint_sel(&mut front, &shot, &sel);
        let small_chip = size_label_box(small, &small.size_label(), w, h);
        let big = Rect::new(80.0, 90.0, 260.0, 180.0);
        sel.set_rect(big, bounds);
        paint_sel(&mut front, &shot, &sel);
        let big_chip = size_label_box(big, &big.size_label(), w, h);
        // Growing keeps the top-left; the chip text gets longer. Pixels that
        // were just past the old chip must not keep the old dark fill.
        if big_chip.right() > small_chip.right() + 4.0 {
            let x = (small_chip.right() + 2.0) as u32;
            let y = (small_chip.y + small_chip.h * 0.5) as u32;
            let px = front.pixmap().pixel(x, y).expect("px").demultiply();
            let on_new = big_chip.contains(Point::new(x as f32, y as f32));
            if !on_new {
                assert!(
                    px.red() > 110,
                    "grow left a size-chip trail at {x},{y} r={}",
                    px.red()
                );
            }
        }
        let x = (small_chip.x + 8.0) as u32;
        let y = (small_chip.y + small_chip.h * 0.5) as u32;
        let px = front.pixmap().pixel(x, y).expect("px").demultiply();
        assert!(px.red() < 100, "resized chip should still be dark");
    }

    #[test]
    fn magnifier_outside_sel_does_not_double_dim() {
        let w = 640u32;
        let h = 400u32;
        let shot = gray_shot(w, h, 210);
        let mut front = OverlayFront::new(&shot, 110).expect("front");
        let bounds = Rect::new(0.0, 0.0, w as f32, h as f32);
        let mut sel = Selection::new();
        sel.set_rect(Rect::new(20.0, 20.0, 100.0, 100.0), bounds);
        let c0 = Point::new(200.0, 40.0);
        let c1 = Point::new(400.0, 280.0);
        let paint = |front: &mut OverlayFront, cursor: Point| {
            front.paint(
                dummy_hwnd(),
                &sel,
                Rect::EMPTY,
                cursor,
                1.0,
                false,
                true,
                &Annotator::new(),
                None,
                None,
                false,
                &shot,
                None,
                None,
            );
        };
        paint(&mut front, c0);
        let mag0 = layout_magnifier(c0, bounds, Rgba::rgb(40, 40, 40), Point::new(200.0, 40.0));
        let sx = (mag0.grid.x + 4.0) as u32;
        let sy = (mag0.grid.y + 4.0) as u32;
        let p_mag = front.pixmap().pixel(sx, sy).expect("px").demultiply();
        assert!(
            p_mag.red() > 150,
            "magnifier should show the undimmed capture, got {}",
            p_mag.red()
        );
        paint(&mut front, c1);
        let p = front.pixmap().pixel(sx, sy).expect("px").demultiply();
        let mag1 = layout_magnifier(c1, bounds, Rgba::rgb(40, 40, 40), Point::new(400.0, 280.0));
        let still_on_mag = mag1
            .grid
            .union(mag1.info)
            .contains(Point::new(sx as f32, sy as f32));
        if !still_on_mag {
            assert!(
                (p.red() as i16 - 119).abs() <= 3 && (p.red() as i16 - p.green() as i16).abs() < 4,
                "old magnifier outside the sel must restore to one dim layer, got {},{},{}",
                p.red(),
                p.green(),
                p.blue()
            );
        }
    }

    #[test]
    fn leaving_hover_hole_matches_never_hovered_dim() {
        let w = 640u32;
        let h = 400u32;
        let shot = gray_shot(w, h, 210);
        let mut front = OverlayFront::new(&shot, 160).expect("front");
        let idle = Selection::new();
        let base = front.pixmap().pixel(40, 40).expect("px").demultiply();
        let paint_hover = |front: &mut OverlayFront, hover: Rect, mag: bool, cursor: Point| {
            front.paint(
                dummy_hwnd(),
                &idle,
                hover,
                cursor,
                1.0,
                false,
                mag,
                &Annotator::new(),
                None,
                None,
                false,
                &shot,
                None,
                None,
            );
        };
        paint_hover(
            &mut front,
            Rect::new(10.0, 10.0, 180.0, 120.0),
            true,
            Point::new(40.0, 40.0),
        );
        let lit = front.pixmap().pixel(40, 40).expect("px").demultiply();
        assert!(
            lit.red() > 180,
            "hover hole should be undimmed, got {}",
            lit.red()
        );
        paint_hover(
            &mut front,
            Rect::new(400.0, 200.0, 160.0, 100.0),
            true,
            Point::new(500.0, 250.0),
        );
        let after = front.pixmap().pixel(40, 40).expect("px").demultiply();
        assert!(
            (after.red() as i16 - base.red() as i16).abs() <= 2
                && (after.red() as i16 - after.green() as i16).abs() < 4,
            "left hover must match never-hovered dim {} not extra-dark {},{},{}",
            base.red(),
            after.red(),
            after.green(),
            after.blue()
        );
        let never = front.pixmap().pixel(250, 300).expect("px").demultiply();
        assert!(
            (never.red() as i16 - base.red() as i16).abs() <= 2,
            "never-hovered should stay at the freeze dim, got {} vs {}",
            never.red(),
            base.red()
        );
        // The navy AA ring sat *outside* the hole and used to remain as a
        // darker window-shaped stain after the hover moved away.
        let ring = front.pixmap().pixel(8, 8).expect("px").demultiply();
        assert!(
            (ring.red() as i16 - base.red() as i16).abs() <= 2
                && (ring.red() as i16 - ring.green() as i16).abs() < 4,
            "old hole ring must restore to the freeze dim {}, got {},{},{}",
            base.red(),
            ring.red(),
            ring.green(),
            ring.blue()
        );
    }

    fn max_red_on_row(pm: &Pixmap, x: u32, y0: u32, y1: u32) -> u8 {
        let mut m = 0u8;
        for y in y0..y1 {
            if let Some(p) = pm.pixel(x, y) {
                m = m.max(p.demultiply().red());
            }
        }
        m
    }

    #[test]
    fn annot_outside_hole_sits_under_the_dim() {
        use pinshot_core::annotation::{Shape, Style, Tool};
        let w = 640u32;
        let h = 400u32;
        let shot = gray_shot(w, h, 210);
        let mut front = OverlayFront::new(&shot, 160).expect("front");
        let bounds = Rect::new(0.0, 0.0, w as f32, h as f32);
        let mut sel = Selection::new();
        let hole = Rect::new(20.0, 20.0, 120.0, 100.0);
        sel.set_rect(hole, bounds);
        let mut annot = Annotator::new();
        annot.tool = Some(Tool::Line);
        let mut st = Style::for_tool(Tool::Line);
        st.color = Rgba::rgb(255, 0, 0);
        st.width = 8;
        annot.shapes.push(Shape::new(
            Tool::Line,
            vec![Point::new(40.0, 70.0), Point::new(400.0, 70.0)],
            st,
        ));
        let paint = |front: &mut OverlayFront, sel: &Selection, annot: &Annotator| {
            front.paint(
                dummy_hwnd(),
                sel,
                Rect::EMPTY,
                Point::new(80.0, 70.0),
                1.0,
                false,
                false,
                annot,
                None,
                None,
                false,
                &shot,
                None,
                None,
            );
        };
        paint(&mut front, &sel, &annot);
        if let Ok(dir) = std::env::var("PINSHOT_DUMP_UI") {
            let _ = std::fs::create_dir_all(&dir);
            let _ = front
                .pixmap()
                .save_png(format!("{dir}/annot_under_dim.png"));
        }
        let inside = max_red_on_row(front.pixmap(), 70, 64, 76);
        let outside = max_red_on_row(front.pixmap(), 280, 64, 76);
        let dim_bg = front
            .pixmap()
            .pixel(280, 200)
            .expect("bg")
            .demultiply()
            .red();
        assert!(
            inside > 180,
            "ink inside the hole must stay undimmed, got {inside}"
        );
        assert!(
            outside > dim_bg + 8,
            "ink outside the hole must still be visible under the dim, outside={outside} bg={dim_bg}"
        );
        assert!(
            (outside as i16) < (inside as i16) - 40,
            "ink outside the hole must sit under the shadow, inside={inside} outside={outside}"
        );

        let cover = Rect::new(240.0, 40.0, 200.0, 80.0);
        sel.set_rect(cover, bounds);
        paint(&mut front, &sel, &annot);
        let uncovered = max_red_on_row(front.pixmap(), 280, 64, 76);
        assert!(
            uncovered > 180,
            "moving the hole over the stroke must undim the ink, got {uncovered}"
        );
        let left_behind = max_red_on_row(front.pixmap(), 70, 64, 76);
        assert!(
            left_behind > dim_bg + 8 && (left_behind as i16) < (uncovered as i16) - 40,
            "ink must stay on the freeze when the hole moves, left={left_behind} bg={dim_bg} uncovered={uncovered}"
        );
    }

    fn skip_chrome(p: Point, hole: Rect, mag: Magnifier, size: Rect) -> bool {
        hole.inflated(10.0).contains(p)
            || mag.grid.union(mag.info).inflated(4.0).contains(p)
            || size.contains(p)
    }

    #[test]
    fn magnifier_and_hover_must_not_stain_the_freeze() {
        let w = 640u32;
        let h = 400u32;
        let shot = gray_shot(w, h, 210);
        let clean = OverlayFront::new(&shot, 160).expect("clean");
        let mut front = OverlayFront::new(&shot, 160).expect("front");
        let bounds = Rect::new(0.0, 0.0, w as f32, h as f32);
        let idle = Selection::new();
        let paint = |front: &mut OverlayFront, hover: Rect, cursor: Point| {
            front.paint(
                dummy_hwnd(),
                &idle,
                hover,
                cursor,
                1.0,
                false,
                true,
                &Annotator::new(),
                None,
                None,
                false,
                &shot,
                None,
                None,
            );
        };
        paint(
            &mut front,
            Rect::new(10.0, 10.0, 180.0, 120.0),
            Point::new(40.0, 40.0),
        );
        let c1 = Point::new(500.0, 250.0);
        let h1 = Rect::new(400.0, 200.0, 160.0, 100.0);
        paint(&mut front, h1, c1);
        let mag = layout_magnifier(c1, bounds, Rgba::rgb(40, 40, 40), Point::new(500.0, 250.0));
        let size = size_label_dirty(h1, &h1.size_label(), w, h);
        let mut stained = 0u32;
        for y in 0..h {
            for x in 0..w {
                let p = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                if skip_chrome(p, h1, mag, size) {
                    continue;
                }
                let a = front.pixmap().pixel(x, y).expect("a");
                let b = clean.pixmap().pixel(x, y).expect("b");
                if a != b {
                    stained += 1;
                    if stained <= 3 {
                        let da = a.demultiply();
                        let db = b.demultiply();
                        panic!(
                            "freeze stained at {x},{y}: got {},{},{} want {},{},{}",
                            da.red(),
                            da.green(),
                            da.blue(),
                            db.red(),
                            db.green(),
                            db.blue()
                        );
                    }
                }
            }
        }
        assert_eq!(stained, 0);
    }

    /// Re-dimming dirty tiles from `shot` rounded differently than the first
    /// full-frame dim, so moving the mouse washed the freeze. Far pixels must
    /// stay bit-identical to the never-moved overlay.
    #[test]
    fn moving_mouse_keeps_far_pixels_at_first_dim() {
        let w = 640u32;
        let h = 400u32;
        let shot = gray_shot(w, h, 210);
        let clean = OverlayFront::new(&shot, 160).expect("clean");
        let mut front = OverlayFront::new(&shot, 160).expect("front");
        let idle = Selection::new();
        let still = clean.pixmap().pixel(20, 380).expect("still");
        for i in 0..12 {
            let x = 80.0 + i as f32 * 28.0;
            let y = 60.0 + i as f32 * 12.0;
            front.paint(
                dummy_hwnd(),
                &idle,
                Rect::new(x - 40.0, y - 30.0, 140.0, 90.0),
                Point::new(x, y),
                1.0,
                false,
                true,
                &Annotator::new(),
                None,
                None,
                false,
                &shot,
                None,
                None,
            );
        }
        let after = front.pixmap().pixel(20, 380).expect("after");
        assert_eq!(
            after,
            still,
            "mouse move washed the freeze dim: still {:?} after {:?}",
            still.demultiply(),
            after.demultiply()
        );
    }

    #[test]
    fn magnifier_hints_stay_inside_info_panel() {
        let shot = gray_shot(80, 80, 40);
        let cursor = Point::new(40.0, 40.0);
        let mag = layout_magnifier(
            Point::new(8.0, 8.0),
            Rect::new(0.0, 0.0, 400.0, 400.0),
            Rgba::rgb(40, 40, 40),
            Point::new(40.0, 40.0),
        );
        let mut pm = Pixmap::new(400, 400).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(119, 119, 119, 255));
        let _ = paint_magnifier(&mut pm, &shot, cursor, 1.0, true, mag);
        let info = mag.info;
        for (x, y) in [
            (info.x as u32 - 1, (info.y + 54.0) as u32),
            ((info.right() as u32) + 1, (info.y + 54.0) as u32),
            ((info.x + 4.0) as u32, (info.bottom() as u32) + 1),
        ] {
            if x >= 400 || y >= 400 {
                continue;
            }
            let p = pm.pixel(x, y).unwrap().demultiply();
            assert!(
                (p.red() as i16 - 119).abs() <= 2,
                "hint/panel must not SrcOver outside info at {x},{y} r={}",
                p.red()
            );
        }
    }

    #[test]
    fn toast_rect_covers_measured_glyphs() {
        let mut pm = Pixmap::new(1280, 800).unwrap();
        let text = "上次选区 Last snip area";
        let r = paint_toast(&mut pm, text, 1280, 800);
        let (tw, _th) = crate::text::measure(text, 16, true);
        assert!(
            r.w >= tw as f32 + 20.0,
            "toast restore {} too narrow for glyphs {}",
            r.w,
            tw
        );
        assert!((r.x + r.w / 2.0 - 640.0).abs() < 40.0);
    }

    #[test]
    fn pin_shadow_is_soft_and_image_edge_stays_bright() {
        let mut px = vec![0u8; 40 * 30 * 4];
        for p in px.chunks_exact_mut(4) {
            p[0] = 200;
            p[1] = 200;
            p[2] = 200;
            p[3] = 255;
        }
        let img = Raster::from_rgba(40, 30, px).expect("raster");
        let pm = paint_pin(&img, &Annotator::new(), 1.0, false, 0.0, true, false);
        let m = PIN_SHADOW_M as u32;
        assert_eq!(pm.width(), 40 + 2 * m, "widget is image + 2M");
        assert_eq!(pm.height(), 30 + 2 * m);
        // Hairline sits on x=m-0.5; sample well outside it.
        let mid_shadow = pm.pixel(3, m + 14).expect("outer shadow");
        assert!(
            mid_shadow.alpha() < 255 && mid_shadow.alpha() > 0,
            "outer shadow must be translucent, a={}",
            mid_shadow.alpha()
        );
        let br_shadow = pm
            .pixel(pm.width() - 3, pm.height() - 3)
            .expect("bottom-right shadow");
        assert!(
            br_shadow.alpha() < 255,
            "shadow must exist on all sides (not a top/left crop), a={}",
            br_shadow.alpha()
        );
        let top_img = pm.pixel(m + 4, m + 2).expect("image top");
        assert_eq!(top_img.alpha(), 255);
        assert!(
            top_img.red() > 150,
            "photo top must not be a dark band, r={}",
            top_img.red()
        );
        let left_img = pm.pixel(m + 2, m + 10).expect("image left");
        assert!(
            left_img.red() > 150,
            "photo left must not be a dark band, r={}",
            left_img.red()
        );
        let mut buf = vec![0u32; (pm.width() * pm.height()) as usize];
        present_argb(&mut buf, &pm);
        let sh = buf[(3 + (m + 14) * pm.width()) as usize];
        assert!(
            sh >> 24 > 0 && sh >> 24 < 255,
            "present_argb keeps shadow alpha, got {sh:#010x}"
        );
    }

    #[test]
    fn pin_hover_toolbar_paints_tip_on_every_icon() {
        use crate::toolbar::Toolbar;
        let sel = Rect::new(0.0, 0.0, 280.0, 160.0);
        let over = Rect::new(0.0, 0.0, 280.0, 160.0);
        let tb = Toolbar::layout_pin_hover(sel, over);
        assert_eq!(tb.buttons.len(), 4);
        for b in &tb.buttons {
            let mut pm = Pixmap::new(280, 160).unwrap();
            pm.fill(tiny_skia::Color::from_rgba8(200, 200, 200, 255));
            paint_toolbar(&mut pm, &tb, Some(b.rect.center()));
            let y = (b.rect.y - 6.0).max(1.0) as u32;
            let x = (b.rect.x + 6.0).clamp(1.0, 278.0) as u32;
            let px = pm.pixel(x, y).expect("px").demultiply();
            assert!(
                px.red() < 60 && px.green() < 60 && px.blue() < 60,
                "expected a dark tooltip above {:?}, got {},{},{}",
                b.tip,
                px.red(),
                px.green(),
                px.blue()
            );
        }
    }

    #[test]
    fn snip_ring_stays_above_the_loupe() {
        let w = 640u32;
        let h = 400u32;
        let shot = gray_shot(w, h, 210);
        let mut front = OverlayFront::new(&shot, 160).expect("front");
        let idle = Selection::new();
        // Default loupe is cursor+22. This lands its top-left on the ring.
        let hover = Rect::new(80.0, 40.0, 220.0, 140.0);
        let cursor = Point::new(61.0, 21.0);
        front.paint(
            dummy_hwnd(),
            &idle,
            hover,
            cursor,
            1.0,
            false,
            true,
            &Annotator::new(),
            None,
            None,
            false,
            &shot,
            None,
            None,
        );
        let mag = layout_magnifier(
            cursor,
            Rect::new(0.0, 0.0, w as f32, h as f32),
            Rgba::BLACK,
            cursor,
        );
        let x = (hover.x + 3.0) as u32;
        let y = (hover.y + 3.0) as u32;
        let on_mag = mag
            .grid
            .union(mag.info)
            .contains(Point::new(x as f32 + 0.5, y as f32 + 0.5));
        assert!(on_mag, "test must place the loupe on the ring");
        assert!(
            ring_strips(hover)
                .iter()
                .any(|s| s.contains(Point::new(x as f32 + 0.5, y as f32 + 0.5))),
            "sample must sit on a ring dirty strip"
        );
        let p = front.pixmap().pixel(x, y).expect("px").demultiply();
        assert!(
            p.green() > 160 && (p.green() as i16) > (p.blue() as i16) + 40,
            "loupe must not bury the hover ring, got {},{},{}",
            p.red(),
            p.green(),
            p.blue()
        );
    }

    #[test]
    fn snip_ring_is_light_green() {
        let mut pm = Pixmap::new(80, 60).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(40, 40, 40, 255));
        paint_snip_ring(
            &mut pm,
            Rect::new(10.0, 8.0, 50.0, 36.0),
            Rgba::SNIP_RING,
            Rgba::SNIP_RING_EDGE,
        );
        // Ring is inset so AA cannot stain the dim mask outside the hole.
        let p = pm.pixel(13, 20).expect("edge").demultiply();
        assert!(
            p.green() > 160 && (p.green() as i16) > (p.blue() as i16) + 40 && p.red() < 180,
            "ring should be light green, got {},{},{}",
            p.red(),
            p.green(),
            p.blue()
        );
    }

    #[test]
    fn selection_handles_are_eight_discs() {
        let hole = Rect::new(20.0, 16.0, 80.0, 48.0);
        assert_eq!(handle_points(hole).len(), 8);
        let mut pm = Pixmap::new(40, 40).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(80, 80, 80, 255));
        paint_handle_dot(&mut pm, Point::new(20.0, 20.0), Rgba::SNIP_RING);
        let mid = pm.pixel(20, 20).expect("mid").demultiply();
        assert!(
            mid.red() > 200 && mid.green() > 200,
            "handle center should be white, got {},{},{}",
            mid.red(),
            mid.green(),
            mid.blue()
        );
        let corner = pm.pixel(16, 16).expect("corner").demultiply();
        assert!(
            corner.red() < 120 && (corner.red() as i16 - corner.green() as i16).abs() < 20,
            "square corner must stay the gray backdrop (circle, not box), got {},{},{}",
            corner.red(),
            corner.green(),
            corner.blue()
        );
    }

    #[test]
    fn toolbar_selected_uses_blue_accent() {
        use crate::toolbar::Toolbar;
        use pinshot_core::annotation::{Style, Tool};
        let sel = Rect::new(100.0, 100.0, 400.0, 240.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let st = Style::for_tool(Tool::Text);
        let tb = Toolbar::layout(
            sel,
            over,
            Some(Tool::Text),
            pinshot_core::ArrowKind::None,
            Some(&st),
        );
        let mut pm = Pixmap::new(1920, 1200).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(80, 80, 80, 255));
        paint_toolbar(&mut pm, &tb, None);
        let btn = tb
            .buttons
            .iter()
            .find(|b| b.checked)
            .expect("text selected");
        let px = pm
            .pixel((btn.rect.x + 2.0) as u32, (btn.rect.y + 2.0) as u32)
            .expect("px")
            .demultiply();
        assert!(
            (px.blue() as i16) > (px.red() as i16) + 15,
            "selected tool fill should be blue, got {},{},{}",
            px.red(),
            px.green(),
            px.blue()
        );
        let bar_px = pm
            .pixel((tb.bar.x + 3.0) as u32, (tb.bar.y + 3.0) as u32)
            .expect("bar")
            .demultiply();
        assert!(
            bar_px.red() > 240 && bar_px.green() > 240 && bar_px.blue() > 240,
            "bar chrome should be near-white, got {},{},{}",
            bar_px.red(),
            bar_px.green(),
            bar_px.blue()
        );
        let corner = pm
            .pixel((btn.rect.right() - 4.0) as u32, (btn.rect.y + 4.0) as u32)
            .expect("corner")
            .demultiply();
        assert!(
            (corner.blue() as i16) >= (corner.red() as i16),
            "selected tool must not keep a red corner mark, got {},{},{}",
            corner.red(),
            corner.green(),
            corner.blue()
        );
        let chip = tb.extras.iter().find(|e| e.checked);
        if let Some(chip) = chip {
            let cpx = pm
                .pixel((chip.rect.x + 3.0) as u32, (chip.rect.y + 3.0) as u32)
                .expect("chip")
                .demultiply();
            assert!(
                (cpx.blue() as i16) > (cpx.red() as i16),
                "checked chip should be accent blue, got {},{},{}",
                cpx.red(),
                cpx.green(),
                cpx.blue()
            );
        }
        if let Ok(dir) = std::env::var("PINSHOT_DUMP_UI") {
            let _ = std::fs::create_dir_all(&dir);
            let crop = tb.bar.union(tb.panel.unwrap_or(tb.bar)).inflated(12.0);
            let cw = crop.w.ceil().max(8.0) as u32;
            let ch = crop.h.ceil().max(8.0) as u32;
            if let Some(mut out) = Pixmap::new(cw, ch) {
                out.fill(tiny_skia::Color::from_rgba8(60, 64, 72, 255));
                let mut full = Pixmap::new(1920, 1200).unwrap();
                full.fill(tiny_skia::Color::from_rgba8(60, 64, 72, 255));
                paint_toolbar(&mut full, &tb, None);
                let x0 = crop.x.max(0.0) as i32;
                let y0 = crop.y.max(0.0) as i32;
                out.draw_pixmap(
                    -x0,
                    -y0,
                    full.as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
                let _ = out.save_png(format!("{dir}/toolbar_text.png"));
            }
            let rst = Style::for_tool(Tool::Rect);
            let rtb = Toolbar::layout(
                sel,
                over,
                Some(Tool::Rect),
                pinshot_core::ArrowKind::None,
                Some(&rst),
            );
            let crop = rtb.bar.union(rtb.panel.unwrap_or(rtb.bar)).inflated(12.0);
            if let Some(out) = crop_rect(
                &{
                    let mut full = Pixmap::new(1920, 1200).unwrap();
                    full.fill(tiny_skia::Color::from_rgba8(60, 64, 72, 255));
                    paint_toolbar(&mut full, &rtb, None);
                    full
                },
                crop,
                [60, 64, 72],
            ) {
                let _ = out.save_png(format!("{dir}/toolbar_rect.png"));
            }
            let est = Style::for_tool(Tool::Ellipse);
            let etb = Toolbar::layout(
                sel,
                over,
                Some(Tool::Ellipse),
                pinshot_core::ArrowKind::None,
                Some(&est),
            );
            let crop = etb.bar.union(etb.panel.unwrap_or(etb.bar)).inflated(12.0);
            if let Some(out) = crop_rect(
                &{
                    let mut full = Pixmap::new(1920, 1200).unwrap();
                    full.fill(tiny_skia::Color::from_rgba8(60, 64, 72, 255));
                    paint_toolbar(&mut full, &etb, None);
                    full
                },
                crop,
                [60, 64, 72],
            ) {
                let _ = out.save_png(format!("{dir}/toolbar_ellipse.png"));
            }
            let lst = Style::for_tool(Tool::Line);
            let mut ltb = Toolbar::layout(
                sel,
                over,
                Some(Tool::Line),
                pinshot_core::ArrowKind::None,
                Some(&lst),
            );
            ltb.attach_line_menu(over, &lst);
            let crop = ltb
                .bar
                .union(ltb.panel.unwrap_or(ltb.bar))
                .union(ltb.menu.as_ref().map(|m| m.rect).unwrap_or(ltb.bar))
                .inflated(12.0);
            if let Some(out) = crop_rect(
                &{
                    let mut full = Pixmap::new(1920, 1200).unwrap();
                    full.fill(tiny_skia::Color::from_rgba8(60, 64, 72, 255));
                    paint_toolbar(&mut full, &ltb, None);
                    full
                },
                crop,
                [60, 64, 72],
            ) {
                let _ = out.save_png(format!("{dir}/toolbar_line.png"));
            }
        }
    }

    fn crop_rect(src: &Pixmap, crop: Rect, fill: [u8; 3]) -> Option<Pixmap> {
        let cw = crop.w.ceil().max(8.0) as u32;
        let ch = crop.h.ceil().max(8.0) as u32;
        let mut out = Pixmap::new(cw, ch)?;
        out.fill(tiny_skia::Color::from_rgba8(fill[0], fill[1], fill[2], 255));
        let x0 = crop.x.max(0.0) as i32;
        let y0 = crop.y.max(0.0) as i32;
        out.draw_pixmap(
            -x0,
            -y0,
            src.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        Some(out)
    }

    fn luma_bins(pm: &Pixmap, r: Rect) -> u32 {
        let mut bins = [0u32; 16];
        let x0 = r.x.max(0.0) as u32;
        let y0 = r.y.max(0.0) as u32;
        let x1 = r.right().min(pm.width() as f32).max(0.0) as u32;
        let y1 = r.bottom().min(pm.height() as f32).max(0.0) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let p = pm.pixel(x, y).expect("px").demultiply();
                let luma = (p.red() as u16 + p.green() as u16 + p.blue() as u16) / 3;
                bins[(luma / 16) as usize] += 1;
            }
        }
        bins.iter().filter(|&&c| c > 0).count() as u32
    }

    fn dump_ui_dir() -> std::path::PathBuf {
        let dir = std::env::var("PINSHOT_DUMP_UI")
            .unwrap_or_else(|_| "/opt/cursor/artifacts/chrome-aa".into());
        let _ = std::fs::create_dir_all(&dir);
        std::path::PathBuf::from(dir)
    }

    #[test]
    fn chrome_chip_and_menu_text_is_antialiased() {
        use crate::toolbar::Toolbar;
        use pinshot_core::annotation::{Style, Tool};
        use pinshot_core::font::chrome_font_paths;
        use pinshot_core::i18n::tr;

        let sel = Rect::new(200.0, 160.0, 520.0, 280.0);
        let over = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let st = Style::for_tool(Tool::Text);
        let tb = Toolbar::layout(
            sel,
            over,
            Some(Tool::Text),
            pinshot_core::ArrowKind::None,
            Some(&st),
        );
        let mut pm = Pixmap::new(1920, 1200).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(60, 64, 72, 255));
        paint_toolbar(&mut pm, &tb, None);

        let outline = tb
            .extras
            .iter()
            .find(|e| e.icon == Some(IconKind::TextOutline))
            .expect("outline chip");
        let font = tb
            .extras
            .iter()
            .find(|e| e.kind == crate::toolbar::ExtraKind::Combo)
            .expect("font combo");
        let bold = tb
            .extras
            .iter()
            .find(|e| e.icon == Some(IconKind::Bold))
            .expect("bold chip");
        let bins_outline = luma_bins(&pm, outline.rect.inflated(-2.0));
        let bins_font = luma_bins(&pm, font.rect.inflated(-2.0));
        let bins_bold = luma_bins(&pm, bold.rect.inflated(-2.0));
        assert!(
            bins_outline >= 6 && bins_font >= 6 && bins_bold >= 4,
            "text chrome needs smooth luma ramps, got outline={bins_outline} font={bins_font} bold={bins_bold} fonts={:?}",
            chrome_font_paths()
        );

        let mut menu = Pixmap::new(280, 220).unwrap();
        menu.fill(tiny_skia::Color::from_rgba8(248, 248, 250, 255));
        let rows = [
            tr("New group…"),
            tr("Click-through"),
            tr("Shadow"),
            tr("Close pin"),
            tr("Zoom"),
        ];
        for (i, lab) in rows.iter().enumerate() {
            crate::text::blit_into_pm(
                &mut menu,
                lab,
                16.0,
                14.0 + i as f32 * 36.0,
                12,
                CHROME_TEXT,
                false,
            );
        }
        let menu_bins = luma_bins(&menu, Rect::new(8.0, 8.0, 240.0, 200.0));
        assert!(
            menu_bins >= 6,
            "pin-menu Chinese labels need AA, bins={menu_bins}"
        );

        let dir = dump_ui_dir();
        if let Some(panel) = tb.panel {
            let crop = panel.inflated(8.0);
            if let Some(out) = crop_rect(&pm, crop, [60, 64, 72]) {
                let _ = out.save_png(dir.join("texttool-chips.png"));
            }
            let bar_panel = tb.bar.union(panel).inflated(12.0);
            if let Some(out) = crop_rect(&pm, bar_panel, [60, 64, 72]) {
                let _ = out.save_png(dir.join("toolbar-text.png"));
            }
            if let Some(out) = crop_rect(&pm, outline.rect.inflated(4.0), [60, 64, 72]) {
                let _ = out.save_png(dir.join("chip-outline.png"));
            }
        }
        let _ = menu.save_png(dir.join("pin-menu-labels.png"));
    }

    #[test]
    fn magnifier_paints_reference_tool_crosshair() {
        let mut shot = gray_shot(80, 80, 40);
        shot.set(40, 40, Rgba::rgb(200, 40, 40));
        let cursor = Point::new(40.0, 40.0);
        let mag = layout_magnifier(
            Point::new(8.0, 8.0),
            Rect::new(0.0, 0.0, 400.0, 400.0),
            Rgba::rgb(40, 40, 40),
            Point::new(40.0, 40.0),
        );
        let mut pm = Pixmap::new(400, 400).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(32, 32, 32, 255));
        let dirty = paint_magnifier(&mut pm, &shot, cursor, 1.0, true, mag);
        assert!(dirty.w >= mag.grid.w);
        let mid = MAG_N / 2;
        let z = MAG_ZOOM;
        let hx = mag.grid.x as i32 + mid * z + z / 2;
        let hy = mag.grid.y as i32 + mid * z + z / 2;
        let top = pm
            .pixel(hx as u32, (mag.grid.y as i32 + 4) as u32)
            .unwrap()
            .demultiply();
        assert!(
            top.blue() > 180 && top.red() < 80,
            "vertical crosshair should be blue, got {} {} {}",
            top.red(),
            top.green(),
            top.blue()
        );
        let left = pm
            .pixel((mag.grid.x as i32 + 4) as u32, hy as u32)
            .unwrap()
            .demultiply();
        assert!(
            left.blue() > 180 && left.red() < 80,
            "horizontal crosshair should be blue, got {} {} {}",
            left.red(),
            left.green(),
            left.blue()
        );
        let hint_y = (mag.info.y + 54.0) as u32;
        let mut best_x = 0u32;
        let mut best_luma = 0u16;
        let x0 = mag.info.x.max(0.0) as u32;
        let x1 = mag.info.right().min(pm.width() as f32) as u32;
        for x in x0..x1 {
            let p = pm.pixel(x, hint_y).unwrap().demultiply();
            let luma = p.red() as u16 + p.green() as u16 + p.blue() as u16;
            if luma > best_luma {
                best_luma = luma;
                best_x = x;
            }
        }
        let mid = mag.info.x + mag.info.w * 0.5;
        assert!(
            best_luma > 400,
            "hint glyphs should be larger/brighter, luma={best_luma}"
        );
        assert!(
            (best_x as f32 - mid).abs() < mag.info.w * 0.28,
            "hint text should sit near the info panel center, best_x={best_x} mid={mid}"
        );
        if let Ok(dir) = std::env::var("PINSHOT_DUMP_UI") {
            let crop = mag.grid.union(mag.info).inflated(8.0);
            if let Some(out) = crop_rect(&pm, crop, [32, 32, 32]) {
                let _ = out.save_png(format!("{dir}/magnifier.png"));
            }
        }
    }
}

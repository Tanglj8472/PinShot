//! tiny-skia overlay / pin / toolbar painter.
//!
//! Overlay frames are composited incrementally: the 6k-class virtual desktop is
//! *not* rebuilt on every mouse move. Only the hole ring + chrome (magnifier,
//! size label, toolbar) are copied, then BitBlt'd to the HWND.

use std::ffi::c_void;

use pinshot_core::annotation::{Annotator, Tool};
use pinshot_core::color::Rgba;
use pinshot_core::geom::{handle_points, Point, Rect, HANDLE_SIZE};
use pinshot_core::i18n::tr;
use pinshot_core::icons::{draw_icon, IconKind};
use pinshot_core::magnifier::{layout_magnifier, Magnifier, MAG_N, MAG_ZOOM};
use pinshot_core::raster::Raster;
use pinshot_core::render::{compose, paint_shapes};
use pinshot_core::selection::{SelState, Selection};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Stroke, Transform};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    RGBQUAD, SRCCOPY,
};

use crate::text::blit_text;
use crate::toolbar::Toolbar;

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c.to_skia());
    p.anti_alias = true;
    p
}

fn fill_rect(pm: &mut Pixmap, r: Rect, c: Rgba) {
    if r.is_empty() {
        return;
    }
    if let Some(sr) = tiny_skia::Rect::from_xywh(r.x, r.y, r.w.max(0.5), r.h.max(0.5)) {
        pm.fill_rect(sr, &paint(c), Transform::identity(), None);
    }
}

fn stroke_rect(pm: &mut Pixmap, r: Rect, c: Rgba, width: f32) {
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

fn clip_rect(r: Rect, bounds: Rect) -> Rect {
    r.intersect(bounds)
}

fn copy_rect(dst: &mut Pixmap, src: &Pixmap, r: Rect) {
    let bounds = Rect::new(0.0, 0.0, dst.width() as f32, dst.height() as f32)
        .intersect(Rect::new(0.0, 0.0, src.width() as f32, src.height() as f32));
    let r = clip_rect(r, bounds);
    if r.is_empty() {
        return;
    }
    let x0 = r.x.floor().max(0.0) as u32;
    let y0 = r.y.floor().max(0.0) as u32;
    let x1 = r.right().ceil().min(dst.width() as f32).min(src.width() as f32) as u32;
    let y1 = r.bottom().ceil().min(dst.height() as f32).min(src.height() as f32) as u32;
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
    let Some(src) = img.to_skia() else {
        return;
    };
    let sx = if dest.w > 0.0 { dest.w / img.width as f32 } else { 1.0 };
    let sy = if dest.h > 0.0 { dest.h / img.height as f32 } else { 1.0 };
    let t = Transform::from_translate(dest.x, dest.y).pre_scale(sx, sy);
    pm.draw_pixmap(0, 0, src.as_ref(), &tiny_skia::PixmapPaint::default(), t, None);
}

/// Persistent overlay compositor: cached capture + dirty GDI present.
pub struct OverlayFront {
    shot: Pixmap,
    frame: Pixmap,
    gdi: GdiBits,
    dim: u8,
    last_hole: Rect,
    last_chrome: Vec<Rect>,
    primed: bool,
}

struct GdiBits {
    hdc: HDC,
    hbmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
    stride: i32,
}

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
        let bounds = Rect::new(0.0, 0.0, self.w as f32, self.h as f32)
            .intersect(Rect::new(0.0, 0.0, pm.width() as f32, pm.height() as f32));
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

impl Drop for GdiBits {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.hbmp.0));
            let _ = DeleteDC(self.hdc);
        }
    }
}

impl OverlayFront {
    pub fn new(shot: &Raster, dim: u8) -> Result<Self, String> {
        let shot_pm = shot.to_skia().ok_or("capture pixmap")?;
        let mut frame = shot_pm.clone();
        let bounds = Rect::new(0.0, 0.0, frame.width() as f32, frame.height() as f32);
        fill_rect(&mut frame, bounds, Rgba::rgba(0, 0, 0, dim));
        let gdi = GdiBits::new(frame.width(), frame.height())?;
        Ok(Self {
            shot: shot_pm,
            frame,
            gdi,
            dim,
            last_hole: Rect::EMPTY,
            last_chrome: Vec::new(),
            primed: false,
        })
    }

    fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.frame.width() as f32, self.frame.height() as f32)
    }

    /// Swap the frozen desktop (history replay). Same virtual size expected.
    pub fn replace_shot(&mut self, shot: &Raster, hwnd: HWND) -> Result<(), String> {
        let shot_pm = shot.to_skia().ok_or("capture pixmap")?;
        if shot_pm.width() != self.frame.width() || shot_pm.height() != self.frame.height() {
            *self = Self::new(shot, self.dim)?;
            return Ok(());
        }
        self.shot = shot_pm;
        self.frame = self.shot.clone();
        let bounds = Rect::new(0.0, 0.0, self.frame.width() as f32, self.frame.height() as f32);
        fill_rect(&mut self.frame, bounds, Rgba::rgba(0, 0, 0, self.dim));
        self.last_hole = Rect::EMPTY;
        self.last_chrome.clear();
        self.primed = false;
        self.gdi.write_rect(&self.frame, bounds);
        self.gdi.blit_rect(hwnd, bounds);
        self.primed = true;
        Ok(())
    }

    fn restore(&mut self, r: Rect, hole: Rect) {
        let r = clip_rect(r, self.bounds());
        if r.is_empty() {
            return;
        }
        copy_rect(&mut self.frame, &self.shot, r);
        let dim_c = Rgba::rgba(0, 0, 0, self.dim);
        for band in r.subtract(hole) {
            fill_rect(&mut self.frame, band, dim_c);
        }
    }

    pub fn paint(
        &mut self,
        hwnd: HWND,
        sel: &Selection,
        hover: Rect,
        cursor: Point,
        dpr: f32,
        hex: bool,
        mag_on: bool,
        annot: &Annotator,
        toolbar: Option<&Toolbar>,
        toast: Option<&str>,
        shot: &Raster,
    ) {
        let bounds = self.bounds();
        let hole = if sel.has_sel() {
            sel.normalized()
        } else if sel.state == SelState::Idle {
            hover
        } else {
            sel.normalized()
        };
        let hole = clip_rect(hole, bounds);
        let hole_pad = hole.inflated(8.0).intersect(bounds);
        let fw = self.frame.width();
        let fh = self.frame.height();

        let mut present: Vec<Rect> = Vec::new();
        if !self.primed {
            present.push(bounds);
        }

        // Symmetric difference of the (padded) hole — thin bands, not the whole desktop.
        for r in self.last_hole.subtract(hole) {
            if !r.is_empty() {
                self.restore(r, hole);
                present.push(r);
            }
        }
        for r in hole_pad.subtract(self.last_hole) {
            if !r.is_empty() {
                self.restore(r, hole);
                present.push(r);
            }
        }
        // Old stroke / handles sit on the previous rim.
        if !self.last_hole.is_empty() {
            let rim = self.last_hole.inflated(8.0).intersect(bounds);
            for r in rim.subtract(self.last_hole.inflated(-2.0)) {
                if !r.is_empty() {
                    self.restore(r, hole);
                    present.push(r);
                }
            }
        }
        for r in std::mem::take(&mut self.last_chrome) {
            self.restore(r, hole);
            present.push(r);
        }

        // Hole interior: undimmed capture + vector annotations (no full compose).
        if !hole.is_empty() {
            copy_rect(&mut self.frame, &self.shot, hole);
            let shapes: Vec<_> = annot.all_shapes().cloned().collect();
            if !shapes.is_empty() {
                let needs_fx = shapes
                    .iter()
                    .any(|s| matches!(s.kind, Tool::Mosaic | Tool::Eraser));
                if needs_fx {
                    let composed = compose(shot, &shapes);
                    if let Some(src) = composed.to_skia() {
                        copy_rect(&mut self.frame, &src, hole);
                    }
                } else {
                    paint_shapes(&mut self.frame, &shapes);
                }
            }
        }

        let mut chrome: Vec<Rect> = Vec::new();
        for sh in annot.all_shapes() {
            let pad = sh.style.width.max(8) as f32 + 8.0;
            chrome.push(sh.bounds().inflated(pad).intersect(bounds));
        }
        if !hole.is_empty() {
            stroke_rect(
                &mut self.frame,
                hole.adjusted(0.0, 0.0, -1.0, -1.0),
                Rgba::BLUE,
                1.0,
            );
            if sel.has_sel() && annot.tool.is_none() {
                for (_, c) in handle_points(hole) {
                    let hr = Rect::new(
                        c.x - HANDLE_SIZE / 2.0,
                        c.y - HANDLE_SIZE / 2.0,
                        HANDLE_SIZE,
                        HANDLE_SIZE,
                    );
                    fill_rect(&mut self.frame, hr, Rgba::BLUE);
                    stroke_rect(&mut self.frame, hr, Rgba::WHITE, 1.0);
                }
            }
            let phys = Rect::new(hole.x * dpr, hole.y * dpr, hole.w * dpr, hole.h * dpr);
            chrome.push(paint_size_label(
                &mut self.frame,
                hole,
                &phys.size_label(),
                fw,
                fh,
            ));
            chrome.push(hole_pad);
        }

        if mag_on {
            let (color, phys) = pinshot_core::sample_color(shot, cursor, dpr);
            let mag = layout_magnifier(cursor, bounds, color, phys);
            chrome.push(paint_magnifier(&mut self.frame, shot, cursor, dpr, hex, mag));
        }

        if let Some(tb) = toolbar {
            if tb.visible {
                chrome.push(paint_toolbar(&mut self.frame, tb));
            }
        }

        if let Some(t) = toast {
            chrome.push(paint_toast(&mut self.frame, t, fw, fh));
        }

        for r in &chrome {
            if !r.is_empty() {
                present.push(*r);
            }
        }
        self.last_hole = hole;
        self.last_chrome = chrome;

        if !self.primed {
            self.gdi.write_rect(&self.frame, bounds);
            self.gdi.blit_rect(hwnd, bounds);
            self.primed = true;
        } else {
            for r in present {
                let r = clip_rect(r, bounds);
                if r.is_empty() {
                    continue;
                }
                self.gdi.write_rect(&self.frame, r);
                self.gdi.blit_rect(hwnd, r);
            }
        }
        let _ = FillRule::Winding;
    }

    /// Replay the already-composited DIB (WM_PAINT / uncover). No software rebuild.
    pub fn present_full(&self, hwnd: HWND) {
        if self.primed {
            self.gdi.blit_rect(hwnd, self.bounds());
        }
    }
}

fn paint_size_label(pm: &mut Pixmap, hole: Rect, label: &str, ow: u32, oh: u32) -> Rect {
    let lw = (label.len() as f32) * 7.5 + 12.0;
    let ly = if hole.y >= 22.0 { hole.y - 22.0 } else { hole.y + 2.0 };
    let lx = if hole.x + lw < ow as f32 {
        hole.x
    } else {
        ow as f32 - lw
    };
    let boxr = Rect::new(lx, ly, lw, 20.0);
    fill_rect(pm, boxr, Rgba::rgba(0, 0, 0, 170));
    blit_into(pm, label, lx as i32 + 6, ly as i32 + 3, 12, Rgba::WHITE, false);
    let _ = oh;
    boxr.inflated(1.0)
}

fn paint_magnifier(
    pm: &mut Pixmap,
    shot: &Raster,
    cursor: Point,
    dpr: f32,
    hex: bool,
    mag: Magnifier,
) -> Rect {
    fill_rect(pm, mag.grid, Rgba::BLACK);
    let cx = (cursor.x * dpr).round() as i32;
    let cy = (cursor.y * dpr).round() as i32;
    let n = MAG_N;
    let z = MAG_ZOOM as f32;
    for j in 0..n {
        for i in 0..n {
            let c = shot.get_clamped(cx + i - n / 2, cy + j - n / 2);
            fill_rect(
                pm,
                Rect::new(mag.grid.x + i as f32 * z, mag.grid.y + j as f32 * z, z, z),
                c,
            );
        }
    }
    stroke_rect(pm, mag.grid, Rgba::WHITE, 1.0);
    let c0 = (n / 2) as f32 * z;
    stroke_rect(
        pm,
        Rect::new(mag.grid.x + c0, mag.grid.y + c0, z, z),
        Rgba::rgba(30, 144, 255, 220),
        1.0,
    );
    fill_rect(pm, mag.info, Rgba::rgba(0, 0, 0, 210));
    let coord = format!("({}, {})", mag.phys.x as i32, mag.phys.y as i32);
    blit_into(
        pm,
        &coord,
        mag.info.x as i32 + 8,
        mag.info.y as i32 + 4,
        11,
        Rgba::WHITE,
        false,
    );
    fill_rect(
        pm,
        Rect::new(mag.info.x + 8.0, mag.info.y + 20.0, 10.0, 10.0),
        mag.color,
    );
    blit_into(
        pm,
        &mag.color.format(hex),
        mag.info.x as i32 + 22,
        mag.info.y as i32 + 18,
        11,
        Rgba::WHITE,
        false,
    );
    blit_into(
        pm,
        &tr("Press C to copy color"),
        mag.info.x as i32 + 4,
        mag.info.y as i32 + 34,
        10,
        Rgba::rgb(210, 210, 210),
        false,
    );
    blit_into(
        pm,
        &tr("Shift: switch RGB/HEX"),
        mag.info.x as i32 + 4,
        mag.info.y as i32 + 48,
        10,
        Rgba::rgb(210, 210, 210),
        false,
    );
    mag.grid.union(mag.info).inflated(2.0)
}

pub fn paint_toolbar(pm: &mut Pixmap, tb: &Toolbar) -> Rect {
    fill_rect(pm, tb.bar, Rgba::rgb(247, 247, 247));
    stroke_rect(pm, tb.bar, Rgba::rgb(154, 154, 154), 1.0);
    for b in &tb.buttons {
        if b.checked {
            fill_rect(pm, b.rect, Rgba::rgb(196, 217, 242));
        }
        draw_icon(pm, b.icon, b.rect.adjusted(3.0, 3.0, -3.0, -3.0), None);
    }
    let mut dirty = tb.bar;
    if let Some(pr) = tb.panel {
        fill_rect(pm, pr, Rgba::rgb(247, 247, 247));
        stroke_rect(pm, pr, Rgba::rgb(154, 154, 154), 1.0);
        for (r, c, _) in &tb.swatches {
            fill_rect(pm, *r, *c);
            stroke_rect(pm, *r, Rgba::rgb(136, 136, 136), 1.0);
        }
        dirty = dirty.union(pr);
    }
    let _ = IconKind::Rect;
    dirty.inflated(1.0)
}

fn paint_toast(pm: &mut Pixmap, text: &str, w: u32, h: u32) -> Rect {
    let tw = text.chars().count() as f32 * 9.0 + 28.0;
    let boxr = Rect::new((w as f32 - tw) / 2.0, h as f32 / 2.0 - 22.0, tw, 44.0);
    fill_rect(pm, boxr, Rgba::rgba(20, 20, 20, 220));
    blit_into(pm, text, boxr.x as i32 + 14, boxr.y as i32 + 12, 16, Rgba::WHITE, true);
    boxr.inflated(2.0)
}

fn blit_into(pm: &mut Pixmap, text: &str, x: i32, y: i32, px: i32, color: Rgba, bold: bool) {
    if text.is_empty() {
        return;
    }
    let (tw, th) = crate::text::measure(text, px, bold);
    if tw == 0 || th == 0 {
        return;
    }
    let mut rgba = vec![0u8; tw as usize * th as usize * 4];
    blit_text(&mut rgba, tw, tw, th, text, 0, 0, px, color, bold);
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
        x,
        y,
        src.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        None,
    );
}

pub fn paint_pin(img: &Raster, annot: &Annotator, scale: f32, click_through: bool, glow: f32) -> Pixmap {
    let w = ((img.width as f32) * scale).round().max(8.0) as u32;
    let h = ((img.height as f32) * scale).round().max(8.0) as u32;
    let mut pm = Pixmap::new(w, h).unwrap_or_else(|| Pixmap::new(8, 8).unwrap());
    let composed = compose(img, &annot.shapes);
    blit_raster(&mut pm, &composed, Rect::new(0.0, 0.0, w as f32, h as f32));
    if glow > 0.0 {
        stroke_rect(
            &mut pm,
            Rect::new(0.5, 0.5, w as f32 - 1.0, h as f32 - 1.0),
            Rgba::rgba(40, 220, 90, (255.0 * glow) as u8),
            3.0,
        );
    } else if click_through {
        stroke_rect(
            &mut pm,
            Rect::new(0.5, 0.5, w as f32 - 1.0, h as f32 - 1.0),
            Rgba::rgba(180, 40, 220, 200),
            2.0,
        );
        fill_rect(
            &mut pm,
            Rect::new(w as f32 - 88.0, 2.0, 86.0, 18.0),
            Rgba::rgba(180, 40, 220, 200),
        );
        blit_into(&mut pm, &tr("CLICK-THRU"), w as i32 - 84, 4, 10, Rgba::WHITE, true);
    } else {
        let col = if annot.tool.is_some() {
            Rgba::rgb(255, 140, 0)
        } else {
            Rgba::rgba(30, 144, 255, 150)
        };
        stroke_rect(&mut pm, Rect::new(0.5, 0.5, w as f32 - 1.0, h as f32 - 1.0), col, 1.0);
    }
    let _ = Tool::Rect;
    pm
}

pub fn present(buf: &mut [u32], pm: &Pixmap) {
    let data = pm.data();
    let n = buf.len().min(data.len() / 4);
    for i in 0..n {
        let o = i * 4;
        buf[i] = (data[o] as u32) << 16 | (data[o + 1] as u32) << 8 | data[o + 2] as u32;
    }
}

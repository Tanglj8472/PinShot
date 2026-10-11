//! GDI text → RGBA so overlay hints / toolbar labels can be Chinese.

use pinshot_core::color::Rgba;
use pinshot_core::geom::Rect;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DRAWTEXTPARAMS, DrawTextExW, FW_BOLD, FW_NORMAL,
    HBITMAP, HGDIOBJ, OUT_TT_PRECIS, RGBQUAD, TRANSPARENT,
};
use windows::Win32::Graphics::Gdi::DT_LEFT;

use crate::win32::wide;

pub fn measure(text: &str, px: i32, _bold: bool) -> (u32, u32) {
    // Approximate; GDI DrawText can refine when we actually blit.
    let w = text.chars().count() as u32 * (px as u32 * 3 / 5).max(6);
    (w + 8, px as u32 + 6)
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
    bold: bool,
) {
    if text.is_empty() {
        return;
    }
    let (tw, th) = measure(text, px, bold);
    let Some(rgba) = rasterize(text, tw, th.max(px as u32 + 4), px, color, bold) else {
        return;
    };
    for yy in 0..th {
        for xx in 0..tw {
            let i = ((yy * tw + xx) * 4) as usize;
            if i + 3 >= rgba.len() {
                continue;
            }
            let a = rgba[i + 3];
            if a == 0 {
                continue;
            }
            let dx = x + xx as i32;
            let dy = y + yy as i32;
            if dx < 0 || dy < 0 || dx >= dest_w as i32 || dy >= dest_h as i32 {
                continue;
            }
            let di = (dy as u32 * stride + dx as u32) * 4;
            let di = di as usize;
            if di + 3 >= pixels.len() {
                continue;
            }
            let ia = 255 - a as u16;
            pixels[di] = ((rgba[i] as u16 * a as u16 + pixels[di] as u16 * ia) / 255) as u8;
            pixels[di + 1] =
                ((rgba[i + 1] as u16 * a as u16 + pixels[di + 1] as u16 * ia) / 255) as u8;
            pixels[di + 2] =
                ((rgba[i + 2] as u16 * a as u16 + pixels[di + 2] as u16 * ia) / 255) as u8;
            pixels[di + 3] = 255;
        }
    }
    let _ = Rect::EMPTY;
}

fn rasterize(text: &str, w: u32, h: u32, px: i32, color: Rgba, bold: bool) -> Option<Vec<u8>> {
    unsafe {
        let hdc_screen = GetDC(HWND::default());
        let hdc = CreateCompatibleDC(hdc_screen);
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
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbmp = CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            let _ = DeleteDC(hdc);
            ReleaseDC(HWND::default(), hdc_screen);
            return None;
        }
        let old_bmp = SelectObject(hdc, HGDIOBJ(hbmp.0));
        let face = wide("Microsoft YaHei UI");
        let font = CreateFontW(
            px,
            0,
            0,
            0,
            if bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_TT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            0,
            DEFAULT_PITCH.0 as u32,
            windows::core::PCWSTR(face.as_ptr()),
        );
        let old_font = SelectObject(hdc, HGDIOBJ(font.0));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(
            hdc,
            windows::Win32::Foundation::COLORREF(
                color.r as u32 | (color.g as u32) << 8 | (color.b as u32) << 16,
            ),
        );
        let mut wtext = wide(text);
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: w as i32,
            bottom: h as i32,
        };
        let mut params = DRAWTEXTPARAMS::default();
        params.cbSize = std::mem::size_of::<DRAWTEXTPARAMS>() as u32;
        let _ = DrawTextExW(hdc, &mut wtext, &mut rc, DT_LEFT, Some(&params));
        let src = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
        let mut out = vec![0u8; src.len()];
        for (o, s) in out.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            // DIB is BGRA; treat non-zero as glyph coverage.
            o[0] = color.r;
            o[1] = color.g;
            o[2] = color.b;
            let cover = s[0].max(s[1]).max(s[2]);
            o[3] = cover;
        }
        SelectObject(hdc, old_font);
        SelectObject(hdc, old_bmp);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        let _ = DeleteDC(hdc);
        ReleaseDC(HWND::default(), hdc_screen);
        let _ = HBITMAP::default();
        Some(out)
    }
}

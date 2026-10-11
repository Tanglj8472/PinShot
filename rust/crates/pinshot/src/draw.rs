//! tiny-skia overlay / pin / toolbar painter.

use pinshot_core::annotation::{Annotator, Tool};
use pinshot_core::color::Rgba;
use pinshot_core::geom::{handle_points, Point, Rect, HANDLE_SIZE};
use pinshot_core::i18n::tr;
use pinshot_core::icons::{draw_icon, IconKind};
use pinshot_core::magnifier::{layout_magnifier, Magnifier, MAG_N, MAG_ZOOM};
use pinshot_core::raster::Raster;
use pinshot_core::render::compose;
use pinshot_core::selection::{SelState, Selection};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::text::blit_text;
use crate::toolbar::Toolbar;

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c.to_skia());
    p.anti_alias = true;
    p
}

fn fill_rect(pm: &mut Pixmap, r: Rect, c: Rgba) {
    if let Some(sr) = tiny_skia::Rect::from_xywh(r.x, r.y, r.w.max(0.5), r.h.max(0.5)) {
        pm.fill_rect(sr, &paint(c), Transform::identity(), None);
    }
}

fn stroke_rect(pm: &mut Pixmap, r: Rect, c: Rgba, width: f32) {
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

pub fn blit_raster(pm: &mut Pixmap, img: &Raster, dest: Rect) {
    let Some(src) = img.to_skia() else {
        return;
    };
    let sx = if dest.w > 0.0 { dest.w / img.width as f32 } else { 1.0 };
    let sy = if dest.h > 0.0 { dest.h / img.height as f32 } else { 1.0 };
    let t = Transform::from_translate(dest.x, dest.y).pre_scale(sx, sy);
    pm.draw_pixmap(0, 0, src.as_ref(), &tiny_skia::PixmapPaint::default(), t, None);
}

pub fn paint_overlay(
    shot: &Raster,
    sel: &Selection,
    hover: Rect,
    cursor: Point,
    dpr: f32,
    hex: bool,
    mag_on: bool,
    annot: &Annotator,
    toolbar: Option<&Toolbar>,
    toast: Option<&str>,
    dim: u8,
) -> Pixmap {
    let w = shot.width;
    let h = shot.height;
    let mut pm = Pixmap::new(w, h).unwrap_or_else(|| Pixmap::new(1, 1).unwrap());
    blit_raster(&mut pm, shot, Rect::new(0.0, 0.0, w as f32, h as f32));

    let hole = if sel.has_sel() {
        sel.normalized()
    } else if sel.state == SelState::Idle {
        hover
    } else {
        sel.normalized()
    };

    // Dim everything except the hole.
    let dim_c = Rgba::rgba(0, 0, 0, dim);
    fill_rect(&mut pm, Rect::new(0.0, 0.0, w as f32, hole.y), dim_c);
    fill_rect(
        &mut pm,
        Rect::new(0.0, hole.bottom(), w as f32, h as f32 - hole.bottom()),
        dim_c,
    );
    fill_rect(&mut pm, Rect::new(0.0, hole.y, hole.x.max(0.0), hole.h), dim_c);
    fill_rect(
        &mut pm,
        Rect::new(hole.right(), hole.y, w as f32 - hole.right(), hole.h),
        dim_c,
    );

    let shapes: Vec<_> = annot.all_shapes().cloned().collect();
    if !shapes.is_empty() {
        let composed = compose(shot, &shapes);
        if let Some(src) = composed.to_skia() {
            pm.draw_pixmap(
                0,
                0,
                src.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
    }

    if !hole.is_empty() {
        stroke_rect(&mut pm, hole.adjusted(0.0, 0.0, -1.0, -1.0), Rgba::BLUE, 1.0);
        if sel.has_sel() && annot.tool.is_none() {
            for (_, c) in handle_points(hole) {
                fill_rect(
                    &mut pm,
                    Rect::new(c.x - HANDLE_SIZE / 2.0, c.y - HANDLE_SIZE / 2.0, HANDLE_SIZE, HANDLE_SIZE),
                    Rgba::BLUE,
                );
                stroke_rect(
                    &mut pm,
                    Rect::new(c.x - HANDLE_SIZE / 2.0, c.y - HANDLE_SIZE / 2.0, HANDLE_SIZE, HANDLE_SIZE),
                    Rgba::WHITE,
                    1.0,
                );
            }
        }
        let phys = Rect::new(hole.x * dpr, hole.y * dpr, hole.w * dpr, hole.h * dpr);
        paint_size_label(&mut pm, hole, &phys.size_label(), w, h);
    }

    if mag_on {
        let (color, phys) = pinshot_core::sample_color(shot, cursor, dpr);
        let mag = layout_magnifier(cursor, Rect::new(0.0, 0.0, w as f32, h as f32), color, phys);
        paint_magnifier(&mut pm, shot, cursor, dpr, hex, mag);
    }

    if let Some(tb) = toolbar {
        if tb.visible {
            paint_toolbar(&mut pm, tb);
        }
    }

    if let Some(t) = toast {
        paint_toast(&mut pm, t, w, h);
    }

    let _ = FillRule::Winding;
    pm
}

fn paint_size_label(pm: &mut Pixmap, hole: Rect, label: &str, ow: u32, oh: u32) {
    let lw = (label.len() as f32) * 7.5 + 12.0;
    let ly = if hole.y >= 22.0 { hole.y - 22.0 } else { hole.y + 2.0 };
    let lx = if hole.x + lw < ow as f32 {
        hole.x
    } else {
        ow as f32 - lw
    };
    fill_rect(pm, Rect::new(lx, ly, lw, 20.0), Rgba::rgba(0, 0, 0, 170));
    blit_into(pm, label, lx as i32 + 6, ly as i32 + 3, 12, Rgba::WHITE, false);
    let _ = oh;
}

fn paint_magnifier(pm: &mut Pixmap, shot: &Raster, cursor: Point, dpr: f32, hex: bool, mag: Magnifier) {
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
}

fn paint_toolbar(pm: &mut Pixmap, tb: &Toolbar) {
    fill_rect(pm, tb.bar, Rgba::rgb(247, 247, 247));
    stroke_rect(pm, tb.bar, Rgba::rgb(154, 154, 154), 1.0);
    for b in &tb.buttons {
        if b.checked {
            fill_rect(pm, b.rect, Rgba::rgb(196, 217, 242));
        }
        draw_icon(pm, b.icon, b.rect.adjusted(3.0, 3.0, -3.0, -3.0), None);
    }
    if let Some(pr) = tb.panel {
        fill_rect(pm, pr, Rgba::rgb(247, 247, 247));
        stroke_rect(pm, pr, Rgba::rgb(154, 154, 154), 1.0);
        for (r, c, _) in &tb.swatches {
            fill_rect(pm, *r, *c);
            stroke_rect(pm, *r, Rgba::rgb(136, 136, 136), 1.0);
        }
    }
    let _ = IconKind::Rect;
}

fn paint_toast(pm: &mut Pixmap, text: &str, w: u32, h: u32) {
    let tw = text.chars().count() as f32 * 9.0 + 28.0;
    let boxr = Rect::new((w as f32 - tw) / 2.0, h as f32 / 2.0 - 22.0, tw, 44.0);
    fill_rect(pm, boxr, Rgba::rgba(20, 20, 20, 220));
    blit_into(pm, text, boxr.x as i32 + 14, boxr.y as i32 + 12, 16, Rgba::WHITE, true);
}

fn blit_into(pm: &mut Pixmap, text: &str, x: i32, y: i32, px: i32, color: Rgba, bold: bool) {
    let w = pm.width();
    let h = pm.height();
    // Convert premultiplied pixmap to straight-ish bytes, blit, write back via pixels_mut.
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for pxel in pm.pixels() {
        let c = pxel.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    blit_text(&mut rgba, w, w, h, text, x, y, px, color, bold);
    for (dst, src) in pm.pixels_mut().iter_mut().zip(rgba.chunks_exact(4)) {
        *dst = tiny_skia::PremultipliedColorU8::from_rgba(
            ((src[0] as u16 * src[3] as u16) / 255) as u8,
            ((src[1] as u16 * src[3] as u16) / 255) as u8,
            ((src[2] as u16 * src[3] as u16) / 255) as u8,
            src[3],
        )
        .unwrap_or(*dst);
    }
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
    let n = buf.len().min(pm.pixels().len());
    for i in 0..n {
        let c = pm.pixels()[i].demultiply();
        buf[i] = (c.red() as u32) << 16 | (c.green() as u32) << 8 | c.blue() as u32;
    }
}

//! Screen capture: DXGI Desktop Duplication, then GDI BitBlt / per-monitor / PrintWindow.
//!
//! Must run *before* the overlay is created. All-black frames are rejected so a
//! timed-out DXGI first frame cannot hide a working GDI fallback.
//!
//! 参考工具 stays fast because it keeps a live `DuplicateOutput` session. Creating
//! a D3D device + duplicator on every F1, then waiting 350ms×8 for the first
//! frame, was the bulk of PinShot's "Rust is slower than C++" delay.

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use pinshot_core::geom::Rect;
use pinshot_core::raster::Raster;
use windows::core::{w, Interface, PCWSTR};
use windows::Win32::Foundation::{BOOL, HMODULE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication,
    IDXGIResource, DXGI_ERROR_ACCESS_DENIED, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_INVALID_CALL,
    DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTPUT_DESC,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDCW, CreateDIBSection, DeleteDC, DeleteObject,
    EnumDisplayMonitors, GetDC, GetMonitorInfoW, ReleaseDC, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HDC, HGDIOBJ, HMONITOR, MONITORINFO,
    MONITORINFOEXW, RGBQUAD, ROP_CODE, SRCCOPY,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::WindowsAndMessaging::{
    GetDesktopWindow, GetSystemMetrics, PW_RENDERFULLCONTENT, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

#[derive(Clone, Debug)]
pub struct ScreenFrame {
    pub image: Raster,
    pub physical: Rect,
    pub scale: f32,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Capture {
    pub frames: Vec<ScreenFrame>,
    pub virtual_origin: (i32, i32),
    pub composed: Raster,
    pub backend: String,
}

impl Capture {
    /// Capture the virtual desktop *before* any overlay window exists.
    ///
    /// Order: DXGI (per-adapter, reject black) → GDI virtual BitBlt →
    /// per-monitor CreateDC → PrintWindow. `--dump-capture` / `PINSHOT_DUMP_CAPTURE`
    /// writes the composed buffer to `%TEMP%\pinshot-capture.png`.
    pub fn grab(dump: bool) -> Result<Self, String> {
        let t0 = Instant::now();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let cap = grab_with_fallback(dump, false);
        crate::logutil::ms("capture grab", t0);
        cap
    }

    /// Capture after a resnip. Desktop Duplication can expose one compositor
    /// frame that still contains the just-closed overlay; discard that frame
    /// before composing the image shown to the user.
    pub fn grab_fresh(dump: bool) -> Result<Self, String> {
        let t0 = Instant::now();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let cap = grab_with_fallback(dump, true);
        crate::logutil::ms("capture grab fresh", t0);
        cap
    }

    /// Warm the DXGI duplicator on the UI thread so the first F1 is not a
    /// D3D11CreateDevice + DuplicateOutput stall.
    pub fn prefetch() -> bool {
        if dxgi_disabled().load(Ordering::Acquire) {
            crate::logutil::log_at("trace", "dxgi prefetch skipped: disabled after previous loss");
            return false;
        }
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let t0 = Instant::now();
        let ok = match grab_dxgi(false) {
            Ok(frames) => {
                crate::logutil::log(&format!(
                    "dxgi prefetch outputs={} {}×{}",
                    frames.len(),
                    frames.first().map(|f| f.image.width).unwrap_or(0),
                    frames.first().map(|f| f.image.height).unwrap_or(0)
                ));
                true
            }
            Err(e) => {
                crate::logutil::log(&format!("dxgi prefetch: {e}"));
                false
            }
        };
        crate::logutil::ms("dxgi prefetch", t0);
        ok
    }

    /// Drop the standing DXGI session after the overlay closes. Each output
    /// keeps a full-desktop RGBA `last` frame plus D3D staging textures;
    /// leaving that alive is ~200MB in Task Manager while the tray is idle.
    pub fn release_idle() {
        DXGI.with(|slot| {
            if slot.borrow().is_some() {
                *slot.borrow_mut() = None;
                crate::logutil::log("dxgi released");
            }
        });
        crate::win32::trim_working_set();
    }
}

fn grab_with_fallback(dump: bool, discard_dxgi_frame: bool) -> Result<Capture, String> {
    let mut tried: Vec<String> = Vec::new();

    if dxgi_disabled().load(Ordering::Acquire) {
        tried.push("dxgi: disabled after previous loss".into());
    } else {
        let t_dxgi = Instant::now();
        let dxgi = grab_dxgi(discard_dxgi_frame);
        crate::logutil::ms("capture dxgi", t_dxgi);
        match dxgi {
            Ok(frames) if !frames.is_empty() && !all_blank(&frames) => {
                return finish_ok(compose(frames, "dxgi"), dump, &tried);
            }
            Ok(frames) if !frames.is_empty() => tried.push("dxgi: all-black".into()),
            Ok(_) => tried.push("dxgi: no frames".into()),
            Err(e) => tried.push(format!("dxgi: {e}")),
        }
    }

    match grab_gdi_virtual() {
        Ok(frame) if !frame.image.is_visually_blank() => {
            return finish_ok(compose(vec![frame], "gdi-virtual"), dump, &tried);
        }
        Ok(_) => tried.push("gdi-virtual: all-black".into()),
        Err(e) => tried.push(format!("gdi-virtual: {e}")),
    }

    match grab_gdi_monitors() {
        Ok(frames) if !frames.is_empty() && !all_blank(&frames) => {
            return finish_ok(compose(frames, "gdi-monitors"), dump, &tried);
        }
        Ok(_) => tried.push("gdi-monitors: all-black/empty".into()),
        Err(e) => tried.push(format!("gdi-monitors: {e}")),
    }

    match grab_printwindow() {
        Ok(frame) if !frame.image.is_visually_blank() => {
            return finish_ok(compose(vec![frame], "gdi-printwindow"), dump, &tried);
        }
        Ok(_) => tried.push("gdi-printwindow: all-black".into()),
        Err(e) => tried.push(format!("gdi-printwindow: {e}")),
    }

    Err(format!("capture failed ({})", tried.join(" | ")))
}

fn finish_ok(cap: Capture, dump: bool, tried: &[String]) -> Result<Capture, String> {
    if !tried.is_empty() {
        crate::logutil::log(&format!("capture: fallback after {}", tried.join(" | ")));
    }
    crate::logutil::log(&format!(
        "capture: backend={} size={}x{}",
        cap.backend,
        cap.composed.width,
        cap.composed.height,
    ));
    if dump || std::env::var_os("PINSHOT_DUMP_CAPTURE").is_some() {
        dump_capture(&cap);
    }
    Ok(cap)
}

fn all_blank(frames: &[ScreenFrame]) -> bool {
    frames.iter().all(|f| f.image.is_visually_blank())
}

fn compose(frames: Vec<ScreenFrame>, backend: &str) -> Capture {
    let mut vx = i32::MAX;
    let mut vy = i32::MAX;
    let mut vr = i32::MIN;
    let mut vb = i32::MIN;
    for f in &frames {
        let (x, y, w, h) = f.physical.to_i32();
        vx = vx.min(x);
        vy = vy.min(y);
        vr = vr.max(x + w);
        vb = vb.max(y + h);
    }
    if vx == i32::MAX {
        vx = 0;
        vy = 0;
        vr = 1;
        vb = 1;
    }
    let mut composed = Raster::new((vr - vx).max(1) as u32, (vb - vy).max(1) as u32);
    for f in &frames {
        let (x, y, w, h) = f.physical.to_i32();
        let cw = f.image.width.min(w.max(0) as u32);
        let ch = f.image.height.min(h.max(0) as u32);
        for yy in 0..ch {
            for xx in 0..cw {
                if let Some(c) = f.image.get(xx as i32, yy as i32) {
                    composed.set(x - vx + xx as i32, y - vy + yy as i32, c);
                }
            }
        }
    }
    composed.force_opaque();
    let _ = frames;
    Capture {
        frames: Vec::new(),
        virtual_origin: (vx, vy),
        composed,
        backend: backend.into(),
    }
}

fn virtual_metrics() -> Result<(i32, i32, i32, i32), String> {
    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if vw <= 0 || vh <= 0 {
            return Err("virtual screen size is zero".into());
        }
        Ok((vx, vy, vw, vh))
    }
}

fn blit_rop() -> ROP_CODE {
    ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0)
}

/// Create a top-down 32-bit DIB and BitBlt into it; read pixels with alpha forced on.
unsafe fn dib_bitblt(src: HDC, src_x: i32, src_y: i32, w: i32, h: i32) -> Result<Raster, String> {
    if w <= 0 || h <= 0 {
        return Err("empty blit".into());
    }
    let hdc_mem = CreateCompatibleDC(src);
    if hdc_mem.is_invalid() {
        return Err("CreateCompatibleDC failed".into());
    }
    let mut bits: *mut c_void = std::ptr::null_mut();
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        bmiColors: [RGBQUAD::default()],
    };
    let hbmp = match CreateDIBSection(hdc_mem, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
        Ok(h) if !bits.is_null() => h,
        _ => {
            let _ = DeleteDC(hdc_mem);
            return Err("CreateDIBSection failed".into());
        }
    };
    let old = SelectObject(hdc_mem, HGDIOBJ(hbmp.0));
    let mut ok = BitBlt(hdc_mem, 0, 0, w, h, src, src_x, src_y, blit_rop());
    if ok.is_err() {
        ok = BitBlt(hdc_mem, 0, 0, w, h, src, src_x, src_y, SRCCOPY);
    }
    let stride = ((w * 32 + 31) / 32) * 4;
    let src_bytes = if bits.is_null() {
        &[][..]
    } else {
        std::slice::from_raw_parts(bits as *const u8, (stride * h) as usize)
    };
    let image = Raster::from_bgra_opaque(w as u32, h as u32, src_bytes, stride as usize);
    SelectObject(hdc_mem, old);
    let _ = DeleteObject(HGDIOBJ(hbmp.0));
    let _ = DeleteDC(hdc_mem);
    if ok.is_err() {
        return Err("BitBlt failed".into());
    }
    Ok(image)
}

fn grab_gdi_virtual() -> Result<ScreenFrame, String> {
    let (vx, vy, vw, vh) = virtual_metrics()?;
    unsafe {
        let hdc_screen = GetDC(HWND::default());
        if hdc_screen.is_invalid() {
            return Err("GetDC failed".into());
        }
        let image = dib_bitblt(hdc_screen, vx, vy, vw, vh);
        ReleaseDC(HWND::default(), hdc_screen);
        let image = image?;
        Ok(ScreenFrame {
            image,
            physical: Rect::from_i32(vx, vy, vw, vh),
            scale: 1.0,
            name: "virtual".into(),
        })
    }
}

struct Mon {
    hmon: HMONITOR,
    rect: RECT,
}

unsafe extern "system" fn enum_mon(hmon: HMONITOR, _hdc: HDC, lprc: *mut RECT, lp: LPARAM) -> BOOL {
    let list = &mut *(lp.0 as *mut Vec<Mon>);
    if !lprc.is_null() {
        list.push(Mon { hmon, rect: *lprc });
    }
    BOOL(1)
}

fn grab_gdi_monitors() -> Result<Vec<ScreenFrame>, String> {
    let mut mons: Vec<Mon> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(enum_mon),
            LPARAM(&mut mons as *mut _ as isize),
        );
    }
    if mons.is_empty() {
        return Err("EnumDisplayMonitors returned none".into());
    }
    let mut frames = Vec::new();
    for mon in mons {
        unsafe {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            let _ = GetMonitorInfoW(
                mon.hmon,
                &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
            );
            let hdc = CreateDCW(
                w!("DISPLAY"),
                PCWSTR(info.szDevice.as_ptr()),
                PCWSTR::null(),
                None,
            );
            let r = mon.rect;
            let w = r.right - r.left;
            let h = r.bottom - r.top;
            let image = if hdc.is_invalid() {
                let screen = GetDC(HWND::default());
                let img = dib_bitblt(screen, r.left, r.top, w, h);
                ReleaseDC(HWND::default(), screen);
                img
            } else {
                let img = dib_bitblt(hdc, 0, 0, w, h);
                let _ = DeleteDC(hdc);
                img
            }?;
            frames.push(ScreenFrame {
                image,
                physical: Rect::from_i32(r.left, r.top, w, h),
                scale: 1.0,
                name: String::from_utf16_lossy(
                    &info.szDevice[..info.szDevice.iter().position(|&c| c == 0).unwrap_or(0)],
                ),
            });
        }
    }
    Ok(frames)
}

fn grab_printwindow() -> Result<ScreenFrame, String> {
    let (vx, vy, vw, vh) = virtual_metrics()?;
    unsafe {
        let desktop = GetDesktopWindow();
        let hdc_screen = GetDC(desktop);
        if hdc_screen.is_invalid() {
            return Err("GetDC(desktop) failed".into());
        }
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let mut bits: *mut c_void = std::ptr::null_mut();
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: vw,
                biHeight: -vh,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            bmiColors: [RGBQUAD::default()],
        };
        let hbmp = CreateDIBSection(hdc_mem, &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| e.to_string())?;
        let old = SelectObject(hdc_mem, HGDIOBJ(hbmp.0));
        let ok = PrintWindow(desktop, hdc_mem, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT));
        let stride = ((vw * 32 + 31) / 32) * 4;
        let image = if bits.is_null() {
            Raster::new(vw as u32, vh as u32)
        } else {
            let src = std::slice::from_raw_parts(bits as *const u8, (stride * vh) as usize);
            Raster::from_bgra_opaque(vw as u32, vh as u32, src, stride as usize)
        };
        SelectObject(hdc_mem, old);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(desktop, hdc_screen);
        if !ok.as_bool() {
            return Err("PrintWindow failed".into());
        }
        Ok(ScreenFrame {
            image,
            physical: Rect::from_i32(vx, vy, vw, vh),
            scale: 1.0,
            name: "printwindow".into(),
        })
    }
}

// Immediate D3D context is not free-threaded — keep the session on the UI thread.
thread_local! {
    static DXGI: RefCell<Option<DxgiDup>> = const { RefCell::new(None) };
}

// Avoid retrying a lost Desktop Duplication session on the UI path. Virtual
// displays can take seconds to recreate it, which freezes mouse interaction.
fn dxgi_disabled() -> &'static AtomicBool {
    static DISABLED: OnceLock<AtomicBool> = OnceLock::new();
    DISABLED.get_or_init(|| AtomicBool::new(false))
}

struct DxgiOutput {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    dup: IDXGIOutputDuplication,
    desc: DXGI_OUTPUT_DESC,
    last: Option<ScreenFrame>,
}

struct DxgiDup {
    outputs: Vec<DxgiOutput>,
}

impl DxgiDup {
    fn open() -> Result<Self, String> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| e.to_string())?;
            let mut outputs = Vec::new();
            let mut adapter_idx = 0u32;
            while let Ok(adapter1) = factory.EnumAdapters1(adapter_idx) {
                adapter_idx += 1;
                let adapter: IDXGIAdapter = match adapter1.cast() {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                let mut device: Option<ID3D11Device> = None;
                let mut ctx: Option<ID3D11DeviceContext> = None;
                if D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut ctx),
                )
                .is_err()
                {
                    continue;
                }
                let Some(device) = device else { continue };
                let Some(ctx) = ctx else { continue };
                let mut output_idx = 0u32;
                while let Ok(output) = adapter1.EnumOutputs(output_idx) {
                    output_idx += 1;
                    let output1: IDXGIOutput1 = match output.cast() {
                        Ok(o) => o,
                        Err(_) => continue,
                    };
                    let desc = match output.GetDesc() {
                        Ok(d) => d,
                        Err(_) => continue,
                    };
                    let dup: IDXGIOutputDuplication = match output1.DuplicateOutput(&device) {
                        Ok(d) => d,
                        Err(e) => {
                            crate::logutil::log(&format!("DuplicateOutput: {e}"));
                            continue;
                        }
                    };
                    outputs.push(DxgiOutput {
                        device: device.clone(),
                        ctx: ctx.clone(),
                        dup,
                        desc,
                        last: None,
                    });
                }
            }
            let _ = D3D_FEATURE_LEVEL::default();
            if outputs.is_empty() {
                Err("DXGI produced no outputs".into())
            } else {
                Ok(Self { outputs })
            }
        }
    }

    fn grab_frames(&mut self) -> Result<Vec<ScreenFrame>, String> {
        let mut frames = Vec::new();
        let mut lost = None;
        for out in &mut self.outputs {
            match unsafe { acquire_one(&out.device, &out.ctx, &out.dup, &out.desc, &mut out.last) } {
                Ok(frame) if !frame.image.is_visually_blank() => frames.push(frame),
                Ok(_) => crate::logutil::log_at("trace", "capture: DXGI output discarded (black first frames)"),
                Err(e) if e.starts_with("lost:") => {
                    lost = Some(e);
                    break;
                }
                Err(e) => crate::logutil::error(&format!("capture: DXGI output: {e}")),
            }
        }
        if let Some(e) = lost {
            return Err(e);
        }
        if frames.is_empty() {
            Err("DXGI produced no usable frames".into())
        } else {
            Ok(frames)
        }
    }
}

fn grab_dxgi(discard_first: bool) -> Result<Vec<ScreenFrame>, String> {
    if dxgi_disabled().load(Ordering::Acquire) {
        return Err("disabled after previous loss".into());
    }
    DXGI.with(|slot| {
        let mut slot = slot.borrow_mut();
        let mut last_err = String::from("dxgi");
        for attempt in 0..2 {
            if slot.is_none() {
                let t_open = Instant::now();
                match DxgiDup::open() {
                    Ok(dup) => {
                        crate::logutil::ms("dxgi open", t_open);
                        crate::logutil::log(&format!("dxgi outputs={}", dup.outputs.len()));
                        *slot = Some(dup);
                    }
                    Err(e) => return Err(e),
                }
            }
            let t_acq = Instant::now();
            if discard_first {
                crate::logutil::log_at("trace", "dxgi discard stale resnip frame");
                let _ = slot.as_mut().unwrap().grab_frames();
            }
            match slot.as_mut().unwrap().grab_frames() {
                Ok(frames) => {
                    crate::logutil::ms("dxgi acquire", t_acq);
                    for out in &mut slot.as_mut().unwrap().outputs {
                        out.last = None;
                    }
                    return Ok(frames);
                }
                Err(e) => {
                    crate::logutil::log(&format!("dxgi disabled after {e}"));
                    last_err = e;
                    *slot = None;
                    dxgi_disabled().store(true, Ordering::Release);
                    break;
                }
            }
        }
        Err(last_err)
    })
}

fn is_bgra_family(fmt: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT) -> bool {
    fmt == DXGI_FORMAT_B8G8R8A8_UNORM
        || fmt == DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
        || fmt == DXGI_FORMAT_R8G8B8A8_UNORM
        || fmt == DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
}

fn is_rgba(fmt: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT) -> bool {
    fmt == DXGI_FORMAT_R8G8B8A8_UNORM || fmt == DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
}

fn dxgi_lost(e: &windows::core::Error) -> bool {
    let c = e.code();
    c == DXGI_ERROR_ACCESS_LOST || c == DXGI_ERROR_ACCESS_DENIED || c == DXGI_ERROR_INVALID_CALL
}

unsafe fn acquire_one(
    device: &ID3D11Device,
    ctx: &ID3D11DeviceContext,
    dup: &IDXGIOutputDuplication,
    desc: &DXGI_OUTPUT_DESC,
    last: &mut Option<ScreenFrame>,
) -> Result<ScreenFrame, String> {
    let mut last_err = String::from("no frame");
    for attempt in 0..4 {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        match dup.AcquireNextFrame(50, &mut info, &mut resource) {
            Ok(()) => {}
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {
                last_err = format!("timeout attempt {attempt}");
                crate::logutil::log(&format!("dxgi {last_err}"));
                if let Some(prev) = last.clone() {
                    crate::logutil::log("dxgi reuse last frame");
                    return Ok(prev);
                }
                continue;
            }
            Err(e) if dxgi_lost(&e) => return Err(format!("lost: {e}")),
            Err(e) => return Err(e.to_string()),
        }
        let Some(resource) = resource else {
            let _ = dup.ReleaseFrame();
            last_err = "null resource".into();
            continue;
        };
        // First acquire after DuplicateOutput is often an empty/black placeholder.
        if info.AccumulatedFrames == 0 {
            let _ = dup.ReleaseFrame();
            if let Some(prev) = last.clone() {
                crate::logutil::log("dxgi zero AccumulatedFrames reuse last");
                return Ok(prev);
            }
            last_err = "zero AccumulatedFrames".into();
            continue;
        }
        let tex: ID3D11Texture2D = resource.cast().map_err(|e| e.to_string())?;
        let mut td = D3D11_TEXTURE2D_DESC::default();
        tex.GetDesc(&mut td);
        if !is_bgra_family(td.Format) {
            let _ = dup.ReleaseFrame();
            return Err(format!("unsupported DXGI format {:?}", td.Format));
        }
        // Keep the *source* format — CopyResource requires an exact match.
        td.Usage = D3D11_USAGE_STAGING;
        td.BindFlags = 0;
        td.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        td.MiscFlags = 0;
        let mut staging: Option<ID3D11Texture2D> = None;
        if device
            .CreateTexture2D(&td, None, Some(&mut staging))
            .is_err()
        {
            let _ = dup.ReleaseFrame();
            return Err("CreateTexture2D staging failed".into());
        }
        let staging = staging.ok_or("staging")?;
        ctx.CopyResource(&staging, &tex);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|e| e.to_string())?;
        let w = td.Width;
        let h = td.Height;
        let pitch = mapped.RowPitch as usize;
        let src = std::slice::from_raw_parts(mapped.pData as *const u8, pitch * h as usize);
        let mut image = if is_rgba(td.Format) {
            // Channel swap into a scratch buffer, so the staging copy and this
            // buffer are both live at once. At 4160x1400 that is a second
            // ~22 MB on top of the mapped staging region.
            crate::logutil::mem("capture:pre-channel-swap");
            let mut bgra = vec![0u8; src.len()];
            for (d, s) in bgra.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                d[3] = s[3];
            }
            let img = Raster::from_bgra_opaque(w, h, &bgra, pitch);
            crate::logutil::mem("capture:post-channel-swap");
            img
        } else {
            Raster::from_bgra_opaque(w, h, src, pitch)
        };
        ctx.Unmap(&staging, 0);
        let _ = dup.ReleaseFrame();
        image.force_opaque();
        if image.is_visually_blank() && attempt < 3 {
            last_err = format!("black frame attempt {attempt}");
            continue;
        }
        let desk = desc.DesktopCoordinates;
        let frame = ScreenFrame {
            image,
            physical: Rect::from_i32(
                desk.left,
                desk.top,
                desk.right - desk.left,
                desk.bottom - desk.top,
            ),
            scale: 1.0,
            name: format!("dxgi-{}", attempt),
        };
        *last = Some(frame.clone());
        return Ok(frame);
    }
    if let Some(prev) = last.clone() {
        crate::logutil::log("dxgi reuse last after retries");
        return Ok(prev);
    }
    Err(last_err)
}

fn dump_path() -> PathBuf {
    std::env::temp_dir().join("pinshot-capture.png")
}

fn dump_capture(cap: &Capture) {
    let path = dump_path();
    match crate::dialog::save_raster(&cap.composed, &path.to_string_lossy(), 92) {
        Ok(()) => crate::logutil::log("capture: diagnostic image saved to temporary directory"),
        Err(e) => crate::logutil::error(&format!("capture: diagnostic image save failed: {e}")),
    }
}

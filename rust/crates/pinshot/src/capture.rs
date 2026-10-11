//! DXGI Desktop Duplication capture with GDI BitBlt fallback.

use pinshot_core::geom::Rect;
use pinshot_core::raster::Raster;
use windows::core::Interface;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION_IDENTITY, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
    DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTPUT_DESC,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
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
}

impl Capture {
    pub fn grab() -> Result<Self, String> {
        match grab_dxgi() {
            Ok(frames) if !frames.is_empty() => Ok(compose(frames)),
            Ok(_) | Err(_) => {
                let frame = grab_gdi().map_err(|e| format!("capture failed: {e}"))?;
                Ok(compose(vec![frame]))
            }
        }
    }
}

fn compose(frames: Vec<ScreenFrame>) -> Capture {
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
        for yy in 0..h {
            for xx in 0..w {
                if let Some(c) = f.image.get(xx, yy) {
                    composed.set(x - vx + xx, y - vy + yy, c);
                }
            }
        }
    }
    Capture {
        frames,
        virtual_origin: (vx, vy),
        composed,
    }
}

fn grab_gdi() -> Result<ScreenFrame, String> {
    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if vw <= 0 || vh <= 0 {
            return Err("virtual screen size is zero".into());
        }
        let hdc_screen = GetDC(HWND::default());
        if hdc_screen.is_invalid() {
            return Err("GetDC failed".into());
        }
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let hbmp = CreateCompatibleBitmap(hdc_screen, vw, vh);
        let old = SelectObject(hdc_mem, HGDIOBJ(hbmp.0));
        let ok = BitBlt(hdc_mem, 0, 0, vw, vh, hdc_screen, vx, vy, SRCCOPY);
        if ok.is_err() {
            SelectObject(hdc_mem, old);
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(HWND::default(), hdc_screen);
            return Err("BitBlt failed".into());
        }
        let stride = ((vw * 32 + 31) / 32) * 4;
        let mut bgra = vec![0u8; (stride * vh) as usize];
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: vw,
                biHeight: -vh,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let got = GetDIBits(
            hdc_mem,
            hbmp,
            0,
            vh as u32,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(hdc_mem, old);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(HWND::default(), hdc_screen);
        if got == 0 {
            return Err("GetDIBits failed".into());
        }
        let image = Raster::from_bgra(vw as u32, vh as u32, &bgra, stride as usize);
        Ok(ScreenFrame {
            image,
            physical: Rect::from_i32(vx, vy, vw, vh),
            scale: 1.0,
            name: "virtual".into(),
        })
    }
}

fn grab_dxgi() -> Result<Vec<ScreenFrame>, String> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| e.to_string())?;
        let mut device: Option<ID3D11Device> = None;
        let mut ctx: Option<ID3D11DeviceContext> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut ctx),
        )
        .map_err(|e| e.to_string())?;
        let device = device.ok_or("D3D11 device")?;
        let ctx = ctx.ok_or("D3D11 context")?;
        let mut frames = Vec::new();
        let mut adapter_idx = 0u32;
        while let Ok(adapter) = factory.EnumAdapters1(adapter_idx) {
            adapter_idx += 1;
            let mut output_idx = 0u32;
            while let Ok(output) = adapter.EnumOutputs(output_idx) {
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
                    Err(_) => continue,
                };
                if let Ok(frame) = acquire_one(&device, &ctx, &dup, &desc) {
                    frames.push(frame);
                }
                let _ = dup.ReleaseFrame();
            }
        }
        let _ = D3D_FEATURE_LEVEL::default();
        let _ = DXGI_FORMAT::default();
        let _ = DXGI_MODE_ROTATION_IDENTITY;
        let _ = DXGI_SAMPLE_DESC::default();
        if frames.is_empty() {
            Err("DXGI produced no frames".into())
        } else {
            Ok(frames)
        }
    }
}

unsafe fn acquire_one(
    device: &ID3D11Device,
    ctx: &ID3D11DeviceContext,
    dup: &IDXGIOutputDuplication,
    desc: &DXGI_OUTPUT_DESC,
) -> Result<ScreenFrame, String> {
    let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
    let mut resource: Option<IDXGIResource> = None;
    match dup.AcquireNextFrame(400, &mut info, &mut resource) {
        Ok(()) => {}
        Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {
            return Err("DXGI timeout".into());
        }
        Err(e) => return Err(e.to_string()),
    }
    let resource = resource.ok_or("no desktop resource")?;
    let tex: ID3D11Texture2D = resource.cast().map_err(|e| e.to_string())?;
    let mut td = D3D11_TEXTURE2D_DESC::default();
    tex.GetDesc(&mut td);
    td.Usage = D3D11_USAGE_STAGING;
    td.BindFlags = 0;
    td.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    td.MiscFlags = 0;
    td.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
    let mut staging: Option<ID3D11Texture2D> = None;
    device
        .CreateTexture2D(&td, None, Some(&mut staging))
        .map_err(|e| e.to_string())?;
    let staging = staging.ok_or("staging")?;
    ctx.CopyResource(&staging, &tex);
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        .map_err(|e| e.to_string())?;
    let w = td.Width;
    let h = td.Height;
    let pitch = mapped.RowPitch as usize;
    let src = std::slice::from_raw_parts(mapped.pData as *const u8, pitch * h as usize);
    let image = Raster::from_bgra(w, h, src, pitch);
    ctx.Unmap(&staging, 0);
    let _ = dup.ReleaseFrame();
    let desk = desc.DesktopCoordinates;
    Ok(ScreenFrame {
        image,
        physical: Rect::from_i32(
            desk.left,
            desk.top,
            desk.right - desk.left,
            desk.bottom - desk.top,
        ),
        scale: 1.0,
        name: String::new(),
    })
}

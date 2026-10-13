use arboard::{Clipboard, ImageData};
use pinshot_core::color::{parse_color_text, Rgba};
use pinshot_core::raster::Raster;
#[cfg(unix)]
use std::io::Write;
use std::sync::{Mutex, OnceLock};
#[cfg(unix)]
use std::time::{Duration, Instant};

pub enum ClipContent {
    Image(Raster),
    Color(Rgba),
    Text(String),
    Empty,
}

struct Owner {
    cb: Option<Clipboard>,
}

fn owner() -> &'static Mutex<Owner> {
    static OWNER: OnceLock<Mutex<Owner>> = OnceLock::new();
    OWNER.get_or_init(|| Mutex::new(Owner { cb: Clipboard::new().ok() }))
}

fn with_owner<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut Clipboard) -> Result<T, String>,
{
    let mut g = owner().lock().map_err(|e| e.to_string())?;
    if g.cb.is_none() {
        g.cb = Clipboard::new().ok();
    }
    let cb = g.cb.as_mut().ok_or_else(|| "clipboard unavailable".to_string())?;
    f(cb)
}

pub fn write_image(img: &Raster) -> Result<(), String> {
    let data = ImageData {
        width: img.width as usize,
        height: img.height as usize,
        bytes: img.pixels.clone().into(),
    };
    let r = with_owner(|cb| cb.set_image(data.clone()).map_err(|e| e.to_string()));
    #[cfg(unix)]
    handoff_x11_image(img);
    if r.is_ok() {
        crate::logutil::log(&format!("clipboard image {}x{}", img.width, img.height));
    } else if let Err(e) = &r {
        crate::logutil::log(&format!("clipboard set_image: {e}"));
    }
    r
}

pub fn write_text(s: &str) -> Result<(), String> {
    let owned = s.to_string();
    let r = with_owner(|cb| cb.set_text(owned.clone()).map_err(|e| e.to_string()));
    #[cfg(unix)]
    handoff_x11_text(&owned);
    r
}

pub fn read_text() -> Result<String, String> {
    with_owner(|cb| cb.get_text().map_err(|e| e.to_string()))
}

pub fn read_best() -> ClipContent {
    if let Ok(img) = with_owner(|cb| cb.get_image().map_err(|e| e.to_string())) {
        if let Some(r) = Raster::from_rgba(
            img.width as u32,
            img.height as u32,
            img.bytes.into_owned(),
        ) {
            return ClipContent::Image(r);
        }
    }
    if let Ok(text) = with_owner(|cb| cb.get_text().map_err(|e| e.to_string())) {
        if let Some(c) = parse_color_text(&text) {
            return ClipContent::Color(c);
        }
        if !text.trim().is_empty() {
            return ClipContent::Text(text);
        }
    }
    ClipContent::Empty
}

pub fn color_card(c: Rgba, _w: u32, _h: u32) -> Raster {
    pinshot_core::font::color_card(c)
}

pub fn text_card(text: &str, _w: u32, _h: u32) -> Raster {
    pinshot_core::font::text_card(text, 640)
}

/// Keep a process-independent CLIPBOARD owner after the overlay window dies
/// (`xclip`/`xsel` fork and stay; Python's QClipboard does the same).
#[cfg(unix)]
fn handoff_x11_image(img: &Raster) {
    let Some(png) = encode_png(img) else {
        crate::logutil::log("clipboard: png encode failed");
        return;
    };
    if spawn_stdin(
        "xclip",
        &["-selection", "clipboard", "-t", "image/png", "-i"],
        &png,
    ) {
        crate::logutil::log(&format!("clipboard xclip image/png {} bytes", png.len()));
    } else {
        crate::logutil::log("clipboard xclip handoff failed; arboard owner kept");
    }
}

#[cfg(unix)]
fn handoff_x11_text(s: &str) {
    let bytes = s.as_bytes();
    for cmd in [
        ("xclip", &["-selection", "clipboard", "-i"][..]),
        ("xsel", &["--clipboard", "--input"][..]),
    ] {
        if spawn_stdin(cmd.0, cmd.1, bytes) {
            return;
        }
    }
}

#[cfg(unix)]
fn spawn_stdin(bin: &str, args: &[&str], bytes: &[u8]) -> bool {
    let Ok(mut child) = std::process::Command::new(bin)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    let wrote = if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(bytes).is_ok()
    } else {
        false
    };
    if !wrote {
        let _ = child.kill();
        return false;
    }
    // xclip -silent forks after stdin EOF; the parent exits. Do not block
    // forever if a helper stays in the foreground serving CLIPBOARD.
    let deadline = Instant::now() + Duration::from_millis(800);
    loop {
        match child.try_wait() {
            Ok(Some(st)) => return st.success(),
            Ok(None) if Instant::now() >= deadline => return true,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return false,
        }
    }
}

#[cfg(unix)]
fn encode_png(img: &Raster) -> Option<Vec<u8>> {
    let im = image::RgbaImage::from_raw(img.width, img.height, img.pixels.clone())?;
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    im.write_to(&mut cursor, image::ImageFormat::Png).ok()?;
    Some(buf)
}

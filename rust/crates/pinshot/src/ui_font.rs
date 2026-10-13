//! Load a system CJK face into egui (Noto Sans CJK / Source Han / YaHei).

use eframe::egui;

struct CjkFace {
    bytes: Vec<u8>,
    index: u32,
    path: std::path::PathBuf,
}

fn candidates() -> Vec<(std::path::PathBuf, u32)> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("PINSHOT_UI_FONT") {
        out.push((std::path::PathBuf::from(p), 0));
    }
    out.extend(
        [
            ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 2),
            ("/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf", 0),
            ("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc", 2),
            ("/usr/share/fonts/truetype/noto/NotoSansSC-Regular.otf", 0),
            (
                "/usr/share/fonts/opentype/source-han-sans-sc/SourceHanSansSC-Regular.otf",
                0,
            ),
            ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
            ("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf", 0),
            ("/usr/share/fonts/truetype/droid/DroidSansFallback.ttf", 0),
        ]
        .into_iter()
        .map(|(p, i)| (std::path::PathBuf::from(p), i)),
    );
    if let Some(windir) = std::env::var_os("WINDIR") {
        let fonts = std::path::PathBuf::from(windir).join("Fonts");
        out.push((fonts.join("msyh.ttc"), 0));
        out.push((fonts.join("msyh.ttf"), 0));
        out.push((fonts.join("msyhbd.ttc"), 0));
    } else {
        out.push((std::path::PathBuf::from("C:\\Windows\\Fonts\\msyh.ttc"), 0));
        out.push((std::path::PathBuf::from("C:\\Windows\\Fonts\\msyh.ttf"), 0));
    }
    out
}

fn load_face() -> Option<CjkFace> {
    for (path, index) in candidates() {
        if !path.is_file() {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            if bytes.len() > 64 {
                crate::logutil::log(&format!(
                    "ui font {} index={index}",
                    path.display()
                ));
                return Some(CjkFace { bytes, index, path });
            }
        }
        let _ = index;
        let _ = path;
    }
    None
}

/// Install a proportional CJK face at the front of egui's font families.
pub fn install_cjk(ctx: &egui::Context) -> Option<std::path::PathBuf> {
    let face = load_face()?;
    let path = face.path.clone();
    let mut fonts = egui::FontDefinitions::default();
    let mut data = egui::FontData::from_owned(face.bytes);
    data.index = face.index;
    fonts
        .font_data
        .insert("pinshot_cjk".into(), data.into());
    if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        fam.insert(0, "pinshot_cjk".into());
    }
    if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        fam.push("pinshot_cjk".into());
    }
    ctx.set_fonts(fonts);
    Some(path)
}

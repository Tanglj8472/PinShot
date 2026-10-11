//! Tiny live i18n: English source strings → Simplified Chinese.
//! Default language is `zh_CN` (Phase 1).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    ZhCn,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::ZhCn => "zh_CN",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "en" | "en_US" | "en-US" => Lang::En,
            "zh" | "zh_CN" | "zh-CN" | "zh_Hans" | "zh-Hans" => Lang::ZhCn,
            _ => Lang::ZhCn,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::ZhCn => "简体中文",
        }
    }
}

const PAIRS: &[(&str, &str)] = &[
    ("PinShot", "PinShot"),
    ("Snip (F1)", "截图 (F1)"),
    ("Pin clipboard (F3)", "贴剪贴板 (F3)"),
    ("Hide/Show pins (Shift+F3)", "隐藏/显示贴图 (Shift+F3)"),
    ("Restore last closed pin", "恢复最近关闭的贴图"),
    ("Preferences…", "首选项…"),
    ("Quit", "退出"),
    ("Snip", "截图"),
    ("Pin clipboard", "贴剪贴板"),
    ("Hide/Show all pins", "隐藏/显示全部贴图"),
    ("Copied to clipboard", "已复制到剪贴板"),
    ("Clipboard has no image or text", "剪贴板中没有图片或文字"),
    ("Rectangle", "矩形"),
    ("Ellipse", "椭圆"),
    ("Line / arrow (Shift+drag continues polyline)", "直线 / 箭头（Shift+拖动继续折线）"),
    ("Pencil", "铅笔"),
    ("Marker", "记号笔"),
    ("Mosaic / blur", "马赛克 / 模糊"),
    ("Text", "文字"),
    ("Numbered marker (N)", "序号标记 (N)"),
    ("Undo (Ctrl+Z)", "撤销 (Ctrl+Z)"),
    ("Redo (Ctrl+Y)", "重做 (Ctrl+Y)"),
    ("Cancel (Esc)", "取消 (Esc)"),
    ("Pin to screen (Ctrl+T)", "贴到屏幕 (Ctrl+T)"),
    ("Save as… (Ctrl+S)", "另存为… (Ctrl+S)"),
    ("Copy (Ctrl+C / Enter)", "复制 (Ctrl+C / Enter)"),
    ("Outline", "描边"),
    ("Filled", "填充"),
    ("Width", "粗细"),
    ("Size", "大小"),
    ("Arrow", "箭头"),
    ("Arrow at end", "末端箭头"),
    ("Brush", "画笔"),
    ("Rect", "矩形"),
    ("Mosaic", "马赛克"),
    ("Blur", "模糊"),
    ("Abort snipping?", "放弃截图？"),
    ("Do not show again", "不再提示"),
    ("Yes", "是"),
    ("No", "否"),
    ("Press C to copy color", "按 C 复制颜色"),
    ("Shift: switch RGB/HEX", "Shift：切换 RGB/HEX"),
    ("Move the cursor by 1 pixel", "光标移动 1 像素"),
    ("Move the selection by 1 pixel", "选区移动 1 像素"),
    ("Enlarge selection / Shift: shrink", "扩大选区 / Shift：缩小"),
    ("Toggle window / element detection", "切换窗口 / 元素检测"),
    ("Select full screen", "全屏选择"),
    ("Copy color / RGB↔HEX", "复制颜色 / RGB↔HEX"),
    ("Numbered marker", "序号标记"),
    ("Decrease / increase width", "减小 / 增大粗细"),
    ("Finish current shape", "完成当前图形"),
    ("Square shape / continue polyline", "正方形 / 继续折线"),
    ("Undo / redo", "撤销 / 重做"),
    ("Copy", "复制"),
    ("Pin", "贴图"),
    ("Save", "保存"),
    ("Cancel", "取消"),
    ("Delete selected", "删除所选"),
    ("Place a text box", "放置文本框"),
    ("Commit text", "确认文字"),
    ("Discard text", "放弃文字"),
    ("Font size", "字号"),
    ("Finish", "完成"),
    ("Place next number", "放置下一个序号"),
    ("Click", "单击"),
    ("Drag", "拖动"),
    ("Handles", "手柄"),
    ("Right-click", "右键"),
    ("Size label", "尺寸"),
    ("Save snip", "保存截图"),
    ("window", "窗口"),
    ("element", "元素"),
    ("CLICK-THRU", "鼠标穿透"),
    ("Copy image", "复制图像"),
    ("Save image as…", "图像另存为…"),
    ("Click-through", "鼠标穿透"),
    ("Zoom", "缩放"),
    ("Close", "关闭"),
    ("Reset zoom to 100%", "重置缩放为 100%"),
    ("Save image as", "图像另存为"),
    ("Line", "直线"),
    ("Arrow tool", "箭头"),
    ("Settings (later)", "设置（稍后）"),
    ("OCR (later)", "OCR（稍后）"),
    ("Translate (later)", "翻译（稍后）"),
];

const PATTERNS: &[(&str, &str)] = &[
    ("Opacity: {0}%", "不透明度：{0}%"),
    ("Zoom: {0}%", "缩放：{0}%"),
    ("Zoom {0}%  ·  Opacity {1}%", "缩放 {0}%  ·  不透明度 {1}%"),
    ("Color copied: {0}", "已复制颜色：{0}"),
    ("Copied {0}", "已复制 {0}"),
    ("Saved {0}", "已保存 {0}"),
    ("Detection: {0}", "检测：{0}"),
    ("Detection: {0} ({1})", "检测：{0}（{1}）"),
];

fn zh_map() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| PAIRS.iter().copied().collect())
}

fn lang_cell() -> &'static Mutex<Lang> {
    static LANG: OnceLock<Mutex<Lang>> = OnceLock::new();
    LANG.get_or_init(|| Mutex::new(Lang::ZhCn))
}

pub fn lang() -> Lang {
    *lang_cell().lock().expect("i18n lock")
}

pub fn set_lang(code: Lang) {
    *lang_cell().lock().expect("i18n lock") = code;
}

pub fn set_lang_code(code: &str) {
    set_lang(Lang::from_code(code));
}

fn tr_line(s: &str) -> String {
    if let Some(z) = zh_map().get(s) {
        return (*z).to_string();
    }
    let st = s.trim();
    if !st.is_empty() {
        if let Some(z) = zh_map().get(st) {
            return s.replacen(st, z, 1);
        }
    }
    for (en, zh) in PATTERNS {
        if let Some(out) = match_pattern(s, en, zh) {
            return out;
        }
    }
    s.to_string()
}

/// Very small `{0}` / `{1}` template matcher (enough for Phase 1 dynamic strings).
fn match_pattern(s: &str, pat: &str, tpl: &str) -> Option<String> {
    let mut rest = s;
    let mut values = Vec::new();
    let mut leftover = pat;
    while let Some(idx) = leftover.find('{') {
        let (prefix, after) = leftover.split_at(idx);
        if !rest.starts_with(prefix) {
            return None;
        }
        rest = &rest[prefix.len()..];
        let end = after.find('}')?;
        let after_ph = &after[end + 1..];
        leftover = after_ph;
        if after_ph.is_empty() {
            values.push(rest.to_string());
            rest = "";
        } else {
            let stop = rest.find(after_ph).unwrap_or(rest.len());
            values.push(rest[..stop].to_string());
            rest = &rest[stop..];
        }
    }
    if !rest.starts_with(leftover) {
        return None;
    }
    rest = &rest[leftover.len()..];
    if !rest.is_empty() && !leftover.is_empty() {
        return None;
    }
    let mut out = tpl.to_string();
    for (i, v) in values.iter().enumerate() {
        let key = format!("{{{i}}}");
        let translated = if *v == "ON" {
            "开".into()
        } else if *v == "OFF" {
            "关".into()
        } else {
            tr_line(v)
        };
        out = out.replace(&key, &translated);
    }
    Some(out.replace("（）", ""))
}

pub fn tr(s: &str) -> String {
    if lang() == Lang::En || s.is_empty() {
        return s.to_string();
    }
    s.split('\n').map(tr_line).collect::<Vec<_>>().join("\n")
}

/// Look up an English source string and translate it (identity when `en`).
pub fn t(en: &str) -> String {
    tr(en)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_lang<R>(l: Lang, f: impl FnOnce() -> R) -> R {
        let prev = lang();
        set_lang(l);
        let r = f();
        set_lang(prev);
        r
    }

    #[test]
    fn default_is_zh() {
        // Fresh process default is zh; this test just checks from_code + set.
        assert_eq!(Lang::from_code("zh"), Lang::ZhCn);
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("de"), Lang::ZhCn);
    }

    #[test]
    fn zh_static_strings() {
        with_lang(Lang::ZhCn, || {
            assert_eq!(tr("Snip (F1)"), "截图 (F1)");
            assert_eq!(tr("Quit"), "退出");
            assert_eq!(tr("Abort snipping?"), "放弃截图？");
            assert_eq!(tr("CLICK-THRU"), "鼠标穿透");
        });
    }

    #[test]
    fn en_is_identity() {
        with_lang(Lang::En, || {
            assert_eq!(tr("Snip (F1)"), "Snip (F1)");
            assert_eq!(tr("Opacity: 80%"), "Opacity: 80%");
        });
    }

    #[test]
    fn patterns() {
        with_lang(Lang::ZhCn, || {
            assert_eq!(tr("Opacity: 80%"), "不透明度：80%");
            assert_eq!(tr("Zoom: 150%"), "缩放：150%");
            assert_eq!(tr("Copied #FF8800"), "已复制 #FF8800");
            assert_eq!(tr("Detection: window"), "检测：窗口");
        });
    }

    #[test]
    fn unknown_stays() {
        with_lang(Lang::ZhCn, || {
            assert_eq!(tr("Completely unknown"), "Completely unknown");
        });
    }
}

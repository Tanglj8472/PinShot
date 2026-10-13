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
        match (lang(), self) {
            (Lang::En, Lang::En) => "English",
            (Lang::En, Lang::ZhCn) => "Simplified Chinese",
            (Lang::ZhCn, Lang::En) => "英语",
            (Lang::ZhCn, Lang::ZhCn) => "简体中文",
        }
    }
}

const PAIRS: &[(&str, &str)] = &[
    ("PinShot Settings", "PinShot 设置"),
    ("Could not save settings", "设置保存失败"),
    ("Saved. Restart PinShot to apply.", "已保存，重启后生效。"),
    ("English", "英语"),
    ("Simplified Chinese", "简体中文"),
    ("Traditional Chinese", "繁体中文"),
    ("Japanese", "日语"),
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
    ("Toggle window / UI element detection", "切换窗口 / 界面元素检测"),
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
    ("Preferences…", "首选项…"),
    ("PinShot Preferences", "PinShot 首选项"),
    ("General", "常规"),
    ("Interface", "界面"),
    ("Control", "控制"),
    ("About", "关于"),
    ("Hotkeys", "热键"),
    ("Output", "输出"),
    ("Advanced", "高级"),
    ("Restore Defaults", "恢复默认"),
    ("Open containing folder", "打开所在文件夹"),
    ("Open log folder", "打开日志文件夹"),
    ("Configuration", "配置"),
    ("Start minimized", "启动后最小化"),
    ("Mask opacity:", "遮罩不透明度："),
    ("Global hotkeys", "全局热键"),
    ("File", "文件"),
    ("Open", "打开"),
    ("Restored this page to defaults.", "已恢复本页为默认值。"),
    ("Settings layout follows Snipaste Preferences. Features stay PinShot's.", "设置页版式对照 Snipaste 首选项。功能仍是 PinShot 的。"),
    ("Apply", "应用"),
    ("Settings applied.", "已应用。"),
    ("No snip history", "没有截图历史"),
    ("OK", "确定"),
    ("Run at login", "登录时启动"),
    ("Start PinShot when you log in", "登录时启动 PinShot"),
    ("Snip delay:", "截图延时："),
    ("None (0 s)", "无（0 秒）"),
    ("Language:", "语言："),
    ("Theme:", "主题："),
    ("Confirm before abort (Esc)", "放弃截图前确认 (Esc)"),
    ("Double-click pin:", "双击贴图："),
    ("Close pin", "关闭贴图"),
    ("Hide pin", "隐藏贴图"),
    ("Do nothing", "无操作"),
    ("Snip history:", "截图历史："),
    ("Closed pins:", "已关闭贴图："),
    ("Pin groups:", "贴图分组："),
    ("Add", "添加"),
    ("Rename", "重命名"),
    ("Remove", "删除"),
    ("Snip:", "截图："),
    ("Pin clipboard:", "贴剪贴板："),
    ("Hide/show pins:", "隐藏/显示贴图："),
    ("Restore last pin:", "恢复最近贴图："),
    ("Select-text translate:", "划词翻译："),
    ("Snip and translate:", "截图翻译："),
    ("Click-through:", "鼠标穿透："),
    ("Save folder:", "保存文件夹："),
    ("Quality:", "质量："),
    ("Browse…", "浏览…"),
    ("Default", "默认"),
    ("Groups", "分组"),
    ("Snip and translate", "截图翻译"),
    ("Show toolbar", "显示工具栏"),
    ("Recognize text", "识别文字"),
    ("Recognize and translate", "识别并翻译"),
    ("UI element detection", "界面元素检测"),
    ("Window detection", "窗口检测"),
    ("Last snip area", "上次选区"),
    ("Snip history", "截图历史"),
    ("Original (editable)", "原文（可编辑）"),
    ("Translation", "译文"),
    ("Translate", "翻译"),
    ("Copy original", "复制原文"),
    ("Copy translation", "复制译文"),
    ("PinShot OCR", "PinShot OCR"),
    ("OCR runs on this device. Translation sends text online.", "OCR 在本机运行。翻译会把文字发到网上。"),
    ("OCR done. Edit, copy, or translate.", "识别完成。可编辑、复制或翻译。"),
    ("No text to translate", "没有可翻译的文字"),
    ("OCR not available in this build. Use pinshot-ocr.exe.", "此构建未包含 OCR，请使用 pinshot-ocr.exe。"),
    ("Downloading OCR models…", "正在下载 OCR 模型…"),
    ("OCR ready — drag to select text", "识别完成 — 拖选文字即可"),
    ("OCR model:", "OCR 模型："),
    ("Mobile (faster)", "轻量（更快）"),
    ("Server (more accurate)", "服务器（更准）"),
    ("Mobile is the default (fast). Server is more accurate but much slower on CPU.", "默认用轻量模型（快）。服务器模型更准，但 CPU 上会慢很多。"),
    ("The Default group cannot be removed.", "不能删除默认分组。"),
    ("The Default group cannot be renamed.", "不能重命名默认分组。"),
    ("Hotkeys already used by another app cannot be bound. Click Apply to rebind.", "已被其他程序占用的热键无法绑定。点「应用」重新注册。"),
    ("Conflicting hotkeys", "热键冲突"),
    ("System", "跟随系统"),
    ("Light", "浅色"),
    ("Dark", "深色"),
    ("Add group", "添加分组"),
    ("Name:", "名称："),
    ("Hide ", "隐藏 "),
    ("Show ", "显示 "),
    ("Hide/Show pins", "隐藏/显示贴图"),
    ("PinShot is running", "PinShot 正在运行"),
    ("Follow system", "跟随系统"),
    ("Provider:", "服务商："),
    ("API URL:", "API 地址："),
    ("API key:", "API 密钥："),
    ("Model:", "模型："),
    ("Target language:", "目标语言："),
    ("Follow UI language", "跟随界面语言"),
    ("Microsoft Edge (free)", "微软 Edge（免费）"),
    ("OpenAI-compatible", "OpenAI 兼容"),
    ("DeepL", "DeepL"),
    ("Test", "测试"),
    ("Testing…", "测试中…"),
    ("Translation OK:", "翻译成功："),
    ("Translation failed:", "翻译失败："),
    (
        "OpenAI-compatible covers OpenAI, DeepSeek, Qwen, Ollama, OpenRouter. Edge stays the free fallback.",
        "OpenAI 兼容接口可用于 OpenAI、DeepSeek、通义千问、Ollama、OpenRouter。Edge 仍是免费备用。",
    ),
    ("Select (V)", "选择 (V)"),
    ("Line / arrow (Shift)", "直线 / 箭头 (Shift)"),
    ("Number (N)", "序号 (N)"),
    ("Eraser", "橡皮擦"),
    ("Pin (Ctrl+T)", "贴到屏幕 (Ctrl+T)"),
    ("Save (Ctrl+S)", "另存为 (Ctrl+S)"),
    ("Copy (Ctrl+C)", "复制 (Ctrl+C)"),
    ("Done (Space)", "完成 (Space)"),
    ("Custom color", "自定义颜色"),
    ("Annotation color", "标注颜色"),
    ("OCR", "OCR"),
    ("Bold", "粗体"),
    ("Italic", "斜体"),
    ("Outline / glow", "描边 / 光晕"),
    ("Filled circle", "实心圆"),
    ("Click, drag, Del, double-click text", "单击、拖动、Del、双击文字"),
    ("Solid", "实线"),
    ("Dash", "虚线"),
    ("Dot", "点线"),
    ("Shift: RGB / HEX", "Shift：切换 RGB / HEX"),
    ("Move cursor 1 px", "光标移动 1 px"),
    ("Move selection 1 px", "选区移动 1 px"),
    ("Grow / Shift: shrink", "扩大选区 / Shift：缩小"),
    ("Window / UI element", "窗口 / 界面元素"),
    ("Full screen", "全屏"),
    ("Show cursor", "显示指针"),
    ("Select / edit", "选择 / 编辑"),
    ("Finish shape", "完成图形"),
    ("Square / polyline", "正方形 / 折线"),
    ("Place text", "放置文本"),
    ("Select shape", "选择图形"),
    ("Move", "移动"),
    ("Resize", "缩放"),
    ("Delete", "删除"),
    ("Re-edit", "重新编辑"),
    ("Undo edit", "撤销编辑"),
    ("1 2 / wheel", "1 2 / 滚轮"),
    ("Cursor shown", "指针：显示"),
    ("Cursor hidden", "指针：隐藏"),
    ("Select text, then release", "框选文字后松开"),
    ("Shadow", "阴影"),
    ("Thumbnail", "缩略图"),
    ("Image", "图像"),
    ("Rotate left", "向左旋转"),
    ("Rotate right", "向右旋转"),
    ("Flip horizontal", "水平翻转"),
    ("Flip vertical", "垂直翻转"),
    ("Paste", "粘贴"),
    ("Replace from file…", "用文件替换…"),
    ("Replace from file", "用文件替换"),
    ("Move to group", "移动到分组"),
    ("New group…", "新建分组…"),
    ("New group", "新建分组"),
    ("Rename group…", "重命名此分组…"),
    ("Rename group", "重命名分组"),
    ("Sans", "Sans"),
    ("Serif", "Serif"),
    ("Mono", "等宽"),
    ("View in folder", "在文件夹中查看"),
    ("Destroy", "销毁"),
    ("Copy size", "复制尺寸"),
    ("Reset zoom", "重置缩放"),
    ("Tip: double-click closes a pin.", "提示：双击关闭贴图。"),
    ("Tip: double-click hides a pin.", "提示：双击隐藏贴图。"),
    ("Double-click again to confirm.", "再双击一次确认。"),
    ("No closed pins to restore", "没有可恢复的已关闭贴图"),
    ("Restored closed pin", "已恢复关闭的贴图"),
    ("No pin under cursor", "光标下没有贴图"),
    ("Rename rejected", "重命名被拒绝"),
    ("Click-through on", "鼠标穿透 开"),
    ("Click-through off", "鼠标穿透 关"),
    ("unbound", "未绑定"),
    ("unavailable", "不可用"),
    ("Could not copy selected text.", "无法复制所选文字。"),
    ("Select-text translate is only available here.", "划词翻译仅在 Windows 上可用。"),
    ("No translation returned", "没有返回译文"),
    ("Recognizing…", "正在识别文字…"),
    ("Translating…", "正在翻译…"),
    ("No text found. Select a clearer area.", "没有识别到文字。请选更清晰的区域。"),
    ("Translation done", "翻译完成"),
    ("RGB", "RGB"),
    ("RGB float", "RGB 浮点"),
    ("HSV", "HSV"),
    ("HSL", "HSL"),
    ("HSI", "HSI"),
    ("HEX", "HEX"),
    ("Could not update autostart:", "无法更新自启动："),
    ("Quick-save folder", "快速保存文件夹"),
    ("Numbered marker (N)", "序号标记 (N)"),
    ("Save image as", "图像另存为"),
    ("Smoothing", "平滑"),
    ("Current", "当前"),
    ("Image processing", "图像处理"),
    ("Horizontal flip", "水平翻转"),
    ("Vertical flip", "垂直翻转"),
    ("Replace by file...", "用文件替换..."),
    ("Group", "分组"),
    ("Rotation", "旋转"),
    ("Opacity", "不透明度"),
    ("X-mirror", "水平镜像"),
    ("Y-mirror", "垂直镜像"),
    ("Grayscale", "灰度"),
    ("Color inverted", "反色"),
    ("OCR failed", "识别失败"),
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
    ("Group {0}", "分组 {0}"),
    ("Unbound hotkeys: {0}", "未绑定热键：{0}"),
    ("{0} s", "{0} 秒"),
    ("Downloading {0}%", "正在下载 {0}%"),
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
            // Only the literal text until the next `{n}` is the separator.
            // Searching for `after_ph` itself fails when it still contains `{1}`.
            let sep_end = after_ph.find('{').unwrap_or(after_ph.len());
            let sep = &after_ph[..sep_end];
            let stop = if sep.is_empty() {
                rest.len()
            } else {
                rest.find(sep).unwrap_or(rest.len())
            };
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
        static TEST: Mutex<()> = Mutex::new(());
        let _g = TEST.lock().expect("i18n test");
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
            assert_eq!(Lang::ZhCn.display_name(), "简体中文");
            assert_eq!(Lang::En.display_name(), "英语");
            assert_eq!(tr("Snip (F1)"), "截图 (F1)");
            assert_eq!(tr("Quit"), "退出");
            assert_eq!(tr("Abort snipping?"), "放弃截图？");
            assert_eq!(tr("CLICK-THRU"), "鼠标穿透");
            assert_eq!(tr("Show toolbar"), "显示工具栏");
            assert_eq!(tr("Rotate left"), "向左旋转");
            assert_eq!(tr("PinShot is running"), "PinShot 正在运行");
            assert_eq!(tr("Save"), "保存");
            assert_eq!(tr("Copy"), "复制");
            assert_eq!(tr("Close"), "关闭");
            assert_eq!(tr("Window detection"), "窗口检测");
            assert_eq!(tr("UI element detection"), "界面元素检测");
        });
    }

    #[test]
    fn en_is_identity() {
        with_lang(Lang::En, || {
            assert_eq!(Lang::ZhCn.display_name(), "Simplified Chinese");
            assert_eq!(Lang::En.display_name(), "English");
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
            assert_eq!(
                tr("Zoom 68%  ·  Opacity 100%"),
                "缩放 68%  ·  不透明度 100%"
            );
        });
    }

    #[test]
    fn unknown_stays() {
        with_lang(Lang::ZhCn, || {
            assert_eq!(tr("Completely unknown"), "Completely unknown");
        });
    }
}

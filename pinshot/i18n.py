"""Live i18n: every user-visible string has English and Simplified Chinese.

Source widgets always store the English table key. tr() and Translator rewrite
to the active language. Painted strings call tr() at paint time.
"""
import re
from PySide6.QtCore import QObject, QEvent, QTimer, Signal
from PySide6.QtWidgets import (QApplication, QWidget, QLabel, QAbstractButton, QGroupBox, QTabWidget,
                               QComboBox, QFontComboBox, QLineEdit, QMenu, QListWidget,
                               QTextEdit, QSpinBox, QAbstractSlider)

LANGS = [("en", "English"), ("zh_CN", "简体中文")]

# Latin tokens allowed inside Chinese strings (keys, units, product, OCR, colour spaces).
ALLOWED_ZH_LATIN = frozenset({
    "Ctrl", "Shift", "Alt", "Esc", "Enter", "Del", "Tab", "Space", "Win",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    "px", "PinShot", "OCR", "RGB", "HEX", "HSV", "HSL", "HSI",
})

STRINGS = {
    # tray / fallback
    "PinShot": {"en": "PinShot", "zh": "PinShot"},
    "Snip": {"en": "Snip", "zh": "截图"},
    "Pin clipboard": {"en": "Pin clipboard", "zh": "贴剪贴板"},
    "Hide/Show pins": {"en": "Hide/Show pins", "zh": "隐藏/显示贴图"},
    "Hide/Show all pins": {"en": "Hide/Show all pins", "zh": "隐藏/显示全部贴图"},
    "Restore last closed pin": {"en": "Restore last closed pin", "zh": "恢复最近关闭的贴图"},
    "Preferences…": {"en": "Preferences…", "zh": "首选项…"},
    "Quit": {"en": "Quit", "zh": "退出"},
    "Groups": {"en": "Groups", "zh": "分组"},
    "Snip and translate": {"en": "Snip and translate", "zh": "截图翻译"},
    "PinShot is running": {"en": "PinShot is running", "zh": "PinShot 正在运行"},
    # prefs
    "PinShot Preferences": {"en": "PinShot Preferences", "zh": "PinShot 首选项"},
    "General": {"en": "General", "zh": "常规"},
    "Hotkeys": {"en": "Hotkeys", "zh": "热键"},
    "Output": {"en": "Output", "zh": "输出"},
    "Advanced": {"en": "Advanced", "zh": "高级"},
    "Apply": {"en": "Apply", "zh": "应用"},
    "OK": {"en": "OK", "zh": "确定"},
    "Cancel": {"en": "Cancel", "zh": "取消"},
    "Run at login": {"en": "Run at login", "zh": "登录时启动"},
    "Snip delay:": {"en": "Snip delay:", "zh": "截图延时："},
    "None (0 s)": {"en": "None (0 s)", "zh": "无（0 秒）"},
    "Confirm before abort (Esc)": {"en": "Confirm before abort (Esc)", "zh": "放弃截图前确认 (Esc)"},
    "Double-click pin:": {"en": "Double-click pin:", "zh": "双击贴图："},
    "Close pin": {"en": "Close pin", "zh": "关闭贴图"},
    "Hide pin": {"en": "Hide pin", "zh": "隐藏贴图"},
    "Do nothing": {"en": "Do nothing", "zh": "无操作"},
    "Snip history:": {"en": "Snip history:", "zh": "截图历史："},
    "Closed pins:": {"en": "Closed pins:", "zh": "可恢复贴图："},
    "Pin groups:": {"en": "Pin groups:", "zh": "贴图分组："},
    "Add": {"en": "Add", "zh": "添加"},
    "Rename": {"en": "Rename", "zh": "重命名"},
    "Remove": {"en": "Remove", "zh": "删除"},
    "Language:": {"en": "Language:", "zh": "语言："},
    "Theme:": {"en": "Theme:", "zh": "主题："},
    "Light": {"en": "Light", "zh": "浅色"},
    "Dark": {"en": "Dark", "zh": "深色"},
    "Follow system": {"en": "Follow system", "zh": "跟随系统"},
    "Snip:": {"en": "Snip:", "zh": "截图："},
    "Pin clipboard:": {"en": "Pin clipboard:", "zh": "贴剪贴板："},
    "Hide/show pins:": {"en": "Hide/show pins:", "zh": "隐藏/显示贴图："},
    "Click-through:": {"en": "Click-through:", "zh": "鼠标穿透："},
    "Restore last pin:": {"en": "Restore last pin:", "zh": "恢复关闭的贴图："},
    "Select-text translate:": {"en": "Select-text translate:", "zh": "划词翻译："},
    "Snip and translate:": {"en": "Snip and translate:", "zh": "截图翻译："},
    "F1, Shift+F3": {"en": "F1, Shift+F3", "zh": "F1、Shift+F3"},
    "Hotkeys already used by another app cannot be bound. Click Apply to rebind.":
        {"en": "Hotkeys already used by another app cannot be bound. Click Apply to rebind.",
         "zh": "已被其他程序占用的组合无法绑定。点击应用后重新绑定。"},
    "Save folder:": {"en": "Save folder:", "zh": "保存文件夹："},
    "Browse…": {"en": "Browse…", "zh": "浏览…"},
    "Quality:": {"en": "Quality:", "zh": "质量："},
    "Name:": {"en": "Name:", "zh": "名称："},
    "Add group": {"en": "Add group", "zh": "添加分组"},
    "Rename group": {"en": "Rename group", "zh": "重命名分组"},
    "New group": {"en": "New group", "zh": "新建分组"},
    "The Default group cannot be removed.": {"en": "The Default group cannot be removed.",
                                             "zh": "默认分组不能删除。"},
    "Could not update autostart:": {"en": "Could not update autostart:", "zh": "无法更新自启动："},
    "Quick-save folder": {"en": "Quick-save folder", "zh": "快速保存文件夹"},
    # toolbar
    "Select (V)": {"en": "Select (V)", "zh": "选择 (V)"},
    "Rectangle": {"en": "Rectangle", "zh": "矩形"},
    "Ellipse": {"en": "Ellipse", "zh": "椭圆"},
    "Line / arrow (Shift)": {"en": "Line / arrow (Shift)", "zh": "直线 / 箭头 (Shift)"},
    "Pencil": {"en": "Pencil", "zh": "铅笔"},
    "Marker": {"en": "Marker", "zh": "记号笔"},
    "Mosaic / blur": {"en": "Mosaic / blur", "zh": "马赛克 / 模糊"},
    "Text": {"en": "Text", "zh": "文字"},
    "Number (N)": {"en": "Number (N)", "zh": "序号 (N)"},
    "Eraser": {"en": "Eraser", "zh": "橡皮擦"},
    "Undo (Ctrl+Z)": {"en": "Undo (Ctrl+Z)", "zh": "撤销 (Ctrl+Z)"},
    "Redo (Ctrl+Y)": {"en": "Redo (Ctrl+Y)", "zh": "重做 (Ctrl+Y)"},
    "Cancel (Esc)": {"en": "Cancel (Esc)", "zh": "取消 (Esc)"},
    "Pin (Ctrl+T)": {"en": "Pin (Ctrl+T)", "zh": "贴到屏幕 (Ctrl+T)"},
    "Save (Ctrl+S)": {"en": "Save (Ctrl+S)", "zh": "另存为 (Ctrl+S)"},
    "Copy (Ctrl+C)": {"en": "Copy (Ctrl+C)", "zh": "复制 (Ctrl+C)"},
    "Done (Space)": {"en": "Done (Space)", "zh": "完成 (Space)"},
    "Custom color": {"en": "Custom color", "zh": "自定义颜色"},
    "Annotation color": {"en": "Annotation color", "zh": "标注颜色"},
    "OCR": {"en": "OCR", "zh": "OCR"},
    "Translate": {"en": "Translate", "zh": "翻译"},
    "Recognize text": {"en": "Recognize text", "zh": "识别文字"},
    "Recognize and translate": {"en": "Recognize and translate", "zh": "识别并翻译"},
    # sub-panels
    "Outline": {"en": "Outline", "zh": "描边"},
    "Filled": {"en": "Filled", "zh": "填充"},
    "Width": {"en": "Width", "zh": "粗细"},
    "Size": {"en": "Size", "zh": "大小"},
    "Arrow": {"en": "Arrow", "zh": "箭头"},
    "Arrow at end": {"en": "Arrow at end", "zh": "末端箭头"},
    "Brush": {"en": "Brush", "zh": "画笔"},
    "Rect": {"en": "Rect", "zh": "矩形"},
    "Mosaic": {"en": "Mosaic", "zh": "马赛克"},
    "Blur": {"en": "Blur", "zh": "模糊"},
    "Bold": {"en": "Bold", "zh": "粗体"},
    "Italic": {"en": "Italic", "zh": "斜体"},
    "Outline / glow": {"en": "Outline / glow", "zh": "描边 / 光晕"},
    "Filled circle": {"en": "Filled circle", "zh": "实心圆"},
    "Click, drag, Del, double-click text": {"en": "Click, drag, Del, double-click text",
                                            "zh": "单击、拖动、Del、双击文字"},
    "Solid": {"en": "Solid", "zh": "实线"},
    "Dash": {"en": "Dash", "zh": "虚线"},
    "Dot": {"en": "Dot", "zh": "点线"},
    # overlay
    "Abort snipping?": {"en": "Abort snipping?", "zh": "放弃截图？"},
    "Do not show again": {"en": "Do not show again", "zh": "不再提示"},
    "Yes": {"en": "Yes", "zh": "是"},
    "No": {"en": "No", "zh": "否"},
    "Press C to copy color": {"en": "Press C to copy color", "zh": "按 C 复制颜色"},
    "Shift: RGB / HEX": {"en": "Shift: RGB / HEX", "zh": "Shift：切换 RGB / HEX"},
    "Move cursor 1 px": {"en": "Move cursor 1 px", "zh": "光标移动 1 px"},
    "Move selection 1 px": {"en": "Move selection 1 px", "zh": "选区移动 1 px"},
    "Grow / Shift: shrink": {"en": "Grow / Shift: shrink", "zh": "扩大选区 / Shift：缩小"},
    "Window / element": {"en": "Window / element", "zh": "窗口 / 控件"},
    "Full screen": {"en": "Full screen", "zh": "全屏"},
    "Last snip area": {"en": "Last snip area", "zh": "上次选区"},
    "Snip history": {"en": "Snip history", "zh": "截图历史"},
    "Show cursor": {"en": "Show cursor", "zh": "显示指针"},
    "Copy color / RGB↔HEX": {"en": "Copy color / RGB↔HEX", "zh": "复制颜色 / RGB↔HEX"},
    "Numbered marker": {"en": "Numbered marker", "zh": "序号标记"},
    "Select / edit": {"en": "Select / edit", "zh": "选择 / 编辑"},
    "Finish shape": {"en": "Finish shape", "zh": "完成图形"},
    "Square / polyline": {"en": "Square / polyline", "zh": "正方形 / 折线"},
    "Undo / redo": {"en": "Undo / redo", "zh": "撤销 / 重做"},
    "Copy": {"en": "Copy", "zh": "复制"},
    "Pin": {"en": "Pin", "zh": "贴图"},
    "Save": {"en": "Save", "zh": "保存"},
    "Select": {"en": "Select", "zh": "选择"},
    "Delete selected": {"en": "Delete selected", "zh": "删除所选"},
    "Place text": {"en": "Place text", "zh": "放置文本"},
    "Commit text": {"en": "Commit text", "zh": "确认文字"},
    "Discard text": {"en": "Discard text", "zh": "放弃文字"},
    "Font size": {"en": "Font size", "zh": "字号"},
    "Finish": {"en": "Finish", "zh": "完成"},
    "Select shape": {"en": "Select shape", "zh": "选择图形"},
    "Move": {"en": "Move", "zh": "移动"},
    "Resize": {"en": "Resize", "zh": "缩放"},
    "Delete": {"en": "Delete", "zh": "删除"},
    "Re-edit": {"en": "Re-edit", "zh": "重新编辑"},
    "Undo edit": {"en": "Undo edit", "zh": "撤销编辑"},
    "Place next number": {"en": "Place next number", "zh": "放置下一个序号"},
    "Click": {"en": "Click", "zh": "单击"},
    "Drag": {"en": "Drag", "zh": "拖动"},
    "Handles": {"en": "Handles", "zh": "手柄"},
    "Double-click text": {"en": "Double-click text", "zh": "双击文字"},
    "Right-click": {"en": "Right-click", "zh": "右键"},
    "1 2 / wheel": {"en": "1 2 / wheel", "zh": "1 2 / 滚轮"},
    "Default": {"en": "Default", "zh": "默认"},
    "Save snip": {"en": "Save snip", "zh": "保存截图"},
    "Window detection": {"en": "Window detection", "zh": "窗口检测"},
    "Element detection": {"en": "Element detection", "zh": "控件检测"},
    "Cursor shown": {"en": "Cursor shown", "zh": "指针：显示"},
    "Cursor hidden": {"en": "Cursor hidden", "zh": "指针：隐藏"},
    "Select text, then release": {"en": "Select text, then release", "zh": "框选文字后松开"},
    # pin
    "CLICK-THRU": {"en": "CLICK-THRU", "zh": "穿透"},
    "Copy image": {"en": "Copy image", "zh": "复制图像"},
    "Save image as…": {"en": "Save image as…", "zh": "图像另存为…"},
    "Save image as": {"en": "Save image as", "zh": "图像另存为"},
    "Show toolbar": {"en": "Show toolbar", "zh": "显示工具栏"},
    "Shadow": {"en": "Shadow", "zh": "阴影"},
    "Click-through": {"en": "Click-through", "zh": "鼠标穿透"},
    "Zoom": {"en": "Zoom", "zh": "缩放"},
    "Image": {"en": "Image", "zh": "图像"},
    "Rotate left": {"en": "Rotate left", "zh": "向左旋转"},
    "Rotate right": {"en": "Rotate right", "zh": "向右旋转"},
    "Flip horizontal": {"en": "Flip horizontal", "zh": "水平翻转"},
    "Flip vertical": {"en": "Flip vertical", "zh": "垂直翻转"},
    "Paste": {"en": "Paste", "zh": "粘贴"},
    "Replace from file…": {"en": "Replace from file…", "zh": "用文件替换…"},
    "Replace from file": {"en": "Replace from file", "zh": "用文件替换"},
    "Move to group": {"en": "Move to group", "zh": "移动到分组"},
    "New group…": {"en": "New group…", "zh": "新建分组…"},
    "Rename group…": {"en": "Rename group…", "zh": "重命名此分组…"},
    "View in folder": {"en": "View in folder", "zh": "在文件夹中查看"},
    "Close": {"en": "Close", "zh": "关闭"},
    "Destroy": {"en": "Destroy", "zh": "销毁"},
    "Copy size": {"en": "Copy size", "zh": "复制尺寸"},
    "Reset zoom": {"en": "Reset zoom", "zh": "重置缩放"},
    "Tip: double-click closes a pin.": {"en": "Tip: double-click closes a pin.",
                                        "zh": "提示：双击关闭贴图。"},
    "Tip: double-click hides a pin.": {"en": "Tip: double-click hides a pin.",
                                       "zh": "提示：双击隐藏贴图。"},
    "Double-click again to confirm.": {"en": "Double-click again to confirm.",
                                       "zh": "再次双击以确认。"},
    # toasts / errors
    "Copied to clipboard": {"en": "Copied to clipboard", "zh": "已复制到剪贴板"},
    "Clipboard has no image or text": {"en": "Clipboard has no image or text",
                                       "zh": "剪贴板中没有图片或文字"},
    "No closed pins to restore": {"en": "No closed pins to restore", "zh": "没有可恢复的已关闭贴图"},
    "Restored closed pin": {"en": "Restored closed pin", "zh": "已恢复关闭的贴图"},
    "No pin under cursor": {"en": "No pin under cursor", "zh": "光标下没有贴图"},
    "Rename rejected": {"en": "Rename rejected", "zh": "重命名被拒绝"},
    "Click-through on": {"en": "Click-through on", "zh": "鼠标穿透 开"},
    "Click-through off": {"en": "Click-through off", "zh": "鼠标穿透 关"},
    "unbound": {"en": "unbound", "zh": "未绑定"},
    "unavailable": {"en": "unavailable", "zh": "不可用"},
    "Could not copy selected text.": {"en": "Could not copy selected text.",
                                      "zh": "无法复制选中文字。"},
    "Select-text translate is only available here.":
        {"en": "Select-text translate is only available here.",
         "zh": "划词翻译在当前系统不可用。"},
    "No translation returned": {"en": "No translation returned", "zh": "没有返回译文"},
    "No text to translate": {"en": "No text to translate", "zh": "没有可翻译的文字"},
    # OCR / translation window
    "PinShot OCR": {"en": "PinShot OCR", "zh": "PinShot 文字识别与翻译"},
    "Original (editable)": {"en": "Original (editable)", "zh": "原文（可编辑）"},
    "Translation": {"en": "Translation", "zh": "译文"},
    "Copy original": {"en": "Copy original", "zh": "复制原文"},
    "Copy translation": {"en": "Copy translation", "zh": "复制译文"},
    "OCR runs on this device. Translation sends text online.":
        {"en": "OCR runs on this device. Translation sends text online.",
         "zh": "OCR 在本机运行。翻译会将原文发送到在线服务。"},
    "No selected text. Select text, then press the hotkey.":
        {"en": "No selected text. Select text, then press the hotkey.",
         "zh": "未获取到选中文字。请先选中文字，再按划词翻译热键。"},
    "Recognizing…": {"en": "Recognizing…", "zh": "正在识别文字…"},
    "Translating…": {"en": "Translating…", "zh": "正在翻译…"},
    "OCR done. Edit, copy, or translate.": {"en": "OCR done. Edit, copy, or translate.",
                                            "zh": "识别完成，可编辑、复制或翻译。"},
    "No text found. Select a clearer area.": {"en": "No text found. Select a clearer area.",
                                              "zh": "未识别到文字，请另选更清晰的区域。"},
    "Translation done": {"en": "Translation done", "zh": "翻译完成"},
    # cards
    "RGB": {"en": "RGB", "zh": "RGB"},
    "RGB float": {"en": "RGB float", "zh": "RGB 浮点"},
    "HSV": {"en": "HSV", "zh": "HSV"},
    "HSL": {"en": "HSL", "zh": "HSL"},
    "HSI": {"en": "HSI", "zh": "HSI"},
    "HEX": {"en": "HEX", "zh": "HEX"},
}

PATTERNS = [
    (r"^Zoom: (\d+)%$", {"en": "Zoom: {0}%", "zh": "缩放：{0}%"}),
    (r"^Opacity: (\d+)%$", {"en": "Opacity: {0}%", "zh": "不透明度：{0}%"}),
    (r"^Zoom (\d+)%  ·  Opacity (\d+)%$", {"en": "Zoom {0}%  ·  Opacity {1}%",
                                           "zh": "缩放 {0}%  ·  不透明度 {1}%"}),
    (r"^Color copied: (.+)$", {"en": "Color copied: {0}", "zh": "已复制颜色：{0}"}),
    (r"^Copied (.+)$", {"en": "Copied {0}", "zh": "已复制 {0}"}),
    (r"^Saved (.+)$", {"en": "Saved {0}", "zh": "已保存 {0}"}),
    (r"^Cleared click-through on (\d+) pin\(s\)$",
     {"en": "Cleared click-through on {0} pin(s)", "zh": "已清除 {0} 个贴图的鼠标穿透"}),
    (r"^Group '(.+)' already exists$", {"en": "Group '{0}' already exists", "zh": "分组“{0}”已存在"}),
    (r"^Group (\d+)$", {"en": "Group {0}", "zh": "分组 {0}"}),
    (r"^Hide (.+)$", {"en": "Hide {0}", "zh": "隐藏 {0}"}),
    (r"^Show (.+)$", {"en": "Show {0}", "zh": "显示 {0}"}),
    (r"^(\d+) s$", {"en": "{0} s", "zh": "{0} 秒"}),
    (r"^(.+)  \(hidden\)$", {"en": "{0}  (hidden)", "zh": "{0}  （已隐藏）"}),
    (r"^OCR failed: (.+)$", {"en": "OCR failed: {0}", "zh": "识别失败：{0}"}),
    (r"^Translation failed: (.+)$", {"en": "Translation failed: {0}", "zh": "翻译失败：{0}"}),
    (r"^Unbound hotkeys: (.+)$", {"en": "Unbound hotkeys: {0}", "zh": "未注册的热键：{0}"}),
]

_PAT = [(re.compile(a), b) for a, b in PATTERNS]
_lang = "en"


def lang():
    return _lang


def _code():
    return "zh" if _lang.startswith("zh") else "en"


def table_entries():
    """Yield (key, langs) for STRINGS and PATTERNS (pattern key is the regex)."""
    for key, langs in STRINGS.items():
        yield key, langs
    for rx, langs in PATTERNS:
        yield rx, langs


def _tr_line(s):
    rec = STRINGS.get(s)
    if rec is None:
        st = s.strip()
        if st and st != s and st in STRINGS:
            return s.replace(st, STRINGS[st][_code()])
    if rec:
        return rec[_code()]
    for rx, langs in _PAT:
        m = rx.match(s)
        if m:
            g = [(_tr_line(x) if x else "") for x in m.groups()]
            return langs[_code()].format(*g).replace("（）", "")
    return s


def tr(s):
    if not isinstance(s, str) or not s:
        return s
    if _code() == "en":
        return s
    return "\n".join(_tr_line(x) for x in s.split("\n"))


def group_label(name):
    """Display name for a stored group id: built-in 'Default' / auto 'Group N' are translated,
    user-created names are shown verbatim. The stored id never changes."""
    if name == "Default" or re.fullmatch(r"Group \d+", name or ""):
        return tr(name)
    return name


# ---------- live widget translation
_P = "_i18n_"
_SKIP_NAMES = {"language", "target_language", "font"}


def _apply(obj, key, cur, setter):
    """cur = text currently shown. Track (original English, last value we set)."""
    orig, last = obj.property(_P + key + "o"), obj.property(_P + key + "l")
    if orig is None or cur != last:
        orig = cur
    new = tr(orig)
    obj.setProperty(_P + key + "o", orig)
    obj.setProperty(_P + key + "l", new)
    if new != cur:
        setter(new)


def translate_widget(w):
    try:
        if w.property("i18n") is False or w.objectName() in _SKIP_NAMES:
            return
        if isinstance(w, QLabel) and w.text():
            _apply(w, "t", w.text(), w.setText)
        elif isinstance(w, QAbstractButton) and w.text():
            _apply(w, "t", w.text(), w.setText)
        if w.toolTip():
            _apply(w, "tip", w.toolTip(), w.setToolTip)
        if w.isWindow() and w.windowTitle():
            _apply(w, "wt", w.windowTitle(), w.setWindowTitle)
        if isinstance(w, QGroupBox) and w.title():
            _apply(w, "gt", w.title(), w.setTitle)
        if isinstance(w, QLineEdit) and w.placeholderText():
            _apply(w, "ph", w.placeholderText(), w.setPlaceholderText)
        if isinstance(w, QTabWidget):
            for i in range(w.count()):
                _apply(w.widget(i), "tab", w.tabText(i), lambda v, i=i: w.setTabText(i, v))
        if isinstance(w, QComboBox) and not isinstance(w, QFontComboBox):
            for i in range(w.count()):
                _apply(w, f"ci{i}", w.itemText(i), lambda v, i=i: w.setItemText(i, v))
        if isinstance(w, QMenu):
            if w.title():
                _apply(w, "mt", w.title(), w.setTitle)
            for a in w.actions():
                if a.text():
                    _apply(a, "t", a.text(), a.setText)
                if a.menu() is not None and a.menu().title():
                    _apply(a.menu(), "mt", a.menu().title(), a.menu().setTitle)
    except RuntimeError:
        pass


def retranslate_all():
    for w in QApplication.allWidgets():
        translate_widget(w)


def _skip_scan(w):
    if w.property("i18n") is False or w.objectName() in _SKIP_NAMES:
        return True
    return isinstance(w, (QLineEdit, QSpinBox, QAbstractSlider, QFontComboBox, QListWidget, QTextEdit))


def visible_ui_strings(root=None):
    """Collect visible widget texts/tooltips/menu actions for script checks."""
    app = QApplication.instance()
    widgets = []
    if root is not None:
        widgets.append(root)
        widgets.extend(root.findChildren(QWidget))
        if isinstance(root, QMenu):
            pass
    else:
        widgets = list(QApplication.topLevelWidgets())
        extra = []
        for w in widgets:
            extra.extend(w.findChildren(QWidget))
        widgets.extend(extra)
    seen = []
    for w in widgets:
        try:
            if _skip_scan(w):
                continue
            if not w.isVisible() and not isinstance(w, QMenu):
                continue
            if isinstance(w, QLabel) and w.text():
                seen.append(("label", w.text()))
            elif isinstance(w, QAbstractButton) and w.text():
                seen.append(("button", w.text()))
            if w.toolTip():
                seen.append(("tip", w.toolTip()))
            if w.isWindow() and w.windowTitle():
                seen.append(("title", w.windowTitle()))
            if isinstance(w, QGroupBox) and w.title():
                seen.append(("group", w.title()))
            if isinstance(w, QTabWidget):
                for i in range(w.count()):
                    seen.append(("tab", w.tabText(i)))
            if isinstance(w, QComboBox) and not isinstance(w, QFontComboBox):
                for i in range(w.count()):
                    seen.append(("combo", w.itemText(i)))
            if isinstance(w, QMenu):
                if w.title():
                    seen.append(("menu", w.title()))
                for a in w.actions():
                    if a.text():
                        seen.append(("action", a.text()))
        except RuntimeError:
            pass
    if app is not None:
        for w in app.topLevelWidgets():
            if isinstance(w, QMenu) and w.isVisible():
                for a in w.actions():
                    if a.text():
                        seen.append(("action", a.text()))
    return seen


class Translator(QObject):
    languageChanged = Signal(str)

    def __init__(self, app):
        super().__init__(app)
        app.installEventFilter(self)
        self.timer = QTimer(self, interval=600)
        self.timer.timeout.connect(self._refresh)
        self.timer.start()

    def eventFilter(self, obj, ev):
        t = ev.type()
        if QApplication.activePopupWidget() is not None:
            return False
        if t in (QEvent.Show, QEvent.Polish) and isinstance(obj, QWidget):
            translate_widget(obj)
            if t == QEvent.Show and obj.isWindow():
                for c in obj.findChildren(QWidget):
                    translate_widget(c)
        return False

    def _refresh(self):
        if _lang == "en" and not getattr(self, "_dirty", False):
            return
        if QApplication.activePopupWidget() is not None:
            return   # never mutate combos/menus while their popup is open (breaks mouse selection)
        self._dirty = False
        for top in QApplication.topLevelWidgets():
            if top.isVisible():
                translate_widget(top)
                for c in top.findChildren(QWidget):
                    if c.isVisible():
                        translate_widget(c)

    def set_language(self, code):
        global _lang
        code = code if code in dict(LANGS) else "en"
        if code == _lang:
            return
        _lang = code
        self._dirty = True
        retranslate_all()
        for w in QApplication.topLevelWidgets():
            try:
                w.update()
            except RuntimeError:
                pass
        self.languageChanged.emit(code)


TRANSLATOR = None


def install(app, code="en"):
    global TRANSLATOR, _lang
    if TRANSLATOR is None:
        TRANSLATOR = Translator(app)
    TRANSLATOR.set_language(code)
    return TRANSLATOR

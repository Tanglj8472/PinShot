"""Tiny live i18n: English source strings -> Simplified Chinese.

* tr(text) translates a string (multi-line aware; regex patterns cover dynamic strings).
* Translator (app event filter + light refresh) rewrites widget texts/tooltips/titles/menu actions/
  combo items in place and remembers the English original, so switching language is live and
  reversible. Painted strings (overlay hints, toasts, badges, cards) call tr() at paint time.
"""
import re
from PySide6.QtCore import QObject, QEvent, QTimer, Signal
from PySide6.QtWidgets import (QApplication, QWidget, QLabel, QAbstractButton, QGroupBox, QTabWidget,
                               QComboBox, QFontComboBox, QLineEdit, QMenu, QDialog, QListWidget)

LANGS = [("en", "English"), ("zh_CN", "简体中文")]

ZH = {
    # control window / tray
    "PinShot": "PinShot", "Snip (F1)": "截图 (F1)", "Pin clipboard (F3)": "贴剪贴板 (F3)",
    "Hide/Show pins (Shift+F3)": "隐藏/显示贴图 (Shift+F3)", "Restore last closed pin": "恢复最近关闭的贴图",
    "Preferences…": "首选项…", "Pin groups (double-click hide/show):": "贴图分组（双击隐藏/显示）：",
    "Add": "添加", "Rename": "重命名", "Remove": "删除", "Quit": "退出", "Snip delay:": "截图延时：",
    "Snip": "截图", "Pin clipboard": "贴剪贴板", "Hide/Show all pins": "隐藏/显示全部贴图", "Groups": "分组",
    "Control window": "控制窗口", "Add group": "添加分组", "Rename group": "重命名分组", "New group": "新建分组",
    "Name:": "名称：",
    "unbound": "未绑定", "unavailable (X11 GrabKey / pynput; not on pure Wayland)": "不可用（X11 GrabKey / pynput；纯 Wayland 下无效）",
    "Clipboard has no image or text": "剪贴板中没有图片或文字", "No closed pins to restore": "没有可恢复的已关闭贴图",
    "Restored closed pin": "已恢复关闭的贴图", "No pin under cursor": "光标下没有贴图", "Copied to clipboard": "已复制到剪贴板",
    "Rename rejected (duplicate or Default)": "重命名被拒绝（重复或为默认分组）",
    # toolbar
    "Select / move / resize (V)": "选择 / 移动 / 缩放 (V)", "Rectangle": "矩形", "Ellipse": "椭圆",
    "Line / arrow (Shift+drag continues polyline)": "直线 / 箭头（Shift+拖动继续折线）", "Pencil": "铅笔",
    "Marker": "记号笔", "Mosaic / blur": "马赛克 / 模糊", "Text": "文字", "Numbered marker (N)": "序号标记 (N)",
    "Eraser": "橡皮擦", "Undo (Ctrl+Z)": "撤销 (Ctrl+Z)", "Redo (Ctrl+Y)": "重做 (Ctrl+Y)", "Cancel (Esc)": "取消 (Esc)",
    "Pin to screen (Ctrl+T)": "贴到屏幕 (Ctrl+T)", "Save as… (Ctrl+S)": "另存为… (Ctrl+S)",
    "Copy (Ctrl+C / Enter)": "复制 (Ctrl+C / Enter)", "Finish editing (Space)": "完成编辑 (空格)",
    "Custom color… (with alpha)": "自定义颜色…（含透明度）", "Annotation color": "标注颜色",
    # sub-panels
    "Outline": "描边", "Filled": "填充", "Width": "粗细", "Size": "大小", "Arrow": "箭头", "Arrow at end": "末端箭头",
    "Brush": "画笔", "Rect": "矩形", "Mosaic": "马赛克", "Blur": "模糊", "Bold": "粗体", "Italic": "斜体",
    "Outline / halo": "描边 / 光晕", "Filled circle": "实心圆",
    "Click to select · drag move · handles resize · Del delete · dbl-click text":
        "单击选择 · 拖动移动 · 手柄缩放 · Del 删除 · 双击编辑文字",
    # overlay
    "Abort snipping?": "放弃截图？", "Do not show again": "不再提示", "Yes": "是", "No": "否",
    "Press C to copy color": "按 C 复制颜色", "Shift: switch RGB/HEX": "Shift：切换 RGB/HEX",
    "Move the cursor by 1 pixel": "光标移动 1 像素", "Move the selection by 1 pixel": "选区移动 1 像素",
    "Enlarge selection / Shift: shrink": "扩大选区 / Shift：缩小", "Toggle window / element detection": "切换窗口 / 元素检测",
    "Select full screen": "全屏选择", "Reuse the last snip area": "复用上次截图区域",
    "Replay snip history (image+area)": "回放截图历史（图像+区域）", "Show/hide captured mouse cursor": "显示/隐藏捕获的鼠标指针",
    "Copy color / RGB↔HEX": "复制颜色 / RGB↔HEX", "Numbered marker": "序号标记", "Select / edit shapes": "选择 / 编辑图形",
    "Decrease / increase width": "减小 / 增大粗细", "Finish current shape": "完成当前图形",
    "Square shape / continue polyline": "正方形 / 继续折线", "Undo / redo": "撤销 / 重做", "Copy": "复制", "Pin": "贴图",
    "Save": "保存", "Cancel": "取消", "Select": "选择", "Delete selected": "删除所选", "Place a text box": "放置文本框",
    "Commit text": "确认文字", "Discard text": "放弃文字", "Font size": "字号", "Finish": "完成", "Select shape": "选择图形",
    "Move": "移动", "Resize": "缩放", "Delete": "删除", "Re-edit": "重新编辑", "Undo edit": "撤销编辑",
    "Place next number": "放置下一个序号", "Click": "单击", "Drag": "拖动", "Handles": "手柄",
    "Double-click text": "双击文字", "Right-click": "右键", "Size label": "尺寸", "Save snip": "保存截图",
    "shown": "显示", "hidden": "隐藏", "window": "窗口", "element": "元素", "AT-SPI": "AT-SPI", "X11 children": "X11 子窗口",
    # pin
    "CLICK-THRU": "鼠标穿透", "Copy image": "复制图像", "Save image as…": "图像另存为…", "Show toolbar": "显示工具栏",
    "Shadow": "阴影", "Click-through": "鼠标穿透", "Zoom": "缩放", "Image processing": "图像处理",
    "Rotate left": "向左旋转", "Rotate right": "向右旋转", "Horizontal flip": "水平翻转", "Vertical flip": "垂直翻转",
    "Paste": "粘贴", "Replace by file…": "用文件替换…", "Move to group": "移动到分组", "New group…": "新建分组…",
    "Rename this group…": "重命名此分组…", "View in folder": "在文件夹中查看", "Close": "关闭", "Destroy": "销毁",
    "Copy size": "复制尺寸", "Reset zoom to 100%": "重置缩放为 100%", "Save image as": "图像另存为",
    "Tip: double-click closes a pin.": "提示：双击关闭贴图。", "Tip: double-click hides a pin.": "提示：双击隐藏贴图。",
    "Double-click again to confirm.": "再次双击以确认。",
    # prefs
    "PinShot Preferences": "PinShot 首选项", "General": "常规", "Hotkeys": "热键", "Control / Mouse": "控制 / 鼠标",
    "Output / Save": "输出 / 保存", "Apply": "应用", "OK": "确定", "Appearance": "外观",
    "Run at login (writes ~/.config/autostart/pinshot.desktop)": "登录时启动（写入 ~/.config/autostart/pinshot.desktop）",
    "Run at login (HKCU\\...\\CurrentVersion\\Run)": "登录时启动（HKCU\\...\\CurrentVersion\\Run）",
    "UI Automation": "UI 自动化", "child windows": "子窗口",
    "Start minimized (control window hidden; tray / hotkeys still work)": "启动时最小化（隐藏控制窗口；托盘/热键仍可用）",
    "None (0 s)": "无 (0 秒)", "Confirm before aborting a snip (Esc)": "放弃截图前确认 (Esc)",
    "Close pin": "关闭贴图", "Hide pin": "隐藏贴图", "Do nothing": "无操作", "Double-click pin:": "双击贴图：",
    "Snip history size (images):": "截图历史数量（图像）：", "Recoverable closed pins:": "可恢复的已关闭贴图：",
    "Pin groups:": "贴图分组：", "Snip:": "截图：", "Pin clipboard:": "贴剪贴板：", "Hide / show all pins:": "隐藏/显示全部贴图：",
    "Toggle click-through:": "切换鼠标穿透：", "Restore last closed pin:": "恢复最近关闭的贴图：",
    "e.g. F1, Shift+F3, Ctrl+Alt+T": "例如 F1、Shift+F3、Ctrl+Alt+T",
    "Global hotkeys use X11 GrabKey on this box (pynput unavailable). They do not work on pure Wayland. Apply to re-bind.":
        "本机全局热键使用 X11 GrabKey（pynput 不可用），纯 Wayland 下无效。点击“应用”重新绑定。",
    "Mouse / control options shared with General (abort confirm, double-click).": "鼠标/控制选项与“常规”共享（放弃确认、双击）。",
    "Global hotkeys use Win32 RegisterHotKey; a combination already taken by another app cannot be bound. Apply to re-bind.":
        "全局热键使用 Win32 RegisterHotKey；已被其他程序占用的组合无法绑定。点击“应用”重新绑定。",
    "Click-through hotkey is under Hotkeys.": "鼠标穿透热键在“热键”页中。", "Browse…": "浏览…",
    "Quick-save folder:": "快速保存文件夹：", "JPEG quality:": "JPEG 质量：", "Language:": "语言：", "Theme:": "主题：",
    "Light": "浅色", "Dark": "深色", "Follow system": "跟随系统",
    "The Default group cannot be removed.": "默认分组不能删除。",
    # cards
    "RGB float": "RGB 浮点",
}

PATTERNS = [
    (r"^PinShot — (.+) snip, (.+) pin, (.+) hide/show$", "PinShot — {0} 截图，{1} 贴图，{2} 隐藏/显示"),
    (r"^Global hotkeys: (.+)$", "全局热键：{0}"),
    (r"^Preferences: Ctrl\+Shift\+P · Restore closed pin: (.+)$", "首选项：Ctrl+Shift+P · 恢复已关闭贴图：{0}"),
    (r"^Detection: (\w+)(?: \((.+)\))?$", "检测：{0}（{1}）"),
    (r"^Captured cursor: (\w+) \(stub\)$", "捕获指针：{0}（占位）"),
    (r"^Opacity: (\d+)%$", "不透明度：{0}%"), (r"^Zoom: (\d+)%$", "缩放：{0}%"),
    (r"^Zoom (\d+)%  ·  Opacity (\d+)%$", "缩放 {0}%  ·  不透明度 {1}%"),
    (r"^Color copied: (.+)$", "已复制颜色：{0}"), (r"^Copied (.+)$", "已复制 {0}"), (r"^Saved (.+)$", "已保存 {0}"),
    (r"^Cleared click-through on (\d+) pin\(s\)$", "已清除 {0} 个贴图的鼠标穿透"),
    (r"^Group '(.+)' already exists$", "分组“{0}”已存在"),
    (r"^Hide (.+)$", "隐藏 {0}"), (r"^Show (.+)$", "显示 {0}"),
    (r"^(\d+) s$", "{0} 秒"), (r"^(.+)  \(hidden\)$", "{0}  （已隐藏）"),
    (r"^Click-through (ON|OFF)(.*)$", "鼠标穿透 {0}{1}"),
    (r"^Could not update autostart desktop file:$", "无法更新自启动文件："),
]
_PAT = [(re.compile(a), b) for a, b in PATTERNS]
_SUBS = {"ON": "开", "OFF": "关"}

_lang = "en"


def lang(): return _lang


def _tr_line(s):
    if s in ZH: return ZH[s]
    st = s.strip()
    if st and st in ZH and st != s:
        return s.replace(st, ZH[st])
    for rx, tpl in _PAT:
        m = rx.match(s)
        if m:
            g = [(_tr_line(x) if x else "") for x in m.groups()]
            g = [_SUBS.get(x, x) for x in g]
            out = tpl.format(*g)
            return out.replace("（）", "")
    return s


def tr(s):
    if _lang == "en" or not isinstance(s, str) or not s: return s
    return "\n".join(_tr_line(x) for x in s.split("\n"))


# ---------- live widget translation
_P = "_i18n_"   # dynamic-property prefix


def _apply(obj, key, cur, setter):
    """cur = text currently shown. Track (original English, last value we set)."""
    orig, last = obj.property(_P + key + "o"), obj.property(_P + key + "l")
    if orig is None or cur != last:          # first time, or app code changed the text since
        orig = cur
    new = tr(orig)
    obj.setProperty(_P + key + "o", orig); obj.setProperty(_P + key + "l", new)
    if new != cur: setter(new)


def translate_widget(w):
    try:
        if isinstance(w, QLabel) and w.text(): _apply(w, "t", w.text(), w.setText)
        elif isinstance(w, QAbstractButton) and w.text(): _apply(w, "t", w.text(), w.setText)
        if w.toolTip(): _apply(w, "tip", w.toolTip(), w.setToolTip)
        if w.isWindow() and w.windowTitle(): _apply(w, "wt", w.windowTitle(), w.setWindowTitle)
        if isinstance(w, QGroupBox) and w.title(): _apply(w, "gt", w.title(), w.setTitle)
        if isinstance(w, QLineEdit) and w.placeholderText(): _apply(w, "ph", w.placeholderText(), w.setPlaceholderText)
        if isinstance(w, QTabWidget):
            for i in range(w.count()):
                _apply(w.widget(i), "tab", w.tabText(i), lambda v, i=i: w.setTabText(i, v))
        if isinstance(w, QComboBox) and not isinstance(w, QFontComboBox) and w.property("i18n") is not False \
                and w.objectName() != "line_style":
            for i in range(w.count()):
                k = f"ci{i}"
                _apply(w, k, w.itemText(i), lambda v, i=i: w.setItemText(i, v))
        if isinstance(w, QMenu):
            if w.title(): _apply(w, "mt", w.title(), w.setTitle)
            for a in w.actions():
                if a.text(): _apply(a, "t", a.text(), a.setText)
                if a.menu() is not None and a.menu().title(): _apply(a.menu(), "mt", a.menu().title(), a.menu().setTitle)
    except RuntimeError:
        pass   # deleted C++ object


def retranslate_all():
    for w in QApplication.allWidgets():
        translate_widget(w)


class Translator(QObject):
    languageChanged = Signal(str)

    def __init__(self, app):
        super().__init__(app)
        app.installEventFilter(self)
        # Visible-widget refresh catches setText() on already shown widgets (cheap; idempotent).
        self.timer = QTimer(self, interval=600); self.timer.timeout.connect(self._refresh); self.timer.start()

    def eventFilter(self, obj, ev):
        t = ev.type()
        if t in (QEvent.Show, QEvent.Polish) and isinstance(obj, QWidget):
            translate_widget(obj)
            if t == QEvent.Show and obj.isWindow():
                for c in obj.findChildren(QWidget): translate_widget(c)
        return False

    def _refresh(self):
        if _lang == "en" and not getattr(self, "_dirty", False): return
        self._dirty = False
        for top in QApplication.topLevelWidgets():
            if top.isVisible():
                translate_widget(top)
                for c in top.findChildren(QWidget):
                    if c.isVisible(): translate_widget(c)

    def set_language(self, code):
        global _lang
        code = code if code in dict(LANGS) else "en"
        if code == _lang: return
        _lang = code; self._dirty = True
        retranslate_all()
        for w in QApplication.topLevelWidgets():
            try: w.update()
            except RuntimeError: pass
        self.languageChanged.emit(code)


TRANSLATOR = None


def install(app, code="en"):
    global TRANSLATOR, _lang
    if TRANSLATOR is None: TRANSLATOR = Translator(app)
    TRANSLATOR.set_language(code)
    return TRANSLATOR

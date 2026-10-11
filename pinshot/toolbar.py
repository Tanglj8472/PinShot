"""Snip/pin annotation toolbar with per-tool floating sub-panels (PinShot's own widgets and icons)."""
from PySide6.QtCore import Qt, Signal, QSize
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (QWidget, QHBoxLayout, QVBoxLayout, QToolButton, QFrame, QSlider, QSpinBox,
                               QComboBox, QFontComboBox, QLabel, QButtonGroup, QGridLayout, QColorDialog, QApplication)
from .icons import icon
from .annotate import TOOLS, PALETTE

TIPS = {"select": "Select / move / resize (V)", "rect": "Rectangle", "ellipse": "Ellipse",
        "line": "Line / arrow (Shift+drag continues polyline)",
        "pen": "Pencil", "marker": "Marker", "mosaic": "Mosaic / blur", "text": "Text",
        "number": "Numbered marker (N)", "eraser": "Eraser",
        "undo": "Undo (Ctrl+Z)", "redo": "Redo (Ctrl+Y)", "close": "Cancel (Esc)", "pin": "Pin to screen (Ctrl+T)",
        "save": "Save as… (Ctrl+S)", "copy": "Copy (Ctrl+C / Enter)", "check": "Finish editing (Space)"}

CSS = """
QFrame#bar, QFrame#panel { background:#f7f7f7; border:1px solid #9a9a9a; border-radius:3px; }
QToolButton { border:none; padding:3px; border-radius:3px; background:transparent; }
QToolButton:hover { background:#dde8f5; }
QToolButton:checked { background:#c4d9f2; }
QFrame#sep { background:#c8c8c8; }
QLabel { color:#333; font-size:11px; }
"""


def _sep():
    s = QFrame(); s.setObjectName("sep"); s.setFixedSize(1, 18); return s


def _tb(name, tip=None, checkable=False):
    from . import theme as _th
    b = QToolButton(); b.setProperty("pinshot_icon", name); b.setIcon(icon(name, _th.icon_color())); b.setIconSize(QSize(20, 20)); b.setToolTip(tip or TIPS.get(name, name))  # i18n Translator rewrites live
    b.setCheckable(checkable); b.setFocusPolicy(Qt.NoFocus); b.setObjectName("tb_" + name); return b


class Palette(QWidget):
    def __init__(self, get, set_):
        super().__init__(); self.get, self.set_ = get, set_
        lay = QHBoxLayout(self); lay.setContentsMargins(0, 0, 0, 0); lay.setSpacing(4)
        self.cur = QToolButton(); self.cur.setFixedSize(30, 30); self.cur.setObjectName("current_color")
        self.cur.setToolTip("Custom color… (with alpha)"); self.cur.setFocusPolicy(Qt.NoFocus)
        self.cur.clicked.connect(self.custom); lay.addWidget(self.cur)
        grid = QGridLayout(); grid.setSpacing(2); lay.addLayout(grid)
        self.swatches = []
        for i, c in enumerate(PALETTE):
            b = QToolButton(); b.setFixedSize(14, 14); b.setFocusPolicy(Qt.NoFocus)
            b.setStyleSheet(f"QToolButton{{background:{c};border:1px solid #888;border-radius:0}}QToolButton:hover{{border:1px solid #000}}")
            b.clicked.connect(lambda _=False, c=c: self.pick(c)); grid.addWidget(b, i // 10, i % 10); self.swatches.append(b)
        self.refresh()

    def pick(self, c): self.set_(QColor(c)); self.refresh()

    def custom(self):
        """Open a topmost ApplicationModal color dialog.

        During snip the override-redirect overlay is hidden (not merely un-grabbed) so the dialog
        is actually visible and clickable; the overlay is shown again when the dialog closes.
        """
        host = self.window()
        ov = None
        w = host
        while w is not None:
            if w.__class__.__name__ == "SnipOverlay" and hasattr(w, "release_grabs_for_dialog"):
                ov = w; break
            w = w.parentWidget() if hasattr(w, "parentWidget") else None
        if ov is None:
            from .overlay import SnipOverlay
            for tl in QApplication.topLevelWidgets():
                if isinstance(tl, SnipOverlay):
                    ov = tl; break
        if ov is not None: ov.release_grabs_for_dialog()
        dlg = QColorDialog(self.get(), None)
        dlg.setOption(QColorDialog.ShowAlphaChannel, True)
        dlg.setWindowTitle("Annotation color")
        dlg.setWindowModality(Qt.ApplicationModal)
        dlg.setWindowFlag(Qt.WindowStaysOnTopHint, True)
        dlg.setWindowFlag(Qt.Dialog, True)
        dlg.show(); dlg.raise_(); dlg.activateWindow()
        QApplication.processEvents()
        try:
            if dlg.exec():
                self.set_(dlg.selectedColor()); self.refresh()
                if hasattr(host, "ann"): host.ann.changed.emit()
        finally:
            if ov is not None: ov.restore_grabs_after_dialog()

    def refresh(self):
        c = self.get()
        self.cur.setStyleSheet(f"QToolButton{{background:rgba({c.red()},{c.green()},{c.blue()},{c.alpha()});"
                               f"border:1px solid #555;border-radius:2px}}")


class ToolDock(QWidget):
    """Toolbar row + sub-panel. As a child of the snip overlay, or a top-level tool window under a pin."""
    action = Signal(str)

    def __init__(self, annotator, parent=None, for_pin=False):
        flags = Qt.Widget if parent else (Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint)
        super().__init__(parent, flags)
        self.ann = annotator; self.for_pin = for_pin
        from . import theme as _th
        self.setProperty("pinshot_css", "toolbar"); self.setStyleSheet(_th.toolbar_css()); self.setAttribute(Qt.WA_TranslucentBackground) if not parent else None
        self.setFocusPolicy(Qt.NoFocus)
        outer = QVBoxLayout(self); outer.setContentsMargins(0, 0, 0, 0); outer.setSpacing(4)
        bar = QFrame(); bar.setObjectName("bar"); row = QHBoxLayout(bar); row.setContentsMargins(4, 2, 4, 2); row.setSpacing(1)
        self.group = QButtonGroup(self); self.group.setExclusive(False); self.tool_buttons = {}
        for t in TOOLS:
            b = _tb(t, checkable=True); b.clicked.connect(lambda _=False, t=t: self._tool_clicked(t))
            self.tool_buttons[t] = b; row.addWidget(b)
        row.addWidget(_sep())
        self.action_buttons = {}
        for a in ["undo", "redo"]:
            b = _tb(a); b.clicked.connect(lambda _=False, a=a: self.action.emit(a)); row.addWidget(b); self.action_buttons[a] = b
        row.addWidget(_sep())
        for a in (["close", "save", "copy", "check"] if for_pin else ["close", "pin", "save", "copy"]):
            b = _tb(a); b.clicked.connect(lambda _=False, a=a: self.action.emit(a)); row.addWidget(b); self.action_buttons[a] = b
        outer.addWidget(bar)
        # Sub-panel: separate floating widget (sibling in the overlay, own tool window for pins) so
        # switching tools never relayouts, resizes or moves the toolbar itself.
        self.panel = QFrame(parent, flags); self.panel.setObjectName("panel"); self.panel.setProperty("pinshot_css", "toolbar"); self.panel.setStyleSheet(_th.toolbar_css())
        self.panel.setFocusPolicy(Qt.NoFocus)
        self.panel_lay = QHBoxLayout(self.panel); self.panel_lay.setContentsMargins(6, 4, 6, 4); self.panel_lay.setSpacing(6)
        self.panel.hide()
        self.bar = bar
        self.adjustSize(); self.setFixedSize(self.sizeHint())
        annotator.toolChanged.connect(self._sync)
        self._sync(annotator.tool)

    def _tool_clicked(self, t):
        self.ann.set_tool(None if self.ann.tool == t else t)

    def _sync(self, tool):
        for t, b in self.tool_buttons.items(): b.setChecked(t == tool)
        self._build_panel(tool); self.place_panel()

    # --- sub-panels
    def _clear(self):
        while self.panel_lay.count():
            it = self.panel_lay.takeAt(0)
            if it.widget(): it.widget().deleteLater()

    def _toggle(self, label, get, set_, tip=None):
        b = QToolButton(); b.setText(label); b.setCheckable(True); b.setChecked(get()); b.setFocusPolicy(Qt.NoFocus)
        b.setToolTip(tip or label); b.setObjectName("opt_" + label.lower().replace(" ", "_"))
        b.toggled.connect(lambda v: (set_(v), self.ann.changed.emit())); self.panel_lay.addWidget(b); return b

    def _choice(self, options, get, set_, name):
        grp = QButtonGroup(self.panel); grp.setExclusive(True)
        for val, label in options:
            b = QToolButton(); b.setText(label); b.setCheckable(True); b.setChecked(get() == val); b.setFocusPolicy(Qt.NoFocus)
            b.setObjectName(f"opt_{name}_{val}")
            b.clicked.connect(lambda _=False, v=val: (set_(v), self.ann.changed.emit())); grp.addButton(b); self.panel_lay.addWidget(b)
        return grp

    def _spin(self, label, lo, hi, get, set_, name):
        self.panel_lay.addWidget(QLabel(label))
        s = QSpinBox(); s.setRange(lo, hi); s.setValue(get()); s.setObjectName(name); s.setFocusPolicy(Qt.ClickFocus)
        s.valueChanged.connect(lambda v: (set_(v), self.ann.changed.emit())); self.panel_lay.addWidget(s); self.spin = s; return s

    def _build_panel(self, tool):
        self._clear(); self.spin = None
        if not tool: self.panel.hide(); return
        if tool == "select":
            self.panel_lay.addWidget(QLabel("Click to select · drag move · handles resize · Del delete · dbl-click text"))
            if self.isVisible(): self.panel.show(); self._fit_panel()
            else: self.panel.hide()
            return
        st = self.ann.styles[tool]; S = lambda k: (lambda v: setattr(st, k, v))
        if tool in ("rect", "ellipse"):
            self._choice([(False, "Outline"), (True, "Filled")], lambda: st.filled, S("filled"), "fill")
            self._spin("Width", 1, 60, lambda: st.width, S("width"), "width")
        elif tool == "line":
            c = QComboBox(); c.addItems(["solid", "dash", "dot"]); c.setCurrentText(st.line_style); c.setObjectName("line_style")
            c.currentTextChanged.connect(lambda v: (setattr(st, "line_style", v), self.ann.changed.emit())); self.panel_lay.addWidget(c)
            self._toggle("Arrow", lambda: st.arrow, S("arrow"), "Arrow at end")
            self._spin("Width", 1, 60, lambda: st.width, S("width"), "width")
        elif tool in ("pen", "marker"):
            self._spin("Width", 1, 60, lambda: st.width, S("width"), "width")
        elif tool == "mosaic":
            self._choice([("brush", "Brush"), ("rect", "Rect")], lambda: st.mode, S("mode"), "mode")
            self._choice([("mosaic", "Mosaic"), ("blur", "Blur")], lambda: st.effect, S("effect"), "effect")
            self.panel_lay.addWidget(QLabel("Size"))
            sl = QSlider(Qt.Horizontal); sl.setRange(1, 30); sl.setValue(st.size); sl.setFixedWidth(110); sl.setObjectName("size")
            v = QLabel(str(st.size)); sl.valueChanged.connect(lambda x: (setattr(st, "size", x), v.setText(str(x)), self.ann.changed.emit()))
            self.panel_lay.addWidget(sl); self.panel_lay.addWidget(v); self.slider = sl
        elif tool == "text":
            self._toggle("B", lambda: st.bold, S("bold"), "Bold").setObjectName("opt_bold")
            self._toggle("I", lambda: st.italic, S("italic"), "Italic").setObjectName("opt_italic")
            self._toggle("Outline", lambda: st.outline, S("outline"), "Outline / halo")
            f = QFontComboBox(); f.setObjectName("font"); f.setFixedWidth(140)
            if st.family: f.setCurrentText(st.family)
            f.currentFontChanged.connect(lambda q: (setattr(st, "family", q.family()), self.ann.changed.emit())); self.panel_lay.addWidget(f)
            self._spin("Size", 8, 120, lambda: st.font_size, S("font_size"), "font_size")
        elif tool == "eraser":
            self._choice([("brush", "Brush"), ("rect", "Rect")], lambda: st.mode, S("mode"), "mode")
            self._spin("Width", 2, 80, lambda: st.width, S("width"), "width")
        elif tool == "number":
            def _set_num_w(v):
                st.width = v; st.font_size = max(8, int(v * 0.65))
            self._spin("Size", 12, 80, lambda: st.width, _set_num_w, "width")
            self._toggle("Filled", lambda: st.filled, S("filled"), "Filled circle")
        if tool not in ("mosaic", "eraser", "select"):
            self.palette = Palette(lambda: st.color, S("color")); self.panel_lay.addWidget(self.palette)
        if self.isVisible(): self.panel.show(); self._fit_panel()

    def _fit_panel(self):
        # Widgets added to a layout are shown via a *queued* call; show them now so the size is right.
        for i in range(self.panel_lay.count()):
            w = self.panel_lay.itemAt(i).widget()
            if w is not None and not w.isVisible(): w.show()
        self.panel_lay.invalidate(); self.panel_lay.activate()
        self.panel.resize(self.panel_lay.sizeHint().expandedTo(self.panel.minimumSizeHint()))

    def place_panel(self):
        """Panel goes under the toolbar, left-aligned with it; above it if there's no room below."""
        if not self.ann.tool or not self.isVisible():
            self.panel.hide(); return
        self.panel.show(); self._fit_panel()
        x, y = self.x(), self.y() + self.height() + 3
        limit = self.parentWidget().height() if self.parentWidget() else \
            (self.screen().availableGeometry().bottom() if self.screen() else 10 ** 6)
        if y + self.panel.height() > limit: y = self.y() - self.panel.height() - 3
        self.panel.move(x, y); self.panel.show(); self.panel.raise_()

    def hideEvent(self, e):
        self.panel.hide(); super().hideEvent(e)

    def moveEvent(self, e):
        if self.panel.isVisible(): self.place_panel()

    def showEvent(self, e):
        super().showEvent(e); self.place_panel()

    def contains_global(self, gp) -> bool:
        return any(w.isVisible() and w.rect().contains(w.mapFromGlobal(gp)) for w in (self, self.panel))

    def close_all(self):
        self.panel.close(); self.close()

    def refresh_values(self):
        """Called after width change via wheel / 1 / 2."""
        t = self.ann.tool
        if not t: return
        st = self.ann.styles[t]
        if self.spin: self.spin.blockSignals(True); self.spin.setValue(st.font_size if t == "text" else st.width); self.spin.blockSignals(False)
        if t == "mosaic" and getattr(self, "slider", None): self.slider.setValue(st.size)

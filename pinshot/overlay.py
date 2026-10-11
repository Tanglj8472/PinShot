"""Snip overlay: window detection, selection with 8 handles, magnifier/color picker, in-snip annotation toolbar."""
import os
from PySide6.QtCore import Qt, QRect, QRectF, QPoint, QPointF, Signal, QTimer, QSize
from PySide6.QtGui import QPainter, QColor, QPen, QPixmap, QCursor, QGuiApplication, QFont, QPainterPath, QImage
from PySide6.QtWidgets import QWidget, QFrame, QLabel, QCheckBox, QPushButton, QHBoxLayout, QVBoxLayout, QFileDialog
from .annotate import Annotator
from .toolbar import ToolDock
from .settings import settings

# Each entry: {"rect": QRect, "image": QImage} — successful snips (copy/pin/save). Newest last.
HISTORY = []


def history_max():
    from .settings import get
    return max(1, int(get("history_max", int)))
BLUE = QColor(30, 144, 255)
HANDLE = 6


def _span(a: QPoint, b: QPoint) -> QRect:
    """Rect whose size equals the drag distance (QRect(p1, p2) is inclusive -> +1 px)."""
    return QRect(min(a.x(), b.x()), min(a.y(), b.y()), abs(b.x() - a.x()), abs(b.y() - a.y()))


HINTS = {
    None: [("W A S D", "Move the cursor by 1 pixel"), ("↑ ↓ ← →", "Move the selection by 1 pixel"),
           ("Ctrl ↑↓←→", "Enlarge selection / Shift: shrink"), ("Tab", "Toggle window / element detection"),
           ("Ctrl A", "Select full screen"), ("Shift R", "Reuse the last snip area"),
           (", .", "Replay snip history (image+area)"), ("`", "Show/hide captured mouse cursor"), ("C / Shift", "Copy color / RGB↔HEX"),
           ("N", "Numbered marker"), ("V", "Select / edit shapes")],
    "_annot": [("1 2 / wheel", "Decrease / increase width"), ("Right-click", "Finish current shape"),
               ("Shift", "Square shape / continue polyline"), ("Ctrl Z / Y", "Undo / redo"),
               ("Enter / Ctrl C", "Copy"), ("Ctrl T", "Pin"), ("Ctrl S", "Save"), ("Esc", "Cancel"),
               ("N", "Numbered marker"), ("V", "Select"), ("Del", "Delete selected")],
    "text": [("Click", "Place a text box"), ("Enter", "Commit text"), ("Esc", "Discard text"),
             ("1 2 / wheel", "Font size"), ("Right-click", "Finish")],
    "select": [("Click", "Select shape"), ("Drag", "Move"), ("Handles", "Resize"), ("Del", "Delete"),
               ("Double-click text", "Re-edit"), ("Ctrl Z", "Undo edit")],
    "number": [("Click", "Place next number"), ("1 2 / wheel", "Size"), ("Right-click", "Finish")],
}


class ConfirmAbort(QFrame):
    def __init__(self, parent, on_yes, on_no):
        super().__init__(parent); self.setObjectName("confirm")
        from . import theme as _th
        self.setProperty("pinshot_css", "confirm"); self.setStyleSheet(_th.confirm_css())
        v = QVBoxLayout(self); v.addWidget(QLabel("Abort snipping?"))
        self.dont = QCheckBox("Do not show again"); v.addWidget(self.dont)
        h = QHBoxLayout(); y = QPushButton("Yes"); n = QPushButton("No"); h.addStretch(); h.addWidget(y); h.addWidget(n); v.addLayout(h)
        for b in (y, n, self.dont): b.setFocusPolicy(Qt.NoFocus)
        y.clicked.connect(on_yes); n.clicked.connect(on_no); self.adjustSize()


class SnipOverlay(QWidget):
    copied = Signal(QPixmap)
    pinned = Signal(QPixmap, QPoint)
    saved = Signal(str)
    cancelled = Signal()
    color_copied = Signal(str)
    text_requested = Signal(QImage, bool)

    def __init__(self, shot: QPixmap, geometry: QRect):
        super().__init__(None, Qt.Window | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint
                         | Qt.X11BypassWindowManagerHint)
        self.setAttribute(Qt.WA_NoSystemBackground); self.setAttribute(Qt.WA_OpaquePaintEvent)
        self.setFocusPolicy(Qt.StrongFocus)
        self.shot = shot
        self.img = shot.toImage().convertToFormat(QImage.Format_RGB32).copy()
        self.dpr = shot.width() / max(1, geometry.width())
        self.setGeometry(geometry)
        self.setMouseTracking(True)
        self.sel = QRect(); self.state = "idle"; self.press_pt = None; self.drag_off = None; self.handle = None
        self.cur = QPoint(0, 0)
        self.mag_forced = False; self.hex_fmt = False
        self.hist_idx = len(HISTORY)  # points past the end (= "current live capture")
        self.detect_mode = "window"; self.show_cursor = False
        self.toast = ""; self._toast_timer = QTimer(self, singleShot=True, interval=1200)
        self._toast_timer.timeout.connect(lambda: (setattr(self, "toast", ""), self.update()))
        self.ready = False; self._mouse_grabbed = False
        from .platform import current as _plat
        g = geometry
        self.windows = [r.translated(-g.x(), -g.y()) for r, _ in _plat().window_rects(exclude_pids=(os.getpid(),))]
        self.elements = []; self._elements_loaded = False; self._geom = QRect(g)
        self.hover = QRect()
        d = self.dpr
        self.ann = Annotator(self, lambda: self.img, lambda p: QPointF(p) * d, lambda p: QPointF(p) / d, lambda: 1 / d)
        self.ann.changed.connect(self.update)
        self.ann.toolChanged.connect(lambda t: self.update())   # toolbar stays put; only the sub-panel changes
        self.dock = ToolDock(self.ann, parent=self, for_pin=False); self.dock.hide()
        self.dock.action.connect(self._action)
        self.confirm = None
        self.pre_grab = None    # X pointer/keyboard hold taken before capture (see main.App.snip)
        self.translate_on_select = False

    # --- lifecycle
    def start(self):
        self.show(); self.raise_(); self.activateWindow()
        self.cur = self.mapFromGlobal(QCursor.pos()); self._update_hover()
        QTimer.singleShot(0, self._arm)

    def _arm(self):
        if not self.windowHandle() or not self.windowHandle().isExposed():
            return QTimer.singleShot(10, self._arm)
        self._arm_now()

    def _arm_now(self):
        if self.pre_grab: self.pre_grab.release(); self.pre_grab = None
        self.activateWindow(); self.raise_(); self.setFocus(Qt.ActiveWindowFocusReason)
        if not self.ready:
            self.grabMouse(Qt.CrossCursor); self._mouse_grabbed = True   # until the first selection is made
            self.grabKeyboard()
        self.ready = True

    def _ungrab_mouse(self):
        if self._mouse_grabbed: self.releaseMouse(); self._mouse_grabbed = False

    def release_grabs_for_dialog(self):
        """Yield to a modal color dialog.

        The snip overlay is override-redirect (X11BypassWindowManagerHint) and always stacks above
        normal Qt dialogs, so releasing grabs alone is not enough. Hide the overlay (and its child
        toolbar/panel) for the duration of the dialog, then restore afterward.
        """
        self._dialog_pause = True
        self._saved_mouse_grab = self._mouse_grabbed
        self._saved_sel = self.sel  # keep selection/state; hide does not clear it
        self.releaseKeyboard()
        if self._mouse_grabbed: self.releaseMouse(); self._mouse_grabbed = False
        # Hide dock/panel explicitly in case they were reparented as Tool windows
        if getattr(self, "dock", None) is not None:
            self.dock.hide()
            if getattr(self.dock, "panel", None) is not None: self.dock.panel.hide()
        self.hide()

    def restore_grabs_after_dialog(self):
        self._dialog_pause = False
        if self.isVisible() and not getattr(self, "_was_hidden_for_dialog", True):
            # already visible
            pass
        self.show(); self.raise_(); self.activateWindow()
        self.setFocus(Qt.ActiveWindowFocusReason)
        if getattr(self, "dock", None) is not None:
            self._place_dock()
        self.grabKeyboard()
        if getattr(self, "_saved_mouse_grab", False) and self.state in ("idle", "creating"):
            self.grabMouse(Qt.CrossCursor); self._mouse_grabbed = True
        self.update()

    def closeEvent(self, e):
        if self.pre_grab: self.pre_grab.release(); self.pre_grab = None
        self._ungrab_mouse(); self.releaseKeyboard(); super().closeEvent(e)

    # --- helpers
    def _phys(self, r: QRect) -> QRect:
        d = self.dpr
        return QRect(int(r.x() * d), int(r.y() * d), int(r.width() * d), int(r.height() * d))

    def _crop(self) -> QPixmap:
        self.ann.finish()
        return QPixmap.fromImage(self.ann.composed().copy(self._phys(self.sel.normalized())))

    def color_at(self, pt: QPoint) -> QColor:
        x = min(max(0, int(pt.x() * self.dpr)), self.img.width() - 1)
        y = min(max(0, int(pt.y() * self.dpr)), self.img.height() - 1)
        return self.img.pixelColor(x, y)

    def color_text(self, c: QColor) -> str:
        return c.name().upper() if self.hex_fmt else f"{c.red()}, {c.green()}, {c.blue()}"

    def show_toast(self, text):
        from .i18n import tr
        self.toast = tr(text); self._toast_timer.start(); self.update()

    def has_sel(self):
        r = self.sel.normalized(); return r.width() > 2 and r.height() > 2

    _valid = has_sel

    def load_elements(self):
        if self._elements_loaded: return
        from .platform import current as _plat
        g = self._geom
        self.elements = [r.translated(-g.x(), -g.y()) for r, _ in _plat().element_rects(exclude_pids=(os.getpid(),))]
        self._elements_loaded = True

    def _update_hover(self):
        if self.state != "idle" or self.has_sel(): return
        from .winlist import hover_rect
        self.hover = hover_rect(self.detect_mode, self.cur, [(r, "") for r in self.windows],
                                [(r, "") for r in self.elements], self.rect())

    def mag_visible(self):
        if self.mag_forced: return True
        if self.ann.tool or self.confirm: return False
        if self.dock.isVisible() and self.dock.geometry().contains(self.cur): return False
        return self.state in ("creating", "resizing") or (self.state == "idle" and not self.has_sel())

    def _handles(self, r: QRect):
        xs = [r.left(), r.center().x(), r.right() + 1]; ys = [r.top(), r.center().y(), r.bottom() + 1]
        names = {(0, 0): "tl", (1, 0): "t", (2, 0): "tr", (0, 1): "l", (2, 1): "r", (0, 2): "bl", (1, 2): "b", (2, 2): "br"}
        return {n: QPoint(xs[i], ys[j]) for (i, j), n in names.items()}

    def _hit_handle(self, pt):
        if not self.has_sel(): return None
        for n, c in self._handles(self.sel.normalized()).items():
            if abs(pt.x() - c.x()) <= HANDLE + 2 and abs(pt.y() - c.y()) <= HANDLE + 2: return n
        return None

    def _place_dock(self):
        if not self.has_sel() or self.state not in ("selected",):
            self.dock.hide(); return
        self.dock.adjustSize()
        r = self.sel.normalized(); w, h = self.dock.width(), self.dock.height()
        x = r.right() + 1 - w; y = r.bottom() + 8
        if y + h > self.height(): y = r.top() - h - 8
        if y < 0: y = r.bottom() - h - 8
        x = max(0, min(x, self.width() - w))
        self.dock.move(x, y); self.dock.show(); self.dock.raise_()

    # --- painting
    def paintEvent(self, _):
        p = QPainter(self)
        p.drawPixmap(self.rect(), self.shot)
        r = self.sel.normalized() if self.has_sel() else (self.hover if self.state == "idle" else _span(self.press_pt or QPoint(), self.cur))
        # dim everything except r
        dim = QPainterPath(); dim.addRect(QRectF(self.rect()))
        hole = QPainterPath(); hole.addRect(QRectF(r)); p.fillPath(dim.subtracted(hole), QColor(0, 0, 0, 110))
        if self.ann.shapes or self.ann.live:
            p.save(); p.scale(1 / self.dpr, 1 / self.dpr); self.ann.paint(p); p.restore()
        if not r.isEmpty():
            p.setPen(QPen(BLUE, 1 if self.state == "idle" else 1)); p.setBrush(Qt.NoBrush); p.drawRect(r.adjusted(0, 0, -1, -1))
            if self.has_sel() and not self.ann.tool:
                p.setBrush(BLUE); p.setPen(QPen(Qt.white, 1))
                for c in self._handles(r).values(): p.drawRect(QRect(c.x() - HANDLE // 2, c.y() - HANDLE // 2, HANDLE, HANDLE))
            pr = self._phys(r)
            label = f"{pr.width()} × {pr.height()} px"
            f = QFont(); f.setPixelSize(12); p.setFont(f); fm = p.fontMetrics()
            lw = fm.horizontalAdvance(label) + 12; ly = r.top() - 22 if r.top() >= 22 else r.top() + 2
            lx = r.left() if r.left() + lw < self.width() else self.width() - lw
            p.setPen(Qt.NoPen); p.setBrush(QColor(0, 0, 0, 170)); p.drawRect(lx, ly, lw, 20)
            p.setPen(Qt.white); p.drawText(QRect(lx, ly, lw, 20), Qt.AlignCenter, label)
        if self.mag_visible(): self._paint_mag(p)
        self._paint_hints(p)
        if self.toast:
            f = p.font(); f.setPixelSize(16); f.setBold(True); p.setFont(f)
            w = p.fontMetrics().horizontalAdvance(self.toast) + 28
            box = QRect((self.width() - w) // 2, self.height() // 2 - 22, w, 44)
            p.setPen(Qt.NoPen); p.setBrush(QColor(20, 20, 20, 220)); p.drawRoundedRect(box, 8, 8)
            p.setPen(Qt.white); p.drawText(box, Qt.AlignCenter, self.toast)

    def _paint_hints(self, p):
        t = self.ann.tool
        key = t if t in ("text", "select", "number") else ("_annot" if t or self.has_sel() else None)
        from .i18n import tr
        rows = [(k, tr(t)) for k, t in HINTS[key]]
        f = QFont(); f.setPixelSize(11); p.setFont(f); fm = p.fontMetrics()
        kw = max(fm.horizontalAdvance(k) for k, _ in rows) + 12
        tw = max(fm.horizontalAdvance(t) for _, t in rows)
        lh = 18; w = kw + tw + 24; h = lh * len(rows) + 12
        box = QRect(8, self.height() - h - 8, w, h)
        if box.contains(self.cur) or (self.has_sel() and box.intersects(self.sel.normalized())):
            box.moveLeft(self.width() - w - 8)
        p.setPen(Qt.NoPen); p.setBrush(QColor(20, 20, 20, 190)); p.drawRoundedRect(box, 4, 4)
        for i, (k, t) in enumerate(rows):
            y = box.top() + 6 + i * lh
            kr = QRect(box.left() + 8, y + 1, kw - 4, lh - 3)
            p.setPen(QPen(QColor(200, 200, 200), 1)); p.setBrush(QColor(60, 60, 60)); p.drawRoundedRect(kr, 3, 3)
            p.setPen(Qt.white); p.drawText(kr, Qt.AlignCenter, k)
            p.drawText(QRect(box.left() + kw + 12, y, tw + 8, lh), Qt.AlignVCenter | Qt.AlignLeft, t)

    def _paint_mag(self, p: QPainter):
        from .i18n import tr
        n, z = 15, 8
        cx, cy = int(self.cur.x() * self.dpr), int(self.cur.y() * self.dpr)
        src = QRect(cx - n // 2, cy - n // 2, n, n)
        size = n * z; info_h = 66
        x, y = self.cur.x() + 22, self.cur.y() + 22
        if x + size > self.width(): x = self.cur.x() - 22 - size
        if y + size + info_h > self.height(): y = self.cur.y() - 22 - size - info_h
        tgt = QRect(x, y, size, size)
        p.fillRect(tgt, Qt.black)
        p.drawPixmap(tgt, self.shot, src)
        p.setPen(QPen(QColor(255, 255, 255, 40), 1))
        for i in range(1, n): p.drawLine(x + i * z, y, x + i * z, y + size); p.drawLine(x, y + i * z, x + size, y + i * z)
        p.setBrush(Qt.NoBrush); p.setPen(QPen(QColor(30, 144, 255, 220), 1))
        c0 = (n // 2) * z
        p.drawRect(x + c0, y + c0, z, z)
        p.fillRect(x, y + c0 + z // 2, c0, 1, QColor(30, 144, 255, 160)); p.fillRect(x + c0 + z, y + c0 + z // 2, c0, 1, QColor(30, 144, 255, 160))
        p.fillRect(x + c0 + z // 2, y, 1, c0, QColor(30, 144, 255, 160)); p.fillRect(x + c0 + z // 2, y + c0 + z, 1, c0, QColor(30, 144, 255, 160))
        p.setPen(QPen(Qt.white, 1)); p.drawRect(tgt)
        c = self.color_at(self.cur)
        p.fillRect(x, y + size, size, info_h, QColor(0, 0, 0, 210))
        f = QFont(); f.setPixelSize(11); p.setFont(f); p.setPen(Qt.white)
        p.drawText(QRect(x, y + size + 2, size, 14), Qt.AlignCenter, f"({cx}, {cy})")
        p.fillRect(x + 8, y + size + 19, 10, 10, c); p.setPen(QPen(Qt.white, 1)); p.drawRect(x + 8, y + size + 19, 10, 10)
        p.drawText(QRect(x + 22, y + size + 16, size - 24, 16), Qt.AlignVCenter, self.color_text(c))
        p.setPen(QColor(210, 210, 210))
        p.drawText(QRect(x, y + size + 34, size, 14), Qt.AlignCenter, tr("Press C to copy color"))
        p.drawText(QRect(x, y + size + 48, size, 14), Qt.AlignCenter, tr("Shift: switch RGB/HEX"))

    # --- mouse
    def mousePressEvent(self, e):
        e.accept()
        if not self.ready: self._arm_now()
        if self.confirm: return
        pt = e.position().toPoint(); self.cur = pt
        if self.dock.isVisible() and self.dock.contains_global(e.globalPosition().toPoint()):
            return   # never treat toolbar/sub-panel presses as selection presses
        if self.has_sel() and self.ann.press(QPointF(pt), e.button(), e.modifiers()):
            if self.ann._sel_mode: self.setCursor(Qt.ClosedHandCursor)
            return
        if e.button() == Qt.RightButton:
            if self.has_sel():   # right-click: reset selection, again exits
                self.sel = QRect(); self.state = "idle"; self.ann.set_tool(None); self.ann.clear(); self._place_dock()
                self._update_hover(); self.update(); return
            return self._cancel()
        if e.button() != Qt.LeftButton: return
        h = self._hit_handle(pt)
        if h and not self.ann.tool:
            self.state = "resizing"; self.handle = h; self.sel = self.sel.normalized(); self._place_dock(); return
        if self.has_sel() and self.sel.normalized().contains(pt):
            if self.ann.tool: return
            self.state = "moving"; self.drag_off = pt - self.sel.normalized().topLeft(); self.sel = self.sel.normalized()
            self._place_dock(); return
        if self.has_sel(): return   # click outside an existing selection: keep it (Snipaste-like)
        self.state = "creating"; self.press_pt = pt; self.update()

    def mouseMoveEvent(self, e):
        self.cur = e.position().toPoint()
        # Freehand: always feed motion (some X11 grabs deliver moves with buttons()==0).
        if self.ann.live and self.ann.is_brush():
            self.ann.move(QPointF(self.cur), e.modifiers()); self.update(); return
        dragging = bool(e.buttons() & (Qt.LeftButton | Qt.RightButton))
        if not dragging and (self.ann.live or self.ann._sel_mode or self.state in ("creating", "moving", "resizing")):
            return   # stray motion must not move rect/line drags
        if self.ann.live or self.ann._sel_mode: self.ann.move(QPointF(self.cur), e.modifiers())
        elif self.state == "creating":
            if (self.cur - self.press_pt).manhattanLength() > 3: self.sel = _span(self.press_pt, self.cur)
        elif self.state == "moving":
            r = QRect(self.sel); r.moveTopLeft(self.cur - self.drag_off)
            r.moveLeft(max(0, min(r.left(), self.width() - r.width()))); r.moveTop(max(0, min(r.top(), self.height() - r.height())))
            self.sel = r
        elif self.state == "resizing":
            r = QRect(self.sel); h = self.handle; c = self.cur
            if "l" in h: r.setLeft(c.x())
            if "r" in h: r.setRight(c.x() - 1)
            if h.startswith("t") or h == "t": r.setTop(c.y())
            if h.startswith("b") or h == "b": r.setBottom(c.y() - 1)
            self.sel = r
        else:
            hover_shape = self.ann.hover(QPointF(self.cur))
            self._update_hover()
            h = self._hit_handle(self.cur)
            cur = {"tl": Qt.SizeFDiagCursor, "br": Qt.SizeFDiagCursor, "tr": Qt.SizeBDiagCursor, "bl": Qt.SizeBDiagCursor,
                   "l": Qt.SizeHorCursor, "r": Qt.SizeHorCursor, "t": Qt.SizeVerCursor, "b": Qt.SizeVerCursor}.get(h)
            if hover_shape: self.setCursor(Qt.OpenHandCursor)
            elif cur and not self.ann.tool: self.setCursor(cur)
            elif self.has_sel() and self.sel.normalized().contains(self.cur) and not self.ann.tool: self.setCursor(Qt.SizeAllCursor)
            else: self.setCursor(Qt.CrossCursor)
        self.update()

    def mouseReleaseEvent(self, e):
        if self.ann.live or self.ann._sel_mode: self.ann.release(QPointF(e.position())); return
        translate_selection = self.translate_on_select and self.state == 'creating' and e.button() == Qt.LeftButton
        if self.state == "creating":
            self.cur = e.position().toPoint()
            self.sel = _span(self.press_pt or self.cur, self.cur)
            if not self.has_sel():
                self.sel = QRect(self.hover if not self.hover.isEmpty() else self.rect())   # click: hovered window
            self.sel = self.sel.normalized()
        if self.state in ("creating", "moving", "resizing"):
            self.sel = self.sel.normalized().intersected(self.rect())
            self.state = "selected"; self._ungrab_mouse(); self._place_dock()
        self.update()

        if translate_selection and self.has_sel(): self._action('translate')

    def mouseDoubleClickEvent(self, e):
        e.accept()
        if self.confirm: return
        if self.ann.tool == "select" and self.has_sel():
            hit = self.ann.hit_test(self.ann.to_img(e.position()))
            if hit and hit.kind == "text":
                self.ann.begin_edit_text(hit); return
        if self.ann.tool or self.confirm: return
        if self.has_sel() and self.sel.normalized().contains(e.position().toPoint()):
            self.state = "selected"; self._copy()

    def wheelEvent(self, e):
        if self.ann.tool or self.ann.selected:
            self.ann.wheel(e.angleDelta().y()); self.dock.refresh_values(); e.accept()

    # --- keys
    def keyPressEvent(self, e):
        if getattr(self, "_dialog_pause", False):
            e.ignore(); return   # dialog owns Esc / keys
        if self.ann.forwarding: e.accept(); return
        if self.ann.key(e): return
        k, m = e.key(), e.modifiers()
        ctrl, shift = bool(m & Qt.ControlModifier), bool(m & Qt.ShiftModifier)
        if self.confirm:
            if k in (Qt.Key_Return, Qt.Key_Enter, Qt.Key_Y): self._confirm_yes()
            elif k in (Qt.Key_Escape, Qt.Key_N): self._confirm_no()
            return
        arrows = {Qt.Key_Left: (-1, 0), Qt.Key_Right: (1, 0), Qt.Key_Up: (0, -1), Qt.Key_Down: (0, 1)}
        wasd = {Qt.Key_A: (-1, 0), Qt.Key_D: (1, 0), Qt.Key_W: (0, -1), Qt.Key_S: (0, 1)}
        if k == Qt.Key_Escape: self.request_abort()
        elif k in (Qt.Key_Return, Qt.Key_Enter) or (k == Qt.Key_C and ctrl): self._copy()
        elif k == Qt.Key_F3 or (k == Qt.Key_T and ctrl): self._pin()
        elif k == Qt.Key_S and ctrl: self._save()
        elif k == Qt.Key_Z and ctrl: self.ann.undo()
        elif k == Qt.Key_Y and ctrl: self.ann.redo()
        elif k == Qt.Key_A and ctrl: self.select_full()
        elif k == Qt.Key_R and shift: self.reuse_last()
        elif k == Qt.Key_C and not ctrl: self.copy_color(); QTimer.singleShot(250, self._cancel)
        elif k == Qt.Key_Shift: self.hex_fmt = not self.hex_fmt; self.update()
        elif k == Qt.Key_Alt: self.mag_forced = True; self.update()
        elif k == Qt.Key_Tab:
            self.detect_mode = "element" if self.detect_mode == "window" else "window"
            tip = self.detect_mode
            if self.detect_mode == "element":
                self.load_elements()
                from .platform import current as _plat
                tip += f" ({_plat().element_source()})"
            self.show_toast(f"Detection: {tip}"); self._update_hover(); self.update()
        elif k in (Qt.Key_QuoteLeft, Qt.Key_AsciiTilde): self.show_cursor = not self.show_cursor; self.show_toast("Captured cursor: " + ("shown" if self.show_cursor else "hidden") + " (stub)")
        elif self.ann.tool == "select" and k in (Qt.Key_Delete, Qt.Key_Backspace): self.ann.delete_selected()
        elif k == Qt.Key_V and self.has_sel(): self.ann.set_tool("select")
        elif k == Qt.Key_N and self.has_sel() and not ctrl: self.ann.set_tool("number")
        elif self.ann.tool and k in (Qt.Key_1, Qt.Key_2): self.ann.adjust_width(-1 if k == Qt.Key_1 else 1); self.dock.refresh_values()
        elif k in wasd and not ctrl: self.move_cursor(*wasd[k])
        elif k in arrows: self.nudge(*arrows[k], mode='grow' if ctrl else 'shrink' if shift else 'move')
        elif k == Qt.Key_Comma: self.history(-1)
        elif k == Qt.Key_Period: self.history(1)

    def keyReleaseEvent(self, e):
        if e.key() == Qt.Key_Alt: self.mag_forced = False; self.update()

    def _action(self, a):
        if a in ('ocr', 'translate'):
            if self.has_sel():
                # Recognize the frozen source, without annotation paint or selection handles.
                image = self.img.copy(self._phys(self.sel.normalized()))
                self.close()
                self.text_requested.emit(image, a == 'translate')
            return
        {"undo": self.ann.undo, "redo": self.ann.redo, "close": self._cancel, "pin": self._pin,
         "save": self._save, "copy": self._copy}[a]()

    def _set_sel(self, r):
        self.sel = QRect(r).intersected(self.rect()); self.state = "selected"; self._ungrab_mouse(); self._place_dock(); self.update()

    def select_full(self): self._set_sel(self.rect())

    def reuse_last(self):
        if HISTORY: self._restore_entry(HISTORY[-1])

    def move_cursor(self, dx, dy):
        self.cur = self.cur + QPoint(dx, dy)
        QCursor.setPos(self.mapToGlobal(self.cur))
        if self.state == "creating": self.sel = _span(self.press_pt, self.cur)
        self._update_hover(); self.update()

    def nudge(self, dx, dy, mode='move'):
        r = self.sel.normalized()
        if r.isEmpty(): return self.move_cursor(dx, dy)
        if mode == 'move': r.translate(dx, dy)
        elif mode == 'grow':
            if dx < 0: r.setLeft(r.left() - 1)
            if dx > 0: r.setRight(r.right() + 1)
            if dy < 0: r.setTop(r.top() - 1)
            if dy > 0: r.setBottom(r.bottom() + 1)
        else:
            if dx < 0 and r.width() > 1: r.setRight(r.right() - 1)
            if dx > 0 and r.width() > 1: r.setLeft(r.left() + 1)
            if dy < 0 and r.height() > 1: r.setBottom(r.bottom() - 1)
            if dy > 0 and r.height() > 1: r.setTop(r.top() + 1)
        self.sel = r.intersected(self.rect())
        if self.state == "idle": self.state = "selected"
        self._place_dock(); self.update()

    def history(self, step):
        """`,` / `.` step through past snips (image + selection), then back to the live capture."""
        if not HISTORY: return
        # hist_idx in [0 .. len(HISTORY)] where len means "live shot"
        self.hist_idx = min(max(0, self.hist_idx + step), len(HISTORY))
        if self.hist_idx == len(HISTORY):
            if hasattr(self, "_live_shot"):
                self.shot = self._live_shot; self.img = self._live_img; self.ann.clear()
                self.ann.cache = type(self.ann.cache)()
            self.update(); return
        self._restore_entry(HISTORY[self.hist_idx])

    def _restore_entry(self, entry):
        if isinstance(entry, QRect):   # legacy rect-only
            self._set_sel(entry); return
        if not hasattr(self, "_live_shot"):
            self._live_shot = QPixmap(self.shot); self._live_img = self.img.copy()
        img = entry["image"]
        self.img = img.copy()
        self.shot = QPixmap.fromImage(self.img)
        self.dpr = self.shot.width() / max(1, self.width())
        self.ann.clear(); self.ann.cache = type(self.ann.cache)()
        self._set_sel(entry["rect"])

    def copy_color(self) -> str:
        txt = self.color_text(self.color_at(self.cur))
        QGuiApplication.clipboard().setText(txt)
        self.show_toast(f"Copied {txt}"); self.repaint()
        self.color_copied.emit(txt)
        return txt

    # --- abort confirmation
    def request_abort(self):
        from .settings import get as sget
        if not self.has_sel() or sget("skip_abort_confirm", bool):
            return self._cancel()
        self.ann.finish()
        self.confirm = ConfirmAbort(self, self._confirm_yes, self._confirm_no)
        self.confirm.move((self.width() - self.confirm.width()) // 2, (self.height() - self.confirm.height()) // 2)
        self.confirm.show(); self.update()

    def _confirm_yes(self):
        if self.confirm and self.confirm.dont.isChecked():
            from .settings import set_ as sset; sset("skip_abort_confirm", True)
        self._cancel()

    def _confirm_no(self):
        if self.confirm: self.confirm.deleteLater(); self.confirm = None; self.update()

    # --- outputs
    def _remember(self):
        r = self.sel.normalized()
        # Store the *current* composed crop's source: full-screen image + rect (annotations already in crop path).
        # History restores the pre-annotation screenshot area for re-editing.
        entry = {"rect": QRect(r), "image": self.img.copy()}
        if HISTORY and isinstance(HISTORY[-1], dict) and HISTORY[-1]["rect"] == r:
            HISTORY[-1] = entry
        else:
            HISTORY.append(entry)
        del HISTORY[:-history_max()]
        self.hist_idx = len(HISTORY)

    def _copy(self):
        if self.has_sel(): self._remember(); self.copied.emit(self._crop()); self.close()

    def _pin(self):
        if self.has_sel():
            self._remember(); pm = self._crop()
            self.pinned.emit(pm, self.mapToGlobal(self.sel.normalized().topLeft())); self.close()

    def _save(self, path=None):
        if not self.has_sel(): return
        from .settings import get
        import os
        pm = self._crop()
        if path is None:
            self.releaseKeyboard(); self.hide()
            folder = get("quick_save_folder") or ""
            path, _ = QFileDialog.getSaveFileName(None, "Save snip", os.path.join(folder, "pinshot.png"),
                                                  "PNG (*.png);;JPEG (*.jpg)")
        if path:
            q = get("image_quality", int)
            (pm.save(path, None, q) if path.lower().endswith((".jpg", ".jpeg")) else pm.save(path))
            self._remember(); self.saved.emit(path); self.close()
        else:
            self.show(); self.grabKeyboard()

    def _cancel(self):
        self.cancelled.emit(); self.close()

"""Frameless always-on-top pin: shadow + glow, zoom/opacity, thumbnail, annotation toolbar, context menu."""
import os
from copy import copy
from PySide6.QtCore import Qt, QPoint, QPointF, QRect, QRectF, QTimer, QUrl
from PySide6.QtGui import (QPainter, QPen, QColor, QPixmap, QTransform, QImage, QCursor, QDesktopServices,
                           QGuiApplication, QAction, QActionGroup)
from PySide6.QtWidgets import QWidget, QMenu, QFileDialog, QToolTip, QApplication
from .clipboard import copy_pixmap
from .annotate import Annotator, Shape, Style
from .toolbar import ToolDock
from .settings import settings

ALL_PINS = []
TOOL_KEYS = {Qt.Key_V: 'select', Qt.Key_R: 'rect', Qt.Key_E: 'ellipse', Qt.Key_A: 'line', Qt.Key_P: 'pen',
             Qt.Key_M: 'mosaic', Qt.Key_T: 'text', Qt.Key_N: 'number', Qt.Key_K: 'marker', Qt.Key_X: 'eraser'}
THUMB = 72


def _composited():
    try:
        from PySide6.QtGui import QGuiApplication
        if QGuiApplication.platformName() != "xcb": return True
        from Xlib import display
        d = display.Display(); ok = d.get_selection_owner(d.intern_atom("_NET_WM_CM_S0")) != 0; d.close(); return ok
    except Exception:
        return False


def _tip_show(pos, text, *a):
    from .i18n import tr
    QToolTip.showText(pos, tr(text), *a)


def _x11():
    from PySide6.QtGui import QGuiApplication
    return QGuiApplication.platformName() == "xcb"


def pin_flags():
    """On X11, Qt.Tool windows get WM_TRANSIENT_FOR=<client leader>; xfwm4 (and others) then keep
    them above *every* other window of the app, so the OCR window could never cover a pin.
    Use a plain always-on-top window there and hide it from the taskbar via EWMH instead."""
    base = Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint
    return base | (Qt.Window if _x11() else Qt.Tool)


class PinWindow(QWidget):
    def __init__(self, pm: QPixmap, pos: QPoint = None):
        super().__init__(None, pin_flags())
        self.setAttribute(Qt.WA_DeleteOnClose)
        self.can_shadow = _composited()
        if self.can_shadow: self.setAttribute(Qt.WA_TranslucentBackground)
        self.shadow = self.can_shadow
        self.img = pm.toImage().convertToFormat(QImage.Format_ARGB32).copy()
        self.scale = 1.0; self.thumb = False; self.group = "Default"; self.saved_path = None
        self.clickthrough = False; self._opacity_before_ct = 1.0
        self.glow = 1.0
        self.ann = Annotator(self, lambda: self.img, self._to_img, self._to_widget, lambda: self.scale)
        self.ann.changed.connect(self.update)
        self.dock = None
        self.drag = None
        self.setMouseTracking(True)
        self._resize()
        if pos is not None: self.move(pos - QPoint(self.M, self.M))
        ALL_PINS.append(self)
        self._glow_timer = QTimer(self, interval=40); self._glow_timer.timeout.connect(self._fade); self._glow_timer.start()

    def showEvent(self, e):
        super().showEvent(e)
        if _x11():
            from .x11raise import set_skip_taskbar
            for ms in (0, 150):   # WM only honours _NET_WM_STATE requests once the window is mapped
                QTimer.singleShot(ms, lambda: self.isVisible() and set_skip_taskbar(self))

    # --- geometry
    @property
    def M(self): return 12 if self.shadow else 0

    def img_rect(self) -> QRect:
        if self.thumb: return QRect(self.M, self.M, THUMB, THUMB)
        return QRect(self.M, self.M, max(8, round(self.img.width() * self.scale)), max(8, round(self.img.height() * self.scale)))

    def _resize(self):
        r = self.img_rect(); self.setFixedSize(r.width() + 2 * self.M, r.height() + 2 * self.M)
        self._place_dock(); self.update()

    def _to_img(self, p): return (QPointF(p) - QPointF(self.M, self.M)) / self.scale
    def _to_widget(self, p): return QPointF(p) * self.scale + QPointF(self.M, self.M)

    def set_scale(self, s, tip=True):
        """Zoom anchored at the image's top-left corner."""
        tl = self.mapToGlobal(QPoint(self.M, self.M))
        self.scale = min(8.0, max(0.05, s)); self._resize(); self.move(tl - QPoint(self.M, self.M))
        if tip: _tip_show(QCursor.pos(), f"Zoom: {round(self.scale * 100)}%", self)

    def set_shadow(self, on):
        tl = self.mapToGlobal(QPoint(self.M, self.M))
        self.shadow = bool(on) and self.can_shadow; self._resize(); self.move(tl - QPoint(self.M, self.M))

    def _fade(self):
        self.glow = max(0.0, self.glow - 0.05); self.update()
        if self.glow <= 0: self._glow_timer.stop()

    def rendered(self) -> QPixmap:
        self.ann.finish(); return QPixmap.fromImage(self.ann.composed())

    def add_shape(self, kind, pts, text="", **style):
        st = copy(self.ann.styles.get(kind, Style()))
        for k, v in style.items(): setattr(st, k, v)
        self.ann.add(Shape(kind, [QPointF(p) for p in pts], st, text))

    # --- paint
    def paintEvent(self, _):
        p = QPainter(self); p.setRenderHint(QPainter.Antialiasing)
        r = self.img_rect()
        if self.shadow:
            for i in range(self.M, 0, -1):
                a = int(46 * (1 - i / self.M) ** 2) + 2
                p.setPen(Qt.NoPen); p.setBrush(QColor(0, 0, 0, a)); p.drawRoundedRect(QRectF(r).adjusted(-i, -i + 2, i, i + 2), i, i)
        if self.thumb:
            s = min(self.img.width(), self.img.height())
            src = QRect((self.img.width() - s) // 2, (self.img.height() - s) // 2, s, s)
            p.drawImage(r, self.ann.composed(), src)
        else:
            p.save(); p.setClipRect(r); p.drawImage(r, self.img)
            p.translate(self.M, self.M); p.scale(self.scale, self.scale); self.ann.paint(p); p.restore()
        p.setBrush(Qt.NoBrush)
        if self.glow > 0:
            g = QColor(40, 220, 90, int(255 * self.glow)); p.setPen(QPen(g, 3)); p.drawRect(QRectF(r).adjusted(-1, -1, 0, 0))
        if self.clickthrough:
            col = QColor(180, 40, 220, 200)
            p.setPen(QPen(col, 2)); p.drawRect(QRectF(r).adjusted(-0.5, -0.5, -0.5, -0.5))
            p.setPen(Qt.NoPen); p.setBrush(QColor(180, 40, 220, 200))
            bw = 86
            badge = QRect(r.right() - bw - 2, r.top() + 2, bw, 18)
            if badge.left() < r.left() + 2: badge.moveLeft(r.left() + 2)
            p.drawRoundedRect(badge, 3, 3); p.setPen(Qt.white)
            f = p.font(); f.setPixelSize(10); f.setBold(True); p.setFont(f)
            from .i18n import tr
            p.drawText(badge, Qt.AlignCenter, tr("CLICK-THRU"))
        else:
            col = QColor(255, 140, 0) if self.ann.tool else QColor(30, 144, 255, 150)
            p.setPen(QPen(col, 1)); p.drawRect(QRectF(r).adjusted(-0.5, -0.5, -0.5, -0.5))

    # --- annotate mode / toolbar
    def annotating(self): return self.dock is not None and self.dock.isVisible()

    def show_toolbar(self, on=True, tool="rect"):
        if on:
            if self.thumb: self.toggle_thumb()
            if self.dock is None:
                self.dock = ToolDock(self.ann, parent=None, for_pin=True); self.dock.action.connect(self._action)
            self.dock.show(); self._place_dock()
            if tool and not self.ann.tool: self.ann.set_tool(tool)
        else:
            self.ann.set_tool(None)
            if self.dock: self.dock.hide()
        self.setCursor(Qt.CrossCursor if self.ann.tool else Qt.ArrowCursor); self.update()

    def _place_dock(self):
        if not self.dock or not self.dock.isVisible(): return
        self.dock.adjustSize()
        r = self.img_rect(); br = self.mapToGlobal(r.bottomRight())
        scr = (QGuiApplication.screenAt(br) or QGuiApplication.primaryScreen()).availableGeometry()
        x = br.x() + 1 - self.dock.width(); y = br.y() + 6
        if y + self.dock.height() > scr.bottom(): y = self.mapToGlobal(r.topLeft()).y() - self.dock.height() - 6
        self.dock.move(max(scr.left(), x), y); self.dock.raise_()

    def moveEvent(self, e): self._place_dock()

    def _action(self, a):
        {"undo": self.ann.undo, "redo": self.ann.redo, "close": self.close, "save": self.save,
         "copy": lambda: copy_pixmap(self.rendered()), "check": lambda: self.show_toolbar(False)}[a]()

    # --- mouse
    def mousePressEvent(self, e):
        if self.clickthrough: e.ignore(); return
        if self.annotating() and self.ann.press(e.position(), e.button(), e.modifiers()):
            if self.ann._sel_mode: self.setCursor(Qt.ClosedHandCursor)
            return
        if e.button() == Qt.LeftButton:
            self.drag = e.globalPosition().toPoint() - self.frameGeometry().topLeft()

    def mouseMoveEvent(self, e):
        if self.clickthrough: e.ignore(); return
        if self.ann.live and self.ann.is_brush():
            self.ann.move(e.position(), e.modifiers()); return
        if not (e.buttons() & Qt.LeftButton) and (self.ann.live or self.ann._sel_mode or self.drag is not None): return
        if self.ann.live or self.ann._sel_mode: self.ann.move(e.position(), e.modifiers()); return
        if self.drag is not None: self.move(e.globalPosition().toPoint() - self.drag)
        elif self.annotating():
            hover_shape = self.ann.hover(e.position())
            self.setCursor(Qt.OpenHandCursor if hover_shape else Qt.CrossCursor)

    def mouseReleaseEvent(self, e):
        if self.clickthrough: e.ignore(); return
        if self.ann.live or self.ann._sel_mode: self.ann.release(e.position()); return
        self.drag = None

    def mouseDoubleClickEvent(self, e):
        if self.clickthrough: e.ignore(); return
        if self.ann.tool == "select" or (self.ann.tool is None and self.ann.selected):
            hit = self.ann.hit_test(self.ann.to_img(e.position()))
            if hit and hit.kind == "text":
                self.show_toolbar(True, "select"); self.ann.begin_edit_text(hit); return
        if self.ann.tool: return
        if e.modifiers() & Qt.ShiftModifier: return self.toggle_thumb()
        from .settings import get, set_
        action = get("dblclick_pin_action")
        if action == "none": return
        if not get("dblclick_tip_shown", bool):
            set_("dblclick_tip_shown", True)
            tip = "Tip: double-click closes a pin." if action == "close" else "Tip: double-click hides a pin."
            _tip_show(e.globalPosition().toPoint(), tip + "\nDouble-click again to confirm.", self, QRect(), 4000)
            return
        if action == "hide": self.hide()
        else: self.close_recoverable()

    def toggle_thumb(self):
        tl = self.mapToGlobal(QPoint(self.M, self.M))
        self.thumb = not self.thumb; self._resize(); self.move(tl - QPoint(self.M, self.M))

    def wheelEvent(self, e):
        if self.clickthrough: e.ignore(); return
        d = e.angleDelta().y()
        if self.annotating() and (self.ann.tool or self.ann.selected) and not (e.modifiers() & Qt.ControlModifier):
            self.ann.wheel(d); self.dock and self.dock.refresh_values(); return
        if e.modifiers() & Qt.ControlModifier:
            self.setWindowOpacity(min(1.0, max(0.1, round(self.windowOpacity() + (0.1 if d > 0 else -0.1), 2))))
            _tip_show(QCursor.pos(), f"Opacity: {round(self.windowOpacity() * 100)}%", self)
        elif not self.thumb:
            self.set_scale(self.scale * (1.1 if d > 0 else 1 / 1.1))

    def keyPressEvent(self, e):
        if self.ann.forwarding: return
        if e.key() in (Qt.Key_Return, Qt.Key_Enter) and self.ann.editor is None and self.ann.tool == "text": return
        if self.ann.key(e): return
        if QApplication.activePopupWidget() is not None and e.key() == Qt.Key_Escape:
            QApplication.activePopupWidget().close(); return
        k, m = e.key(), e.modifiers(); ctrl = bool(m & Qt.ControlModifier)
        if k == Qt.Key_Escape and self.ann.tool and self.ann.live: self.ann.finish()
        elif k == Qt.Key_Escape or (ctrl and k == Qt.Key_W): self.close_recoverable()
        elif ctrl and k == Qt.Key_C: copy_pixmap(self.rendered())
        elif ctrl and k == Qt.Key_S: self.save()
        elif ctrl and k == Qt.Key_Z: self.ann.undo()
        elif ctrl and k == Qt.Key_Y: self.ann.redo()
        elif k in (Qt.Key_Delete, Qt.Key_Backspace) and self.ann.tool == "select": self.ann.delete_selected()
        elif k == Qt.Key_Space: self.show_toolbar(not self.annotating())
        elif k in TOOL_KEYS and not ctrl and self.annotating(): self.ann.set_tool(TOOL_KEYS[k])
        elif k in TOOL_KEYS and not ctrl: self.show_toolbar(True, TOOL_KEYS[k]); self.ann.set_tool(TOOL_KEYS[k])
        elif k in (Qt.Key_1, Qt.Key_2) and self.ann.tool: self.ann.adjust_width(-1 if k == Qt.Key_1 else 1); self.dock.refresh_values()
        elif k in (Qt.Key_1, Qt.Key_2): self.rotate(-90 if k == Qt.Key_1 else 90)
        elif k == Qt.Key_3: self.flip(True)
        elif k == Qt.Key_4: self.flip(False)

    def _xform(self, t):
        self.img = self.ann.composed().transformed(t); self.ann.clear(); self.ann.cache = type(self.ann.cache)(); self._resize()

    def rotate(self, deg): self._xform(QTransform().rotate(deg))
    def flip(self, horizontal): self._xform(QTransform().scale(-1, 1) if horizontal else QTransform().scale(1, -1))

    def replace_image(self, img: QImage):
        if img is None or img.isNull(): return
        self.img = img.convertToFormat(QImage.Format_ARGB32).copy(); self.ann.clear(); self._resize()

    def paste(self):
        from .clipboard import clipboard_pixmap
        pm = clipboard_pixmap()
        if pm: self.replace_image(pm.toImage())

    def replace_by_file(self):
        path, _ = QFileDialog.getOpenFileName(self, "Replace from file", "", "Images (*.png *.jpg *.jpeg *.bmp *.gif *.webp)")
        if path: self.replace_image(QImage(path))

    def save(self, path=None):
        from .settings import get
        import os
        if path is None:
            folder = get("quick_save_folder") or ""
            path, filt = QFileDialog.getSaveFileName(self, "Save image as", os.path.join(folder, "pinshot.png"),
                                                     "PNG (*.png);;JPEG (*.jpg)")
        if path:
            q = get("image_quality", int)
            ok = self.rendered().save(path, None, q) if path.lower().endswith((".jpg", ".jpeg")) else self.rendered().save(path)
            if ok: self.saved_path = path
        return self.saved_path

    def view_in_folder(self):
        if self.saved_path: QDesktopServices.openUrl(QUrl.fromLocalFile(os.path.dirname(self.saved_path)))

    def build_menu(self) -> QMenu:
        m = QMenu(self)
        m.addAction("Copy image", lambda: copy_pixmap(self.rendered()), "Ctrl+C")
        m.addAction("Save image as…", self.save, "Ctrl+S")
        m.addSeparator()
        a = m.addAction("Show toolbar", lambda: self.show_toolbar(not self.annotating())); a.setCheckable(True); a.setChecked(self.annotating())
        a = m.addAction("Shadow", lambda: self.set_shadow(not self.shadow)); a.setCheckable(True); a.setChecked(self.shadow); a.setEnabled(self.can_shadow)
        a = m.addAction("Click-through", lambda: self.set_clickthrough(not self.clickthrough)); a.setCheckable(True); a.setChecked(self.clickthrough)
        z = m.addMenu("Zoom")
        for pct in (25, 50, 75, 100, 150, 200, 400):
            act = z.addAction(f"{pct}%", lambda pct=pct: self.set_scale(pct / 100, tip=False)); act.setCheckable(True)
            act.setChecked(abs(self.scale * 100 - pct) < 0.5)
        ip = m.addMenu("Image")
        ip.addAction("Rotate left", lambda: self.rotate(-90), "1"); ip.addAction("Rotate right", lambda: self.rotate(90), "2")
        ip.addAction("Flip horizontal", lambda: self.flip(True), "3"); ip.addAction("Flip vertical", lambda: self.flip(False), "4")
        m.addSeparator()
        m.addAction("Paste", self.paste); m.addAction("Replace from file…", self.replace_by_file)
        from . import session as SES
        from .i18n import group_label
        g = m.addMenu("Move to group"); ag = QActionGroup(g)
        for name in SES.group_names():
            act = g.addAction(group_label(name), lambda n=name: self.move_to_group(n)); act.setCheckable(True); act.setChecked(self.group == name); ag.addAction(act)
        g.addSeparator()
        g.addAction("New group…", self._new_group)
        g.addAction("Rename group…", self._rename_group)
        a = m.addAction("View in folder", self.view_in_folder); a.setEnabled(bool(self.saved_path))
        m.addSeparator()
        m.addAction("Close", self.close_recoverable, "Esc")
        m.addAction("Destroy", self.destroy_pin)
        m.addSeparator()
        sz = m.addMenu(f"{self.img.width()} × {self.img.height()}")
        sz.addAction("Copy size", lambda: QGuiApplication.clipboard().setText(f"{self.img.width()}x{self.img.height()}"))
        sz.addAction("Reset zoom", lambda: self.set_scale(1.0, tip=False))
        sz.addAction(f"Zoom {round(self.scale * 100)}%  ·  Opacity {round(self.windowOpacity() * 100)}%").setEnabled(False)
        return m

    def contextMenuEvent(self, e):
        if self.clickthrough: e.ignore(); return
        if self.ann.tool: return
        self.open_menu(e.globalPos(), modal=True)

    def open_menu(self, gpos, modal=True):
        m = self.build_menu(); self.menu = m
        m.aboutToShow.connect(lambda: QTimer.singleShot(0, lambda: (m.activateWindow(), m.setFocus())))
        app = QApplication.instance(); app.installEventFilter(self)
        m.aboutToHide.connect(lambda: app.removeEventFilter(self))
        if modal: m.exec(gpos)
        else: m.popup(gpos)
        return m

    def eventFilter(self, obj, ev):
        from PySide6.QtCore import QEvent
        if ev.type() == QEvent.KeyPress and ev.key() == Qt.Key_Escape:
            pop = QApplication.activePopupWidget()
            if pop is not None:
                pop.close(); return True
        return False

    def move_to_group(self, name):
        from . import session as SES
        self.group = SES.ensure_group(name)
        if not SES.group_visible(self.group): self.hide()

    def _new_group(self):
        from PySide6.QtWidgets import QInputDialog
        name, ok = QInputDialog.getText(self, "New group", "Name:")
        if ok and name.strip(): self.new_group(name.strip())

    def new_group(self, name):
        """Create (or reuse, case-insensitive) a group and move this pin into it; refreshes UI via BUS."""
        from . import session as SES
        self.move_to_group(SES.ensure_group(name)); SES.BUS.groups_changed.emit()

    def _rename_group(self):
        from PySide6.QtWidgets import QInputDialog
        from . import session as SES
        from .i18n import group_label
        shown = group_label(self.group)
        name, ok = QInputDialog.getText(self, "Rename group", "Name:", text=shown)
        if ok and name.strip() and name.strip() != shown and self.group != "Default":
            SES.rename_group(self.group, name.strip())

    def set_clickthrough(self, on: bool):
        """Pass mouse clicks through to windows below.
        Qt.WindowTransparentForInput alone is not enough on X11 (WM frame keeps a full input shape);
        also clear ShapeInput on the client window and its frame via XShape."""
        on = bool(on)
        if on == self.clickthrough: return
        if on:
            self._opacity_before_ct = self.windowOpacity()
            self.setWindowOpacity(min(self._opacity_before_ct, 0.55))
        else:
            self.setWindowOpacity(self._opacity_before_ct)
        self.clickthrough = on
        was = self.isVisible()
        self.setWindowFlag(Qt.WindowTransparentForInput, on)
        if was: self.show(); self.raise_()
        # Apply after the window is mapped so winId()/frame exist
        from PySide6.QtCore import QTimer
        QTimer.singleShot(0, lambda: self._apply_x11_passthrough(on))
        self.update()

    def _apply_x11_passthrough(self, on: bool):
        try:
            from .platform import current as _plat
            _plat().set_clickthrough(self, on)
        except Exception as e:
            print("[pinshot] click-through shape:", e, flush=True)

    def resizeEvent(self, e):
        super().resizeEvent(e)
        if self.clickthrough:
            from PySide6.QtCore import QTimer
            QTimer.singleShot(0, lambda: self._apply_x11_passthrough(True))

    def close_recoverable(self):
        if self.clickthrough: self.set_clickthrough(False)
        self._recoverable = True; self.close()

    def destroy_pin(self):
        if self.clickthrough: self.set_clickthrough(False)
        self._recoverable = False; self.close()

    def closeEvent(self, e):
        self.ann.finish()
        if self.dock: self.dock.close_all(); self.dock.deleteLater(); self.dock = None
        if getattr(self, "_recoverable", True):
            from . import session as SES
            try: SES.push_closed(self)
            except Exception: pass
        if self in ALL_PINS: ALL_PINS.remove(self)
        super().closeEvent(e)

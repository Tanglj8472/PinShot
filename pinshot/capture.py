"""Screen capture: virtual desktop (all monitors) via Qt, mss fallback. X11 input hold."""
from PySide6.QtCore import QRect
from PySide6.QtGui import QGuiApplication, QImage, QPixmap, QPainter


def grab_primary() -> QPixmap:
    """Backward-compatible alias: grab the full virtual desktop."""
    pm, _ = grab_virtual()
    return pm


def grab_virtual():
    """Return (QPixmap covering virtualGeometry, QRect virtualGeometry in global coords)."""
    screens = list(QGuiApplication.screens())
    if not screens:
        return _grab_mss_virtual() or (QPixmap(), QRect())
    vg = screens[0].virtualGeometry()
    out = QPixmap(vg.size())
    out.fill(0)
    p = QPainter(out)
    for screen in screens:
        g = screen.geometry()
        # grab this screen's contents (0,0,w,h relative to the screen)
        pm = screen.grabWindow(0, 0, 0, g.width(), g.height())
        if pm.isNull() or _is_blank(pm):
            continue
        pm = QPixmap(pm); pm.setDevicePixelRatio(1.0)
        p.drawPixmap(g.x() - vg.x(), g.y() - vg.y(), pm)
    p.end()
    if out.isNull() or _is_blank(out):
        alt = _grab_mss_virtual()
        if alt: return alt
    out.setDevicePixelRatio(1.0)
    return out, QRect(vg)


def _is_blank(pm: QPixmap) -> bool:
    img = pm.toImage().scaled(16, 16)
    return len({img.pixel(x, y) for x in range(16) for y in range(16)}) <= 1 and img.pixel(0, 0) in (0xFF000000, 0)


def _grab_mss_virtual():
    try:
        import mss
        with mss.mss() as s:
            mon = s.monitors[0]   # all monitors
            shot = s.grab(mon)
            img = QImage(shot.bgra, shot.width, shot.height, QImage.Format_ARGB32).copy()
            return QPixmap.fromImage(img), QRect(mon["left"], mon["top"], mon["width"], mon["height"])
    except Exception:
        return None


class InputHold:
    """Grab the X pointer+keyboard on the root window until the overlay takes over."""
    def __init__(self):
        self.d = None
        try:
            from Xlib import display, X
            self.d = display.Display(); root = self.d.screen().root
            self.pointer = root.grab_pointer(False, X.ButtonPressMask | X.ButtonReleaseMask | X.PointerMotionMask,
                                             X.GrabModeAsync, X.GrabModeAsync, X.NONE, X.NONE, X.CurrentTime)
            self.keyboard = root.grab_keyboard(False, X.GrabModeAsync, X.GrabModeAsync, X.CurrentTime)
            self.d.flush()
        except Exception:
            self.d = None

    def active(self): return self.d is not None and getattr(self, "pointer", 1) == 0

    def release(self):
        if self.d is None: return
        try:
            from Xlib import X
            self.d.ungrab_pointer(X.CurrentTime); self.d.ungrab_keyboard(X.CurrentTime); self.d.sync(); self.d.close()
        except Exception:
            pass
        self.d = None

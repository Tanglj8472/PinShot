"""Windows backend: pure ctypes (no compiled extensions); optional comtypes for UI Automation.

All windll access happens inside functions, so the module imports on Linux; the unit tests
exercise the pure helpers (hotkey parsing, physical->logical mapping, Run-key command)."""
import ctypes
import ctypes.wintypes as wt
import subprocess
import sys
import time
from .base import BasePlatform

# ---------------------------------------------------------------- constants
GWL_EXSTYLE = -20
WS_EX_TRANSPARENT = 0x00000020
WS_EX_LAYERED = 0x00080000
WS_EX_TOOLWINDOW = 0x00000080
WS_EX_NOACTIVATE = 0x08000000
LWA_ALPHA = 0x2
WM_HOTKEY = 0x0312
MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, MOD_NOREPEAT = 0x1, 0x2, 0x4, 0x8, 0x4000
DWMWA_EXTENDED_FRAME_BOUNDS = 9
DWMWA_CLOAKED = 14
WDA_NONE, WDA_EXCLUDEFROMCAPTURE = 0x0, 0x11
DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
RUN_VALUE = "PinShot"

_MODS = {"ctrl": MOD_CONTROL, "control": MOD_CONTROL, "shift": MOD_SHIFT, "alt": MOD_ALT,
         "win": MOD_WIN, "super": MOD_WIN, "meta": MOD_WIN}
_VK_NAMED = {
    "esc": 0x1B, "escape": 0x1B, "tab": 0x09, "space": 0x20, "enter": 0x0D, "return": 0x0D,
    "backspace": 0x08, "delete": 0x2E, "del": 0x2E, "insert": 0x2D, "ins": 0x2D, "home": 0x24, "end": 0x23,
    "pgup": 0x21, "pageup": 0x21, "page_up": 0x21, "pgdn": 0x22, "pagedown": 0x22, "page_down": 0x22,
    "left": 0x25, "up": 0x26, "right": 0x27, "down": 0x28, "print": 0x2C, "printscreen": 0x2C, "prtsc": 0x2C,
    "pause": 0x13, "`": 0xC0, "-": 0xBD, "=": 0xBB, "[": 0xDB, "]": 0xDD, ";": 0xBA, "'": 0xDE,
    ",": 0xBC, ".": 0xBE, "/": 0xBF, "\\": 0xDC,
}


# ---------------------------------------------------------------- pure helpers (unit-tested on Linux)
def parse_hotkey(seq):
    """'Ctrl+Shift+F3' -> (mods, vk) for RegisterHotKey; None if empty/unknown."""
    seq = (seq or "").strip()
    if not seq: return None
    parts = [p.strip().lower() for p in seq.split("+") if p.strip()]
    if not parts: return None
    mods = 0
    for p in parts[:-1]:
        if p not in _MODS: return None
        mods |= _MODS[p]
    k = parts[-1]
    if len(k) >= 2 and k[0] == "f" and k[1:].isdigit() and 1 <= int(k[1:]) <= 24:
        vk = 0x70 + int(k[1:]) - 1
    elif len(k) == 1 and (k.isalpha() or k.isdigit()):
        vk = ord(k.upper())
    elif k in _VK_NAMED:
        vk = _VK_NAMED[k]
    else:
        return None
    return mods, vk


def phys_to_logical(x, y, w, h, screens):
    """Map a physical-pixel rect to Qt logical coords. screens: [(lx, ly, lw, lh, dpr)] logical geometry.
    Qt (per-monitor DPI v2) keeps each screen's origin and scales sizes by its DPR."""
    cx, cy = x + w / 2, y + h / 2
    best = None
    for (sx, sy, sw, sh, dpr) in screens:
        if sx <= cx < sx + sw * dpr and sy <= cy < sy + sh * dpr:
            best = (sx, sy, dpr); break
    if best is None:
        best = (screens[0][0], screens[0][1], screens[0][4]) if screens else (0, 0, 1.0)
    sx, sy, dpr = best
    return (round(sx + (x - sx) / dpr), round(sy + (y - sy) / dpr), round(w / dpr), round(h / dpr))


def run_key_command(argv):
    """Value stored under HKCU\\...\\Run: quoted exe + args (pythonw for source installs, no console)."""
    argv = list(argv)
    if argv and argv[0].lower().endswith("python.exe"):
        cand = argv[0][:-len("python.exe")] + "pythonw.exe"
        argv[0] = cand
    return subprocess.list2cmdline(argv)


# ---------------------------------------------------------------- Win32 lazy handles
def _u32(): return ctypes.windll.user32
def _dwm(): return ctypes.windll.dwmapi
def _k32(): return ctypes.windll.kernel32


def _hwnd(widget):
    return int(widget.winId())


def _screens():
    from PySide6.QtGui import QGuiApplication
    out = []
    for s in QGuiApplication.screens():
        g = s.geometry(); out.append((g.x(), g.y(), g.width(), g.height(), s.devicePixelRatio()))
    return out


def _qrect(x, y, w, h, screens):
    from PySide6.QtCore import QRect
    lx, ly, lw, lh = phys_to_logical(x, y, w, h, screens)
    return QRect(lx, ly, lw, lh)


def _window_pid(hwnd):
    pid = wt.DWORD()
    _u32().GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return pid.value


def _frame_bounds(hwnd):
    r = wt.RECT()
    hr = _dwm().DwmGetWindowAttribute(wt.HWND(hwnd), DWMWA_EXTENDED_FRAME_BOUNDS, ctypes.byref(r), ctypes.sizeof(r))
    if hr != 0:
        if not _u32().GetWindowRect(wt.HWND(hwnd), ctypes.byref(r)): return None
    return r.left, r.top, r.right - r.left, r.bottom - r.top


def _cloaked(hwnd):
    v = wt.DWORD()
    hr = _dwm().DwmGetWindowAttribute(wt.HWND(hwnd), DWMWA_CLOAKED, ctypes.byref(v), ctypes.sizeof(v))
    return hr == 0 and v.value != 0


def _title(hwnd):
    n = _u32().GetWindowTextLengthW(hwnd)
    buf = ctypes.create_unicode_buffer(n + 1)
    _u32().GetWindowTextW(hwnd, buf, n + 1)
    return buf.value


# WINFUNCTYPE (stdcall) exists only on Windows; CFUNCTYPE keeps the module importable elsewhere.
WNDENUMPROC = getattr(ctypes, "WINFUNCTYPE", ctypes.CFUNCTYPE)(wt.BOOL, wt.HWND, wt.LPARAM)


# ---------------------------------------------------------------- hotkeys
class WinHotkeys:
    """RegisterHotKey(hwnd=NULL) posts WM_HOTKEY to the GUI thread's queue; a Qt native event filter
    picks it up (no extra thread, no window)."""

    def __init__(self, bus, bindings):
        from PySide6.QtCore import QAbstractNativeEventFilter, QCoreApplication, QTimer
        self.bus = bus; self.ids = {}; self.failed = []
        outer = self

        class _Filter(QAbstractNativeEventFilter):
            def nativeEventFilter(self, etype, message):
                try:
                    if etype in (b"windows_generic_MSG", "windows_generic_MSG"):
                        msg = wt.MSG.from_address(int(message))
                        if msg.message == WM_HOTKEY and msg.wParam in outer.ids:
                            cmd = outer.ids[msg.wParam]
                            QTimer.singleShot(0, lambda c=cmd: outer.bus.fired.emit(c))
                            return True, 0
                except Exception:
                    pass
                return False, 0

        self.filter = _Filter()
        QCoreApplication.instance().installNativeEventFilter(self.filter)
        next_id = 0xA000
        for cmd, seq in bindings.items():
            hk = parse_hotkey(seq)
            if not hk: self.failed.append(seq); continue
            mods, vk = hk
            if _u32().RegisterHotKey(None, next_id, mods | MOD_NOREPEAT, vk):
                self.ids[next_id] = cmd
            else:
                self.failed.append(seq)          # taken by another app
            next_id += 1
        self.ok = bool(self.ids)

    def stop(self):
        for i in list(self.ids):
            try: _u32().UnregisterHotKey(None, i)
            except Exception: pass
        self.ids.clear()
        try:
            from PySide6.QtCore import QCoreApplication
            QCoreApplication.instance().removeNativeEventFilter(self.filter)
        except Exception:
            pass


# ---------------------------------------------------------------- backend
class WindowsPlatform(BasePlatform):
    name = "windows"

    def __init__(self):
        self._elem_source = "child windows"
        self._uia = None          # None = not probed, False = unavailable, else IUIAutomation

    # ---- process
    def init_process(self):
        try:
            if not _u32().SetProcessDpiAwarenessContext(ctypes.c_void_p(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)):
                ctypes.windll.shcore.SetProcessDpiAwareness(2)
        except Exception:
            pass   # already set (Qt 6 defaults to per-monitor v2) or pre-Win10

    # ---- capture
    def capture_virtual(self):
        """Qt per-screen grab composed into the logical virtual desktop; mss fallback.
        Mixed-DPI setups are scaled to logical resolution (see README known gaps)."""
        from PySide6.QtCore import QRect
        from PySide6.QtGui import QGuiApplication, QPixmap, QPainter
        screens = QGuiApplication.screens()
        if not screens:
            return self._mss_virtual()
        vg = screens[0].virtualGeometry()
        out = QPixmap(vg.size()); out.fill(0)
        p = QPainter(out); p.setRenderHint(QPainter.SmoothPixmapTransform)
        got = False
        for s in screens:
            g = s.geometry()
            pm = s.grabWindow(0)
            if pm.isNull(): continue
            pm.setDevicePixelRatio(1.0)
            p.drawPixmap(QRect(g.x() - vg.x(), g.y() - vg.y(), g.width(), g.height()), pm); got = True
        p.end()
        if not got:
            return self._mss_virtual()
        return out, QRect(vg)

    def _mss_virtual(self):
        from PySide6.QtCore import QRect
        from PySide6.QtGui import QImage, QPixmap
        import mss
        with mss.mss() as m:
            mon = m.monitors[0]; g = m.grab(mon)
            img = QImage(g.bgra, g.width, g.height, QImage.Format_ARGB32).copy()
        return QPixmap.fromImage(img), QRect(mon["left"], mon["top"], g.width, g.height)

    def exclude_from_capture(self, widget, on=True):
        try:
            return bool(_u32().SetWindowDisplayAffinity(wt.HWND(_hwnd(widget)), WDA_EXCLUDEFROMCAPTURE if on else WDA_NONE))
        except Exception:
            return False

    # ---- hotkeys
    def install_hotkeys(self, bus, bindings):
        try:
            h = WinHotkeys(bus, bindings)
            return ("win32-registerhotkey" if h.ok else None), h
        except Exception as e:
            return None, e

    # ---- click-through
    def set_clickthrough(self, widget, on):
        try:
            u = _u32(); hwnd = wt.HWND(_hwnd(widget))
            get = u.GetWindowLongPtrW; set_ = u.SetWindowLongPtrW
            get.restype = ctypes.c_ssize_t; set_.restype = ctypes.c_ssize_t
            set_.argtypes = [wt.HWND, ctypes.c_int, ctypes.c_ssize_t]
            ex = get(hwnd, GWL_EXSTYLE)
            if on:
                was_layered = bool(ex & WS_EX_LAYERED)
                set_(hwnd, GWL_EXSTYLE, ex | WS_EX_TRANSPARENT | WS_EX_LAYERED)
                if not was_layered:      # a freshly layered window is invisible until it gets an alpha
                    alpha = max(1, min(255, int(round(widget.windowOpacity() * 255))))
                    u.SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA)
            else:
                set_(hwnd, GWL_EXSTYLE, ex & ~WS_EX_TRANSPARENT)   # Qt keeps managing WS_EX_LAYERED (opacity)
            return True
        except Exception:
            return False

    # ---- detection
    def window_rects(self, exclude_pids=()):
        u = _u32(); screens = _screens(); out = []

        def cb(hwnd, _):
            try:
                if not u.IsWindowVisible(hwnd) or u.IsIconic(hwnd) or _cloaked(hwnd): return True
                if _window_pid(hwnd) in exclude_pids: return True
                ex = u.GetWindowLongW(hwnd, GWL_EXSTYLE)
                title = _title(hwnd)
                if (ex & WS_EX_TOOLWINDOW) and not title: return True
                b = _frame_bounds(hwnd)
                if not b or b[2] < 8 or b[3] < 8: return True
                out.append((_qrect(*b, screens), title))
            except Exception:
                pass
            return True

        u.EnumWindows(WNDENUMPROC(cb), 0)      # Z-order, topmost first
        return out

    def element_backend_available(self):
        return bool(self._get_uia())

    def warm_element_backend(self):
        self._get_uia()

    def _get_uia(self):
        if self._uia is None:
            try:
                import comtypes.client
                comtypes.client.GetModule("UIAutomationCore.dll")
                from comtypes.gen.UIAutomationClient import CUIAutomation, IUIAutomation
                self._uia = comtypes.client.CreateObject(CUIAutomation, interface=IUIAutomation)
            except Exception:
                self._uia = False
        return self._uia

    def element_source(self):
        return self._elem_source

    def element_rects(self, exclude_pids=(), budget_s=0.6, max_depth=10):
        uia = self._get_uia()
        if uia:
            try:
                r = self._uia_rects(uia, exclude_pids, budget_s, max_depth)
                if r:
                    self._elem_source = "UI Automation"; return r + self._child_rects(exclude_pids)
            except Exception:
                pass
        self._elem_source = "child windows"
        return self._child_rects(exclude_pids)

    def _uia_rects(self, uia, exclude_pids, budget_s, max_depth):
        """Control-view walk of visible top-level windows (time-budgeted)."""
        screens = _screens(); t_end = time.monotonic() + budget_s; out = []
        walker = uia.ControlViewWalker

        def add(el, depth):
            if el is None or depth > max_depth or time.monotonic() > t_end: return
            try:
                if not el.CurrentIsOffscreen:
                    r = el.CurrentBoundingRectangle
                    w, h = r.right - r.left, r.bottom - r.top
                    if w > 3 and h > 3:
                        out.append((_qrect(r.left, r.top, w, h, screens),
                                    f"{el.CurrentLocalizedControlType}:{el.CurrentName or ''}"))
                child = walker.GetFirstChildElement(el)
                while child is not None and time.monotonic() <= t_end:
                    add(child, depth + 1)
                    child = walker.GetNextSiblingElement(child)
            except Exception:
                return

        u = _u32(); hwnds = []
        def cb(hwnd, _):
            if u.IsWindowVisible(hwnd) and not u.IsIconic(hwnd) and not _cloaked(hwnd) \
                    and _window_pid(hwnd) not in exclude_pids:
                hwnds.append(hwnd)
            return len(hwnds) < 12
        u.EnumWindows(WNDENUMPROC(cb), 0)
        for hwnd in hwnds:
            try: add(uia.ElementFromHandle(hwnd), 0)
            except Exception: continue
        return out

    def _child_rects(self, exclude_pids=(), min_size=8):
        u = _u32(); screens = _screens(); out = []
        tops = []
        def top_cb(hwnd, _):
            if u.IsWindowVisible(hwnd) and not u.IsIconic(hwnd) and not _cloaked(hwnd) \
                    and _window_pid(hwnd) not in exclude_pids:
                tops.append(hwnd)
            return True
        u.EnumWindows(WNDENUMPROC(top_cb), 0)

        def child_cb(hwnd, _):
            try:
                if not u.IsWindowVisible(hwnd): return True
                r = wt.RECT()
                if u.GetWindowRect(hwnd, ctypes.byref(r)):
                    w, h = r.right - r.left, r.bottom - r.top
                    if w >= min_size and h >= min_size:
                        out.append((_qrect(r.left, r.top, w, h, screens), f"hwnd:{int(hwnd or 0):#x}"))
            except Exception:
                pass
            return True
        proc = WNDENUMPROC(child_cb)
        for t in tops:
            u.EnumChildWindows(t, proc, 0)
        out.sort(key=lambda rt: rt[0].width() * rt[0].height())
        return out

    # ---- autostart (HKCU Run)
    def autostart_description(self):
        return r"Run at login (HKCU\...\CurrentVersion\Run)"

    def autostart_is_enabled(self):
        import winreg
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as k:
                winreg.QueryValueEx(k, RUN_VALUE); return True
        except OSError:
            return False

    def autostart_set_enabled(self, on):
        import winreg
        with winreg.CreateKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as k:
            if on:
                winreg.SetValueEx(k, RUN_VALUE, 0, winreg.REG_SZ, run_key_command(self.launch_command()))
            else:
                try: winreg.DeleteValue(k, RUN_VALUE)
                except OSError: pass
        return RUN_KEY

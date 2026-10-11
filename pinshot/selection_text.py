"""Hotkey-driven Windows selection copy; restore clipboard even on errors."""
import sys
from PySide6.QtCore import QObject, QTimer, QMimeData
from PySide6.QtGui import QGuiApplication


def send_copy():
    import ctypes as C
    from ctypes import wintypes as W
    class Mouse(C.Structure):
        _fields_ = [('dx', W.LONG), ('dy', W.LONG), ('data', W.DWORD), ('flags', W.DWORD), ('time', W.DWORD), ('extra', C.c_size_t)]
    class Key(C.Structure):
        _fields_ = [('vk', W.WORD), ('scan', W.WORD), ('flags', W.DWORD), ('time', W.DWORD), ('extra', C.c_size_t)]
    class Union(C.Union):
        _fields_ = [('mouse', Mouse), ('key', Key)]
    class Input(C.Structure):
        _fields_ = [('type', W.DWORD), ('u', Union)]
    user32 = C.WinDLL('user32', use_last_error=True)
    user32.SendInput.argtypes = [W.UINT, C.POINTER(Input), C.c_int]
    user32.SendInput.restype = W.UINT
    keys = []
    for vk in (0x12, 0x10, 0x5B, 0x5C):
        if user32.GetAsyncKeyState(vk) & 0x8000: keys.append((vk, 2))
    keys += [(0x11, 0), (0x43, 0), (0x43, 2), (0x11, 2)]
    events = (Input * len(keys))(*(Input(1, Union(key=Key(vk, 0, flags, 0, 0))) for vk, flags in keys))
    if user32.SendInput(len(keys), events, C.sizeof(Input)) != len(keys):
        raise RuntimeError('Could not copy selected text.')


class SelectionReader(QObject):
    def __init__(self, parent=None):
        super().__init__(parent); self.active = False
        self.timer = QTimer(self); self.timer.setInterval(20); self.timer.timeout.connect(self._poll)

    def read(self, callback):
        if self.active: return
        if sys.platform != 'win32': callback('', 'Select-text translate is only available here.'); return
        import time
        self.callback = callback; self.active = True; self.deadline = time.monotonic() + .5
        clipboard = QGuiApplication.clipboard(); self.backup = QMimeData()
        current = clipboard.mimeData()
        if current:
            for fmt in current.formats(): self.backup.setData(fmt, current.data(fmt))
            if current.hasImage(): self.backup.setImageData(current.imageData())
        try:
            clipboard.clear(); send_copy(); self.timer.start()
        except Exception as exc: self._finish('', str(exc))

    def _poll(self):
        import time
        text = QGuiApplication.clipboard().text()
        if text.strip() or time.monotonic() >= self.deadline: self._finish(text.strip(), '')

    def _finish(self, text, error):
        self.timer.stop()
        try: QGuiApplication.clipboard().setMimeData(self.backup)
        finally:
            self.backup = None; self.active = False
        self.callback(text, error)

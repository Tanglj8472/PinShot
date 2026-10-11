"""Real Win32 smoke test (Windows only; needs a desktop session). Exits 0 with a skip note elsewhere.
Checks: DPI awareness call, window enumeration + DWM bounds, element detection, RegisterHotKey,
click-through ex-style bits, display affinity, HKCU Run round-trip, single-instance server, capture."""
import os, sys, ctypes
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if sys.platform != "win32":
    print("win_smoke: skipped (not Windows)"); sys.exit(0)

from pinshot.platform import current
from pinshot.platform import windows as W
P = current(); P.init_process()
from PySide6.QtWidgets import QApplication, QWidget
from PySide6.QtCore import Qt, QTimer
from PySide6.QtTest import QTest
app = QApplication(sys.argv)
fails = []
def check(name, cond, extra=""):
    print(("PASS " if cond else "FAIL ") + name, extra, flush=True)
    if not cond: fails.append(name)

check("backend is windows", P.name == "windows")
wins = P.window_rects(exclude_pids=())
check("EnumWindows found windows", isinstance(wins, list), len(wins))
w = QWidget(); w.setWindowTitle("PinShotSmoke"); w.setGeometry(200, 200, 300, 200)
w.setWindowFlags(Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint); w.show(); QTest.qWait(300)
wins = P.window_rects(exclude_pids=())
check("own window listed with DWM bounds", any(t == "PinShotSmoke" for _, t in wins), [t for _, t in wins][:5])
els = P.element_rects(exclude_pids=())
check("element detection returns list", isinstance(els, list), (P.element_source(), len(els)))
from pinshot.hotkeys import HotkeyBus
name, helper = P.install_hotkeys(HotkeyBus(), {"snip": "Ctrl+Alt+Shift+F11"})
check("RegisterHotKey", name == "win32-registerhotkey", (name, getattr(helper, "failed", helper)))
if hasattr(helper, "stop"): helper.stop()
w.setWindowOpacity(0.5); QTest.qWait(50)
check("click-through on", P.set_clickthrough(w, True))
u = ctypes.windll.user32; u.GetWindowLongPtrW.restype = ctypes.c_ssize_t
ex = u.GetWindowLongPtrW(int(w.winId()), W.GWL_EXSTYLE)
check("WS_EX_TRANSPARENT|LAYERED set", bool(ex & W.WS_EX_TRANSPARENT) and bool(ex & W.WS_EX_LAYERED), hex(ex))
P.set_clickthrough(w, False)
ex = u.GetWindowLongPtrW(int(w.winId()), W.GWL_EXSTYLE)
check("WS_EX_TRANSPARENT cleared", not (ex & W.WS_EX_TRANSPARENT), hex(ex))
print("info: exclude_from_capture ->", P.exclude_from_capture(w, True), "(False before Win10 2004)")
was = P.autostart_is_enabled()
P.autostart_set_enabled(True); on = P.autostart_is_enabled()
P.autostart_set_enabled(was); check("HKCU Run round-trip", on and P.autostart_is_enabled() == was)
from pinshot.ipc import IpcServer, try_send
srv = IpcServer(app); got = []
srv.command.connect(got.append)
check("single-instance send", try_send({"cmd": "raise"}) is True)
QTest.qWait(100); check("single-instance delivered", bool(got), got)
pm, vg = P.capture_virtual()
check("capture virtual desktop", not pm.isNull() and pm.width() == vg.width(), (pm.width(), pm.height(), vg.getRect()))
print("SUMMARY:", "OK" if not fails else f"FAILED {fails}", flush=True)
os._exit(1 if fails else 0)

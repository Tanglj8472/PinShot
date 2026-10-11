"""Platform abstraction. `current()` returns the backend for this OS (cached).

Backends: linux_x11.LinuxX11Platform (X11/XWayland), windows.WindowsPlatform (Win32 via ctypes),
base.BasePlatform (portable Qt-only fallback, e.g. macOS / pure Wayland)."""
import sys

_CURRENT = None


def current():
    global _CURRENT
    if _CURRENT is None:
        if sys.platform == "win32":
            from .windows import WindowsPlatform as P
        elif sys.platform.startswith("linux"):
            from .linux_x11 import LinuxX11Platform as P
        else:
            from .base import BasePlatform as P
        _CURRENT = P()
    return _CURRENT


def set_current(p):
    """Tests: force a backend."""
    global _CURRENT
    _CURRENT = p


def _log(msg):
    print("[pinshot]", msg, flush=True)

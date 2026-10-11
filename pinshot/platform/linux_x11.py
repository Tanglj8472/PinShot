"""Linux / X11 backend: thin delegation to the existing, tested X11 modules."""
from PySide6.QtGui import QGuiApplication
from .base import BasePlatform


class LinuxX11Platform(BasePlatform):
    name = "linux-x11"

    def _xcb(self):
        return QGuiApplication.platformName() == "xcb"

    def instance_name(self):
        from ..ipc import SERVER_NAME
        return SERVER_NAME

    def input_hold(self):
        if not self._xcb(): return None
        from ..capture import InputHold
        return InputHold()

    def install_hotkeys(self, bus, bindings):
        from ..hotkeys import install_global
        return install_global(bus, bindings)

    def set_clickthrough(self, widget, on):
        if not self._xcb(): return False
        from ..x11pass import set_input_passthrough
        return set_input_passthrough(widget, on)

    def window_rects(self, exclude_pids=()):
        from ..winlist import window_rects
        return window_rects(exclude_pids=exclude_pids)

    def element_rects(self, exclude_pids=()):
        from ..winlist import element_rects
        return element_rects(exclude_pids=exclude_pids)

    def element_source(self):
        from .. import winlist
        return winlist.LAST_SOURCE

    def warm_element_backend(self):
        from ..winlist import atspi_available
        atspi_available()

    def element_backend_available(self):
        from ..winlist import atspi_available
        return atspi_available()

    def clipboard_read(self, prefer=None):
        if not self._xcb(): return None
        from . import _log
        from .. import x11clip
        return x11clip.read(log=_log, prefer=prefer) if prefer else x11clip.read(log=_log)

    def autostart_description(self):
        return "Run at login (writes ~/.config/autostart/pinshot.desktop)"

    def autostart_is_enabled(self):
        from .. import autostart
        return autostart.is_enabled()

    def autostart_set_enabled(self, on):
        from .. import autostart
        return autostart.set_enabled(on)

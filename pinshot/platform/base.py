"""Portable defaults (Qt only). Every backend subclasses this; missing features degrade to no-ops."""
import getpass, os, re, sys


class BasePlatform:
    name = "generic"

    # ---- process
    def init_process(self):
        """Called before QApplication is created (DPI awareness etc.)."""

    def instance_name(self):
        """QLocalServer name: unique per user (named pipe on Windows, unix socket elsewhere)."""
        try: user = getpass.getuser()
        except Exception: user = str(os.getpid())
        return "pinshot-" + re.sub(r"[^A-Za-z0-9_.-]", "_", user)

    # ---- capture
    def capture_virtual(self):
        """(QPixmap, QRect virtualGeometry) of all screens in logical (Qt) coordinates."""
        from ..capture import grab_virtual
        return grab_virtual()

    def input_hold(self):
        """Optional object with .release(): holds input while the overlay maps (X11 only)."""
        return None

    def exclude_from_capture(self, widget, on=True) -> bool:
        """Hide a window from screen capture (Windows 10 2004+). False if unsupported."""
        return False

    # ---- hotkeys
    def install_hotkeys(self, bus, bindings):
        """Register global hotkeys; bus.fired.emit(cmd) on the Qt thread. Returns (backend name | None, helper)."""
        return None, None

    # ---- click-through
    def raise_on_top(self, widget, lower=()) -> bool:
        """Put widget above all windows (incl. other always-on-top ones); lower `lower`."""
        for w in lower: w.lower()
        widget.raise_(); widget.activateWindow()
        return True

    def set_clickthrough(self, widget, on) -> bool:
        """OS-level extra beyond Qt.WindowTransparentForInput. True if applied."""
        return False

    # ---- detection (global logical coords; topmost first)
    def window_rects(self, exclude_pids=()):
        return []

    def element_rects(self, exclude_pids=()):
        return []

    def element_source(self) -> str:
        return "none"

    def warm_element_backend(self):
        """Background probe at startup (may block; called from a thread)."""

    def element_backend_available(self) -> bool:
        return False

    # ---- clipboard extras
    def clipboard_read(self, prefer=None):
        """(target, bytes) read directly from the OS when Qt's view may be stale, or None."""
        return None

    # ---- autostart
    def autostart_description(self) -> str:
        return "Run at login"  # i18n: keep this exact English key

    def autostart_is_enabled(self) -> bool:
        return False

    def autostart_set_enabled(self, on: bool):
        raise NotImplementedError("autostart not supported on this platform")

    @staticmethod
    def launch_command():
        """Command line that starts PinShot (frozen exe or interpreter + -m pinshot)."""
        if getattr(sys, "frozen", False):
            return [sys.executable]
        return [sys.executable, "-m", "pinshot"]

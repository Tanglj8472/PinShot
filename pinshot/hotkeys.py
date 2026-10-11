"""Global hotkeys: pynput if available, else X11 XGrabKey via python-xlib (X11/XWayland only)."""
from PySide6.QtCore import QObject, Signal, QTimer, QAbstractNativeEventFilter, QCoreApplication


class HotkeyBus(QObject):
    fired = Signal(str)   # command name: snip | pin | hide_all | clickthrough


_MOD = {
    "shift": 1, "lock": 2, "ctrl": 4, "control": 4, "mod1": 8, "alt": 8,
    "mod2": 16, "mod3": 32, "mod4": 64, "super": 64, "meta": 64, "mod5": 128,
}


def parse_seq(seq: str):
    """'Shift+F3' -> (mods, keysym_name). Empty seq -> None."""
    seq = (seq or "").strip()
    if not seq: return None
    parts = [p.strip().lower() for p in seq.replace("-", "+").split("+") if p.strip()]
    if not parts: return None
    key = parts[-1]; mods = 0
    for p in parts[:-1]:
        if p not in _MOD: return None
        mods |= _MOD[p]
    # normalize key names for Xlib keysym
    aliases = {"esc": "escape", "return": "return", "enter": "return", "del": "delete",
               "pgup": "page_up", "pgdn": "page_down", "pageup": "page_up", "pagedown": "page_down"}
    key = aliases.get(key, key)
    return mods, key


class X11Hotkeys(QAbstractNativeEventFilter):
    """Grab keys on the root window; deliver via HotkeyBus on the Qt thread."""

    def __init__(self, bus: HotkeyBus, bindings: dict):
        super().__init__()
        self.bus = bus; self.bindings = {}  # (mods, keycode) -> cmd
        self.d = None; self.ok = False
        try:
            from Xlib import display, X
            from Xlib.XK import string_to_keysym
            self.X = X
            self.d = display.Display()
            self.root = self.d.screen().root
            # Ignore CapsLock / NumLock variants
            self.ignore = [0, 2, 16, 18]   # lock, mod2, lock|mod2
            for cmd, seq in bindings.items():
                parsed = parse_seq(seq)
                if not parsed: continue
                mods, name = parsed
                ks = string_to_keysym(name) or string_to_keysym(name.upper()) or string_to_keysym(name.capitalize())
                if not ks: continue
                code = self.d.keysym_to_keycode(ks)
                if not code: continue
                for ig in self.ignore:
                    try:
                        self.root.grab_key(code, mods | ig, True, X.GrabModeAsync, X.GrabModeAsync)
                    except Exception:
                        pass
                self.bindings[(mods, code)] = cmd
            self.d.flush()
            # Pump X events on a timer (native event filter may not see this Display connection)
            self._timer = QTimer(); self._timer.timeout.connect(self._pump); self._timer.start(20)
            self.ok = bool(self.bindings)
        except Exception as e:
            self.error = e; self.ok = False

    def _pump(self):
        if not self.d: return
        X = self.X
        while self.d.pending_events():
            ev = self.d.next_event()
            if ev.type == X.KeyPress:
                mods = ev.state & ~(2 | 16)   # strip Lock/NumLock
                cmd = self.bindings.get((mods, ev.detail))
                if cmd: QTimer.singleShot(0, lambda c=cmd: self.bus.fired.emit(c))

    def nativeEventFilter(self, eventType, message):
        return False, 0

    def stop(self):
        if self.d:
            try:
                from Xlib import X
                for (mods, code) in {(m, c) for (m, c) in self.bindings}:
                    for ig in self.ignore:
                        try: self.root.ungrab_key(code, mods | ig)
                        except Exception: pass
                self.d.flush(); self.d.close()
            except Exception:
                pass
            self.d = None
        if getattr(self, "_timer", None): self._timer.stop()


class PynputHotkeys:
    def __init__(self, bus: HotkeyBus, bindings: dict):
        from pynput import keyboard
        mapping = {}
        for cmd, seq in bindings.items():
            p = parse_seq(seq)
            if not p: continue
            mods, key = p
            parts = []
            if mods & 4: parts.append("<ctrl>")
            if mods & 1: parts.append("<shift>")
            if mods & 8: parts.append("<alt>")
            if mods & 64: parts.append("<cmd>")
            parts.append(f"<{key}>" if len(key) > 1 else key)
            mapping["+".join(parts)] = (lambda c=cmd: QTimer.singleShot(0, lambda: bus.fired.emit(c)))
        self._hk = keyboard.GlobalHotKeys(mapping); self._hk.daemon = True; self._hk.start(); self.ok = True

    def stop(self):
        try: self._hk.stop()
        except Exception: pass


def install_global(bus: HotkeyBus, bindings: dict):
    """Try pynput, then X11 GrabKey. Returns (backend_name_or_None, helper)."""
    try:
        h = PynputHotkeys(bus, bindings)
        if h.ok: return "pynput", h
    except Exception:
        pass
    try:
        h = X11Hotkeys(bus, bindings)
        if h.ok: return "x11-grabkey", h
        return None, h
    except Exception as e:
        return None, e

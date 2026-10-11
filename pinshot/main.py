"""PinShot entry: tray + control window, preferences, groups, global hotkeys, session."""
import argparse, os, sys, atexit
from PySide6.QtCore import Qt, QTimer, QPoint, QRect
from PySide6.QtGui import QIcon, QPixmap, QPainter, QColor, QKeySequence, QShortcut, QGuiApplication, QAction
from PySide6.QtWidgets import (QApplication, QSystemTrayIcon, QMenu, QWidget, QVBoxLayout, QHBoxLayout,
                               QPushButton, QLabel, QListWidget, QListWidgetItem, QComboBox)
from .capture import grab_primary, grab_virtual
from .clipboard import copy_pixmap, clipboard_pixmap
from .overlay import SnipOverlay
from .pin_window import PinWindow, ALL_PINS
from . import session as SES
from . import settings as S
from .hotkeys import HotkeyBus, install_global
from .prefs import PrefsDialog


def _icon():
    from .icons import app_image
    ic = QIcon()
    for n in (16, 24, 32, 48, 64, 256): ic.addPixmap(QPixmap.fromImage(app_image(n)))
    return ic


class App:
    def __init__(self, qapp):
        from . import i18n, theme
        theme.apply(S.get("theme") or "system"); theme.watch_system()
        i18n.install(qapp, S.get("language") or "en")
        self.qapp = qapp; self.overlay = None; self._snip_pending = False
        from .translation import TranslationController
        from .selection_text import SelectionReader
        self.translation = TranslationController(qapp)
        self.selection_reader = SelectionReader(qapp)
        self.hk_backend = None; self._hk_helper = None
        self.bus = HotkeyBus(); self.bus.fired.connect(self._on_hotkey)
        self.win = QWidget(); self.win.setWindowTitle("PinShot"); self.win.setWindowIcon(_icon())
        lay = QVBoxLayout(self.win)
        self.status = QLabel(); self.status.setWordWrap(True); lay.addWidget(self.status)
        drow = QHBoxLayout(); drow.addWidget(QLabel("Snip delay:"))
        self.delay_box = QComboBox()
        for sec in (0, 1, 2, 3, 5): self.delay_box.addItem(f"{sec}s" if sec else "0s", sec)
        di = self.delay_box.findData(S.get("snip_delay", int)); self.delay_box.setCurrentIndex(max(0, di))
        self.delay_box.currentIndexChanged.connect(lambda: S.set_("snip_delay", int(self.delay_box.currentData())))
        drow.addWidget(self.delay_box); lay.addLayout(drow)
        for text, fn in [("Snip (F1)", self.snip), ("Pin clipboard (F3)", self.pin_clipboard),
                         ("截图翻译 (F6)", self.capture_translate),
                         ("Hide/Show pins (Shift+F3)", self.toggle_pins),
                         ("Restore last closed pin", self.restore_closed),
                         ("Preferences…", self.open_prefs)]:
            b = QPushButton(text); b.clicked.connect(fn); lay.addWidget(b)
        # Groups list
        lay.addWidget(QLabel("Pin groups (double-click hide/show):"))
        self.group_list = QListWidget(); self.group_list.itemDoubleClicked.connect(self._toggle_group_item)
        lay.addWidget(self.group_list)
        grow = QHBoxLayout()
        for t, fn in [("Add", self._add_group), ("Rename", self._ren_group), ("Remove", self._rm_group)]:
            b = QPushButton(t); b.clicked.connect(fn); grow.addWidget(b)
        lay.addLayout(grow)
        qb = QPushButton("Quit"); qb.clicked.connect(self.quit); lay.addWidget(qb)
        self.tray = None
        SES.BUS.groups_changed.connect(self._rebuild_groups)
        self._bind_local_shortcuts()
        self._rebuild_groups()
        if QSystemTrayIcon.isSystemTrayAvailable():
            self.tray = QSystemTrayIcon(_icon()); self._rebuild_tray()
            self.tray.activated.connect(lambda r: self.snip() if r == QSystemTrayIcon.Trigger else None)
            self.tray.setToolTip("PinShot"); self.tray.show()
        self.reinstall_hotkeys()
        self.ipc = None
        try:
            from .ipc import IpcServer
            self.ipc = IpcServer(qapp)
            self.ipc.command.connect(self._on_ipc)
        except Exception as e:
            print("[pinshot] IPC server:", e, flush=True)
        import threading
        from .platform import current as _plat
        threading.Thread(target=_plat().warm_element_backend, daemon=True).start()   # warm AT-SPI / UIA probe
        atexit.register(lambda: SES.persist_session())

    def _bind_local_shortcuts(self):
        # Remove old shortcuts by parenting fresh ones to win (replace each time prefs change)
        for sc in self.win.findChildren(QShortcut): sc.setParent(None); sc.deleteLater()
        QShortcut(QKeySequence(S.get("hotkey_snip") or "F1"), self.win, self.snip)
        QShortcut(QKeySequence(S.get("hotkey_pin") or "F3"), self.win, self.pin_clipboard)
        QShortcut(QKeySequence(S.get("hotkey_hide_all") or "Shift+F3"), self.win, self.toggle_pins)
        QShortcut(QKeySequence("Ctrl+Shift+P"), self.win, self.open_prefs)
        rs = S.get("hotkey_restore") or ""
        if rs: QShortcut(QKeySequence(rs), self.win, self.restore_closed)
        ct = S.get("hotkey_clickthrough") or ""
        if ct: QShortcut(QKeySequence(ct), self.win, self.toggle_clickthrough)

    def reinstall_hotkeys(self):
        if self._hk_helper:
            try: self._hk_helper.stop()
            except Exception: pass
            self._hk_helper = None
        bindings = {
            "snip": S.get("hotkey_snip") or "F1",
            "pin": S.get("hotkey_pin") or "F3",
            "selection_translate": S.get("hotkey_selection_translate") or "F5",
            "capture_translate": S.get("hotkey_capture_translate") or "F6",
            "hide_all": S.get("hotkey_hide_all") or "Shift+F3",
        }
        ct = S.get("hotkey_clickthrough") or ""
        if ct: bindings["clickthrough"] = ct
        rs = S.get("hotkey_restore") or ""
        if rs: bindings["restore"] = rs
        self.hk_bindings = bindings
        from .platform import current as _plat
        name, helper = _plat().install_hotkeys(self.bus, bindings)
        self.hk_backend = name; self._hk_helper = helper if name else None
        self.status.setText(
            f"PinShot — {bindings['snip']} snip, {bindings['pin']} pin, {bindings['hide_all']} hide/show\n"
            f"Global hotkeys: {name or 'unavailable (X11 GrabKey / pynput; not on pure Wayland)'}\n"
            f"{bindings['selection_translate']} 划词翻译 · {bindings['capture_translate']} 截图翻译\n"
            f"Preferences: Ctrl+Shift+P · Restore closed pin: {rs or 'unbound'}"
        )
        failed = getattr(helper, 'failed', []) if helper else []
        if failed: self.status.setText(self.status.text() + '\n未注册的热键：' + ', '.join(failed))
        self._bind_local_shortcuts()

    def _on_hotkey(self, cmd):
        {"snip": self.snip, "pin": self.pin_clipboard, "hide_all": self.toggle_pins,
         "selection_translate": self.selection_translate, "capture_translate": self.capture_translate,
         "clickthrough": self.toggle_clickthrough, "restore": self.restore_closed}.get(cmd, lambda: None)()

    def _on_ipc(self, m):
        c = m.get("cmd") if isinstance(m, dict) else None
        if c == "snip": self.snip(delay=m.get("delay"))
        elif c == "translate": self.capture_translate()
        elif c == "selection": self.selection_translate()
        elif c == "pin": self.pin_clipboard()
        elif c == "pins": self.toggle_pins()
        elif c == "prefs": self.open_prefs()
        elif c == "quit": self.quit()
        else: self.win.show(); self.win.raise_(); self.win.activateWindow()

    # --- groups UI
    def _rebuild_groups(self):
        self.group_list.clear()
        for name in SES.group_names():
            it = QListWidgetItem(name + ("" if SES.group_visible(name) else "  (hidden)"))
            it.setData(Qt.UserRole, name); self.group_list.addItem(it)
        self._rebuild_tray()

    def _toggle_group_item(self, item):
        name = item.data(Qt.UserRole)
        SES.set_group_visible(name, not SES.group_visible(name)); self._rebuild_groups()

    def _add_group(self):
        from PySide6.QtWidgets import QInputDialog
        name, ok = QInputDialog.getText(self.win, "Add group", "Name:")
        if ok and name.strip():
            if SES.find_group(name): return self._notify(f"Group '{name.strip()}' already exists")
            SES.ensure_group(name.strip()); self._rebuild_groups()

    def _ren_group(self):
        from PySide6.QtWidgets import QInputDialog
        it = self.group_list.currentItem()
        if not it: return
        old = it.data(Qt.UserRole)
        name, ok = QInputDialog.getText(self.win, "Rename group", "Name:", text=old)
        if ok and name.strip():
            if not SES.rename_group(old, name.strip()): self._notify("Rename rejected (duplicate or Default)")
            self._rebuild_groups()

    def _rm_group(self):
        it = self.group_list.currentItem()
        if not it: return
        SES.remove_group(it.data(Qt.UserRole)); self._rebuild_groups()

    def _rebuild_tray(self):
        if not self.tray: return
        m = QMenu()
        m.addAction("Snip", self.snip); m.addAction("Pin clipboard", self.pin_clipboard)
        m.addAction("截图翻译 (F6)", self.capture_translate)
        m.addAction("Hide/Show all pins", self.toggle_pins)
        m.addAction("Restore last closed pin", self.restore_closed)
        gm = m.addMenu("Groups")
        for name in SES.group_names():
            vis = SES.group_visible(name)
            a = gm.addAction(("Hide " if vis else "Show ") + name,
                             lambda n=name, v=vis: (SES.set_group_visible(n, not v), self._rebuild_groups()))
        m.addAction("Control window", self.win.show)
        m.addAction("Preferences…", self.open_prefs)
        m.addSeparator(); m.addAction("Quit", self.quit)
        self.tray.setContextMenu(m)

    # --- core actions
    def snip(self, then=None, delay=None):
        if self.overlay or self._snip_pending: return
        if delay is None: delay = int(S.get("snip_delay", int) or 0)
        if delay > 0:
            self._countdown_snip(delay, then); return
        self._snip_now(then)

    def _countdown_snip(self, seconds, then=None):
        """Hide UI briefly is NOT done yet — show a top-level countdown so menus can open, then snip."""
        self._snip_pending = True
        from PySide6.QtWidgets import QLabel as _QL
        lab = _QL(None, Qt.Window | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool)
        lab.setStyleSheet("background:rgba(20,20,20,200);color:white;font-size:48px;font-weight:bold;padding:24px;border-radius:12px;")
        from .platform import current as _plat
        QTimer.singleShot(0, lambda: _plat().exclude_from_capture(lab))   # countdown never in the shot (Windows)
        lab.setAlignment(Qt.AlignCenter); self._countdown_label = lab
        left = [seconds]
        def tick():
            if left[0] <= 0:
                lab.hide(); lab.deleteLater(); self._snip_pending = False; self._snip_now(then); return
            lab.setText(str(left[0])); lab.adjustSize()
            scr = QGuiApplication.primaryScreen().availableGeometry()
            lab.move(scr.center() - QPoint(lab.width() // 2, lab.height() // 2)); lab.show(); lab.raise_()
            left[0] -= 1; QTimer.singleShot(1000, tick)
        tick()

    def _snip_now(self, then=None):
        from .platform import current as _plat
        self._snip_pending = True
        hold = _plat().input_hold()
        hid = self.win.isVisible()
        if hid: self.win.hide()
        def go():
            try:
                shot, vg = _plat().capture_virtual()
                ov = SnipOverlay(shot, vg)
            except Exception:
                if hold: hold.release()
                self._snip_pending = False; raise
            self.overlay = ov; ov.pre_grab = hold; self._snip_pending = False
            ov.copied.connect(lambda pm: (copy_pixmap(pm), self._notify("Copied to clipboard")))
            ov.color_copied.connect(lambda t: self._notify(f"Color copied: {t}"))
            ov.pinned.connect(lambda pm, pos: PinWindow(pm, pos).show())
            ov.saved.connect(lambda path: self._notify(f"Saved {path}"))
            ov.text_requested.connect(self.translation.image)
            ov.destroyed.connect(lambda *_: setattr(self, 'overlay', None))
            ov.setAttribute(Qt.WA_DeleteOnClose)
            if hid: ov.destroyed.connect(lambda *_: self.win.show())
            ov.start()
            if then: then(ov)
        QTimer.singleShot(150 if hid else 0, go)

    def capture_translate(self):
        def configure(ov):
            ov.translate_on_select = True
            ov.show_toast('框选文字区域，松开鼠标后识别并翻译')
        self.snip(then=configure)

    def selection_translate(self):
        if self.overlay: return
        def ready(text, error):
            if error: self._notify(error)
            else: self.translation.text(text)
        self.selection_reader.read(ready)

    def pin_clipboard(self):
        from PySide6.QtGui import QCursor
        from .cards import parse_color, color_card, text_card
        from .clipboard import clipboard_text
        pm = clipboard_pixmap(); centered = False
        if pm is None:
            txt = clipboard_text()
            if not txt.strip(): return self._notify("Clipboard has no image or text")
            c = parse_color(txt.strip())
            pm = color_card(c) if c is not None else text_card(txt); centered = True
        scr = QGuiApplication.screenAt(QCursor.pos()) or QGuiApplication.primaryScreen()
        g = scr.availableGeometry()
        n = len(ALL_PINS)
        w = PinWindow(pm)
        base = QCursor.pos() + QPoint(16, 16)
        if centered or not g.contains(base): base = g.center() - QPoint(w.width() // 2, w.height() // 2)
        k = sum(1 for p in ALL_PINS if p is not w and getattr(p, "is_card", False)) if centered else n
        pos = base + QPoint(28 * (k % 8), 28 * (k % 8))
        if centered:
            w.is_card = True
            cw = self.win.frameGeometry() if self.win.isVisible() else None
            if cw is not None and QRect(pos, w.size()).intersects(cw):
                below = cw.bottom() + 12; above = cw.top() - 12 - w.height()
                pos.setY(below if below + w.height() <= g.bottom() else max(g.top(), above))
        pos.setX(max(g.left(), min(pos.x(), g.right() - w.width())))
        pos.setY(max(g.top(), min(pos.y(), g.bottom() - w.height())))
        w.move(pos); w.show(); w.raise_(); w.activateWindow()
        if not SES.group_visible(w.group): w.hide()
        return w

    def toggle_pins(self):
        vis = any(p.isVisible() for p in ALL_PINS)
        for p in ALL_PINS: p.setVisible(not vis)

    def toggle_clickthrough(self):
        """Hotkey: toggle click-through on the pin under the cursor; if none, clear all click-through."""
        from PySide6.QtGui import QCursor
        pos = QCursor.pos()
        hit = None
        for p in reversed(list(ALL_PINS)):   # topmost last in list ≈ raise order; check geometry
            if p.isVisible() and p.frameGeometry().contains(pos) and not p.clickthrough:
                hit = p; break
        if hit is None:
            # also hit a pin that already has click-through (under cursor) to turn it off
            for p in reversed(list(ALL_PINS)):
                if p.isVisible() and p.frameGeometry().contains(pos):
                    hit = p; break
        if hit is not None:
            hit.set_clickthrough(not hit.clickthrough)
            self._notify("Click-through " + ("ON" if hit.clickthrough else "OFF"))
        else:
            n = 0
            for p in ALL_PINS:
                if p.clickthrough: p.set_clickthrough(False); n += 1
            self._notify(f"Cleared click-through on {n} pin(s)" if n else "No pin under cursor")

    def restore_closed(self):
        w = SES.restore_last_closed()
        self._notify("Restored closed pin" if w else "No closed pins to restore")
        return w

    def _sync_delay_box(self):
        di = self.delay_box.findData(S.get("snip_delay", int))
        self.delay_box.blockSignals(True)
        self.delay_box.setCurrentIndex(max(0, di))
        self.delay_box.blockSignals(False)

    def apply_appearance(self, language=None, theme_mode=None):
        """Live: language + theme from settings (or explicit args, which are also persisted)."""
        from . import i18n, theme
        if language is not None: S.set_("language", language)
        if theme_mode is not None: S.set_("theme", theme_mode)
        theme.apply(S.get("theme") or "system")
        i18n.install(self.qapp, S.get("language") or "en")
        self._rebuild_tray()

    def open_prefs(self):
        d = PrefsDialog(self.win)
        d.applied.connect(lambda: (self.apply_appearance(), self.reinstall_hotkeys(), self._rebuild_groups(), self._sync_delay_box()))
        d.exec()

    def quit(self):
        if self.selection_reader.active: self.selection_reader._finish('', '已取消')
        SES.persist_session()
        if self._hk_helper:
            try: self._hk_helper.stop()
            except Exception: pass
        self.qapp.quit()

    def _notify(self, msg):
        from .i18n import tr
        msg = tr(msg)
        if self.tray: self.tray.showMessage("PinShot", msg, QSystemTrayIcon.Information, 1500)
        else: print("[pinshot]", msg, flush=True)


def main(argv=None):
    """CLI:
      python -m pinshot                  # start / raise control window
      python -m pinshot snip [--delay N]
      python -m pinshot pin
      python -m pinshot pins             # hide/show all pins
      python -m pinshot prefs
      python -m pinshot --snip [--delay N]
      python -m pinshot --selftest
    Second process forwards the command to the first via QLocalServer then exits.
    """
    argv = list(sys.argv[1:] if argv is None else argv)
    ap = argparse.ArgumentParser(prog="pinshot")
    ap.add_argument("command", nargs="?", choices=["snip", "pin", "pins", "prefs", "quit", "translate", "selection"], help="IPC command")
    ap.add_argument("--snip", action="store_true", help="same as command snip")
    ap.add_argument("--delay", type=int, default=None, help="snip delay seconds")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--version", action="store_true", help="print version + platform backend and exit")
    a = ap.parse_args(argv)
    if a.version:
        from . import __version__
        from .platform import current as _plat
        line = f"PinShot {__version__} ({_plat().name})"
        print(line, flush=True)                      # no-op in a --windowed exe (stdout is None)
        out = os.environ.get("PINSHOT_VERSION_OUT")
        if out:
            with open(out, "w", encoding="utf-8") as f: f.write(line + "\n")
        return 0

    cmd = a.command
    if a.snip: cmd = "snip"
    msg = {"cmd": cmd or "raise"}
    if a.delay is not None: msg["delay"] = a.delay

    from .ipc import try_send
    if not a.selftest and try_send(msg):
        print("[pinshot] forwarded to running instance:", msg, flush=True)
        return 0

    from .platform import current as _plat
    _plat().init_process()                      # per-monitor DPI v2 on Windows (before QApplication)
    QApplication.setAttribute(Qt.AA_CompressHighFrequencyEvents, False)
    qapp = QApplication(sys.argv[:1]); qapp.setQuitOnLastWindowClosed(False)
    app = App(qapp)

    if S.get("start_minimized", bool) and not a.selftest and cmd not in ("snip", "prefs"):
        app.win.hide()
    else:
        app.win.show()
    try: SES.restore_session()
    except Exception as e: print("[pinshot] session restore:", e, flush=True)

    if a.selftest:
        QTimer.singleShot(300, lambda: _selftest(app))
    elif cmd == "snip":
        app.snip(delay=a.delay)
    elif cmd == "translate":
        app.capture_translate()
    elif cmd == "selection":
        QTimer.singleShot(100, app.selection_translate)
    elif cmd == "pin":
        app.pin_clipboard()
    elif cmd == "pins":
        app.toggle_pins()
    elif cmd == "prefs":
        QTimer.singleShot(100, app.open_prefs)

    rc = qapp.exec()
    for p in list(ALL_PINS): p.close()
    if app.tray: app.tray.hide()
    app.win.deleteLater(); del app
    return rc



def _selftest(app):
    from PySide6.QtCore import QRect, QPointF
    from .overlay import HISTORY
    def on_overlay(ov):
        ov.cur = QPoint(50, 50)
        c = ov.copy_color(); ov.hex_fmt = not ov.hex_fmt; c2 = ov.copy_color()
        print("selftest: color", c, "/", c2, "clipboard text:", QGuiApplication.clipboard().text())
        ov.mag_forced = True; ov.repaint(); ov.mag_forced = False
        ov.sel = QRect(10, 10, 120, 80); ov.state = "selected"
        ov.nudge(1, 0); ov.nudge(0, 1, 'grow'); ov.nudge(1, 0, 'shrink')
        print("selftest: nudged sel", ov.sel.getRect())
        ov._copy()
        pm = clipboard_pixmap()
        print("selftest: clipboard image", None if pm is None else (pm.width(), pm.height()), "history", len(HISTORY))
        if HISTORY: print("selftest: history has image", isinstance(HISTORY[-1], dict) and not HISTORY[-1]["image"].isNull())
        pin = app.pin_clipboard()
        P = QPointF
        pin.add_shape('line', [P(5, 5), P(60, 40)]); pin.add_shape('rect', [P(2, 2), P(50, 30)])
        pin.add_shape('ellipse', [P(60, 5), P(110, 40)]); pin.add_shape('pen', [P(5, 70), P(30, 50), P(60, 70)])
        pin.add_shape('mosaic', [P(70, 40), P(115, 78)], mode="rect"); pin.add_shape('text', [P(4, 40)], text="Hi")
        pin.ann.undo(); pin.ann.redo()
        import tempfile; _sp = os.path.join(tempfile.gettempdir(), "pinshot_selftest.png"); pin.rendered().save(_sp)
        print("selftest: pin", pin.img_rect().size().toTuple(), "shapes", len(pin.ann.shapes),
              "saved", _sp, "; tray:", app.tray is not None, "hk:", app.hk_backend)
        pin.close_recoverable()
        print("selftest: closed stack", len(SES.CLOSED))
        app.restore_closed()
        print("selftest: restored pins", len(ALL_PINS))
        QTimer.singleShot(500, lambda: app.snip(then=on_second))
    def on_second(ov):
        ov.history(-1); print("selftest: history replay ->", ov.sel.getRect(),
                              "img", ov.img.size().toTuple() if ov.img else None)
        ov._cancel()
        QTimer.singleShot(300, app.qapp.quit)
    app.snip(then=on_overlay)

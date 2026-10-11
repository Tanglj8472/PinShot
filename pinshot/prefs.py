"""Preferences dialog: General, Hotkeys, Control/Mouse, Output/Save."""
from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (QDialog, QTabWidget, QWidget, QVBoxLayout, QHBoxLayout, QFormLayout,
                               QCheckBox, QLineEdit, QSpinBox, QComboBox, QPushButton, QLabel,
                               QFileDialog, QListWidget, QInputDialog, QMessageBox)
from . import settings as S
from .platform import current as _plat


def _safe_autostart_enabled():
    try: return _plat().autostart_is_enabled()
    except Exception: return False


class PrefsDialog(QDialog):
    applied = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("PinShot Preferences")
        self.setMinimumWidth(480)
        lay = QVBoxLayout(self)
        self.tabs = QTabWidget(); lay.addWidget(self.tabs)
        self._build_general(); self._build_hotkeys(); self._build_control(); self._build_output()
        row = QHBoxLayout(); row.addStretch()
        apply_b = QPushButton("Apply"); ok_b = QPushButton("OK"); cancel_b = QPushButton("Cancel")
        apply_b.clicked.connect(self._apply); ok_b.clicked.connect(self._ok); cancel_b.clicked.connect(self.reject)
        row.addWidget(apply_b); row.addWidget(ok_b); row.addWidget(cancel_b); lay.addLayout(row)
        self._load()

    def _build_general(self):
        w = QWidget(); f = QFormLayout(w)
        self.run_at_login = QCheckBox(_plat().autostart_description())
        self.start_minimized = QCheckBox("Start minimized (control window hidden; tray / hotkeys still work)")
        self.snip_delay = QComboBox()
        for sec in (0, 1, 2, 3, 5): self.snip_delay.addItem(f"{sec} s" if sec else "None (0 s)", sec)
        self.abort_confirm = QCheckBox("Confirm before aborting a snip (Esc)")
        self.dblclick = QComboBox(); self.dblclick.addItem("Close pin", "close"); self.dblclick.addItem("Hide pin", "hide"); self.dblclick.addItem("Do nothing", "none")
        self.history_max = QSpinBox(); self.history_max.setRange(1, 50)
        self.closed_max = QSpinBox(); self.closed_max.setRange(0, 50)
        f.addRow(self.run_at_login)
        f.addRow(self.start_minimized)
        f.addRow("Snip delay:", self.snip_delay)
        f.addRow(self.abort_confirm)
        f.addRow("Double-click pin:", self.dblclick)
        f.addRow("Snip history size (images):", self.history_max)
        f.addRow("Recoverable closed pins:", self.closed_max)
        from .i18n import LANGS
        from .theme import MODES
        self.language = QComboBox(); self.language.setObjectName("language"); self.language.setProperty("i18n", False)
        for code, name in LANGS: self.language.addItem(name, code)      # native names, never translated
        self.theme = QComboBox(); self.theme.setObjectName("theme")
        for code, name in MODES: self.theme.addItem(name, code)
        f.addRow("Language:", self.language)
        f.addRow("Theme:", self.theme)
        # Groups editor
        f.addRow(QLabel("Pin groups:"))
        self.groups = QListWidget(); self.groups.setMaximumHeight(120); f.addRow(self.groups)
        grow = QHBoxLayout()
        add = QPushButton("Add"); ren = QPushButton("Rename"); rm = QPushButton("Remove")
        add.clicked.connect(self._add_group); ren.clicked.connect(self._ren_group); rm.clicked.connect(self._rm_group)
        grow.addWidget(add); grow.addWidget(ren); grow.addWidget(rm); f.addRow(grow)
        self.tabs.addTab(w, "General")

    def _build_hotkeys(self):
        w = QWidget(); f = QFormLayout(w)
        self.hk_snip = QLineEdit(); self.hk_pin = QLineEdit(); self.hk_hide = QLineEdit(); self.hk_ct = QLineEdit(); self.hk_restore = QLineEdit()
        for ed in (self.hk_snip, self.hk_pin, self.hk_hide, self.hk_ct, self.hk_restore):
            ed.setPlaceholderText("e.g. F1, Shift+F3, Ctrl+Alt+T")
        f.addRow("Snip:", self.hk_snip)
        f.addRow("Pin clipboard:", self.hk_pin)
        f.addRow("Hide / show all pins:", self.hk_hide)
        f.addRow("Toggle click-through:", self.hk_ct)
        f.addRow("Restore last closed pin:", self.hk_restore)
        import sys as _sys
        note = QLabel("Global hotkeys use Win32 RegisterHotKey; a combination already taken by another app "
                      "cannot be bound. Apply to re-bind." if _sys.platform == "win32" else
                      "Global hotkeys use X11 GrabKey on this box (pynput unavailable). "
                      "They do not work on pure Wayland. Apply to re-bind.")
        note.setWordWrap(True); note.setStyleSheet("color:gray"); f.addRow(note)
        self.tabs.addTab(w, "Hotkeys")

    def _build_control(self):
        w = QWidget(); f = QFormLayout(w)
        f.addRow(QLabel("Mouse / control options shared with General (abort confirm, double-click)."))
        f.addRow(QLabel("Click-through hotkey is under Hotkeys."))
        self.tabs.addTab(w, "Control / Mouse")

    def _build_output(self):
        w = QWidget(); f = QFormLayout(w)
        self.save_folder = QLineEdit(); browse = QPushButton("Browse…")
        browse.clicked.connect(self._browse)
        row = QHBoxLayout(); row.addWidget(self.save_folder); row.addWidget(browse)
        wrap = QWidget(); wrap.setLayout(row)
        self.quality = QSpinBox(); self.quality.setRange(1, 100)
        f.addRow("Quick-save folder:", wrap)
        f.addRow("JPEG quality:", self.quality)
        self.tabs.addTab(w, "Output / Save")

    def _browse(self):
        d = QFileDialog.getExistingDirectory(self, "Quick-save folder", self.save_folder.text())
        if d: self.save_folder.setText(d)

    def _add_group(self):
        name, ok = QInputDialog.getText(self, "Add group", "Name:")
        n = name.strip()
        if ok and n and not any(self.groups.item(i).text().lower() == n.lower() for i in range(self.groups.count())):
            self.groups.addItem(n)

    def _ren_group(self):
        it = self.groups.currentItem()
        if not it: return
        name, ok = QInputDialog.getText(self, "Rename group", "Name:", text=it.text())
        n = name.strip()
        if ok and n and not any(self.groups.item(i) is not it and self.groups.item(i).text().lower() == n.lower()
                                for i in range(self.groups.count())):
            it.setText(n)

    def _rm_group(self):
        row = self.groups.currentRow()
        if row < 0: return
        if self.groups.item(row).text() == "Default":
            QMessageBox.information(self, "PinShot", "The Default group cannot be removed.")
            return
        self.groups.takeItem(row)

    def _load(self):
        self.run_at_login.setChecked(_safe_autostart_enabled() or S.get("run_at_login", bool))
        self.start_minimized.setChecked(S.get("start_minimized", bool))
        di = self.snip_delay.findData(S.get("snip_delay", int)); self.snip_delay.setCurrentIndex(max(0, di))
        self.abort_confirm.setChecked(not S.get("skip_abort_confirm", bool))
        idx = self.dblclick.findData(S.get("dblclick_pin_action"))
        self.dblclick.setCurrentIndex(max(0, idx))
        self.history_max.setValue(S.get("history_max", int))
        self.closed_max.setValue(S.get("closed_pins_max", int))
        self.groups.clear()
        for g in S.get("groups"): self.groups.addItem(g)
        self.hk_snip.setText(S.get("hotkey_snip")); self.hk_pin.setText(S.get("hotkey_pin"))
        self.hk_hide.setText(S.get("hotkey_hide_all")); self.hk_ct.setText(S.get("hotkey_clickthrough") or "")
        self.hk_restore.setText(S.get("hotkey_restore") or "")
        self.language.setCurrentIndex(max(0, self.language.findData(S.get("language"))))
        self.theme.setCurrentIndex(max(0, self.theme.findData(S.get("theme"))))
        self.save_folder.setText(S.get("quick_save_folder")); self.quality.setValue(S.get("image_quality", int))

    def _apply(self):
        on = self.run_at_login.isChecked()
        try: _plat().autostart_set_enabled(on)
        except Exception as e:
            QMessageBox.warning(self, "PinShot", f"Could not update autostart desktop file:\n{e}")
        S.set_("run_at_login", on)
        S.set_("start_minimized", self.start_minimized.isChecked())
        S.set_("snip_delay", int(self.snip_delay.currentData()))
        S.set_("skip_abort_confirm", not self.abort_confirm.isChecked())
        S.set_("dblclick_pin_action", self.dblclick.currentData())
        S.set_("history_max", self.history_max.value())
        S.set_("closed_pins_max", self.closed_max.value())
        S.set_("groups", [self.groups.item(i).text() for i in range(self.groups.count())] or ["Default"])
        S.set_("hotkey_snip", self.hk_snip.text().strip() or "F1")
        S.set_("hotkey_pin", self.hk_pin.text().strip() or "F3")
        S.set_("hotkey_hide_all", self.hk_hide.text().strip() or "Shift+F3")
        S.set_("hotkey_clickthrough", self.hk_ct.text().strip())
        S.set_("hotkey_restore", self.hk_restore.text().strip())
        S.set_("quick_save_folder", self.save_folder.text().strip())
        S.set_("image_quality", self.quality.value())
        S.set_("language", self.language.currentData()); S.set_("theme", self.theme.currentData())
        self.applied.emit()

    def _ok(self):
        self._apply(); self.accept()

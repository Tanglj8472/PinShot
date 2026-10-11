"""Preferences dialog: General, Hotkeys, Output. Rare options live under Advanced."""
from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (QDialog, QTabWidget, QWidget, QVBoxLayout, QHBoxLayout, QFormLayout,
                               QCheckBox, QLineEdit, QSpinBox, QComboBox, QPushButton, QLabel,
                               QFileDialog, QListWidget, QListWidgetItem, QInputDialog, QMessageBox, QToolButton)
from . import settings as S
from .platform import current as _plat


def _safe_autostart_enabled():
    try:
        return _plat().autostart_is_enabled()
    except Exception:
        return False


def _form():
    f = QFormLayout()
    f.setContentsMargins(12, 12, 12, 12)
    f.setHorizontalSpacing(16)
    f.setVerticalSpacing(8)
    return f


class PrefsDialog(QDialog):
    applied = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("PinShot Preferences")
        self.setMinimumWidth(440)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(12, 12, 12, 12)
        lay.setSpacing(10)
        self.tabs = QTabWidget()
        lay.addWidget(self.tabs)
        self._build_general()
        self._build_hotkeys()
        self._build_output()
        row = QHBoxLayout()
        row.addStretch()
        apply_b = QPushButton("Apply")
        ok_b = QPushButton("OK")
        cancel_b = QPushButton("Cancel")
        apply_b.clicked.connect(self._apply)
        ok_b.clicked.connect(self._ok)
        cancel_b.clicked.connect(self.reject)
        row.addWidget(apply_b)
        row.addWidget(ok_b)
        row.addWidget(cancel_b)
        lay.addLayout(row)
        self._load()

    def _build_general(self):
        w = QWidget()
        outer = QVBoxLayout(w)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(4)
        f = _form()
        self.run_at_login = QCheckBox("Run at login")
        self.snip_delay = QComboBox()
        for sec in (0, 1, 2, 3, 5):
            self.snip_delay.addItem("None (0 s)" if sec == 0 else f"{sec} s", sec)
        from .i18n import LANGS
        from .theme import MODES
        self.language = QComboBox()
        self.language.setObjectName("language")
        self.language.setProperty("i18n", False)
        self.language.setFocusPolicy(Qt.StrongFocus)
        for code, name in LANGS:
            self.language.addItem(name, code)
        self.theme = QComboBox()
        self.theme.setObjectName("theme")
        for code, name in MODES:
            self.theme.addItem(name, code)
        f.addRow("Language:", self.language)
        f.addRow("Theme:", self.theme)
        f.addRow(self.run_at_login)
        f.addRow("Snip delay:", self.snip_delay)
        outer.addLayout(f)

        self.advanced_btn = QToolButton()
        self.advanced_btn.setText("Advanced")
        self.advanced_btn.setCheckable(True)
        self.advanced_btn.setToolButtonStyle(Qt.ToolButtonTextBesideIcon)
        self.advanced_btn.setArrowType(Qt.RightArrow)
        self.advanced_btn.setAutoRaise(True)
        self.advanced_btn.toggled.connect(self._toggle_advanced)
        outer.addWidget(self.advanced_btn)

        self.advanced = QWidget()
        af = _form()
        self.advanced.setLayout(af)
        self.abort_confirm = QCheckBox("Confirm before abort (Esc)")
        self.dblclick = QComboBox()
        self.dblclick.addItem("Close pin", "close")
        self.dblclick.addItem("Hide pin", "hide")
        self.dblclick.addItem("Do nothing", "none")
        self.history_max = QSpinBox()
        self.history_max.setRange(1, 50)
        self.closed_max = QSpinBox()
        self.closed_max.setRange(0, 50)
        af.addRow(self.abort_confirm)
        af.addRow("Double-click pin:", self.dblclick)
        af.addRow("Snip history:", self.history_max)
        af.addRow("Closed pins:", self.closed_max)
        af.addRow(QLabel("Pin groups:"))
        self.groups = QListWidget()
        self.groups.setMaximumHeight(100)
        af.addRow(self.groups)
        grow = QHBoxLayout()
        add = QPushButton("Add")
        ren = QPushButton("Rename")
        rm = QPushButton("Remove")
        add.clicked.connect(self._add_group)
        ren.clicked.connect(self._ren_group)
        rm.clicked.connect(self._rm_group)
        grow.addWidget(add)
        grow.addWidget(ren)
        grow.addWidget(rm)
        af.addRow(grow)
        self.advanced.hide()
        outer.addWidget(self.advanced)
        outer.addStretch()
        self.tabs.addTab(w, "General")

    def _toggle_advanced(self, on):
        self.advanced_btn.setArrowType(Qt.DownArrow if on else Qt.RightArrow)
        self.advanced.setVisible(on)
        self.adjustSize()

    def _build_hotkeys(self):
        w = QWidget()
        f = _form()
        w.setLayout(f)
        self.hk_snip = QLineEdit()
        self.hk_pin = QLineEdit()
        self.hk_hide = QLineEdit()
        self.hk_restore = QLineEdit()
        self.hk_selection = QLineEdit()
        self.hk_translate = QLineEdit()
        self.hk_ct = QLineEdit()
        for ed in (self.hk_snip, self.hk_pin, self.hk_hide, self.hk_restore,
                   self.hk_selection, self.hk_translate, self.hk_ct):
            ed.setPlaceholderText("F1, Shift+F3")
        f.addRow("Snip:", self.hk_snip)
        f.addRow("Pin clipboard:", self.hk_pin)
        f.addRow("Hide/show pins:", self.hk_hide)
        f.addRow("Restore last pin:", self.hk_restore)
        f.addRow("Select-text translate:", self.hk_selection)
        f.addRow("Snip and translate:", self.hk_translate)
        f.addRow("Click-through:", self.hk_ct)
        note = QLabel("Hotkeys already used by another app cannot be bound. Click Apply to rebind.")
        note.setWordWrap(True)
        note.setStyleSheet("color:gray")
        f.addRow(note)
        self.tabs.addTab(w, "Hotkeys")

    def _build_output(self):
        w = QWidget()
        f = _form()
        w.setLayout(f)
        self.save_folder = QLineEdit()
        browse = QPushButton("Browse…")
        browse.clicked.connect(self._browse)
        row = QHBoxLayout()
        row.setSpacing(8)
        row.addWidget(self.save_folder)
        row.addWidget(browse)
        wrap = QWidget()
        wrap.setLayout(row)
        self.quality = QSpinBox()
        self.quality.setRange(1, 100)
        f.addRow("Save folder:", wrap)
        f.addRow("Quality:", self.quality)
        self.tabs.addTab(w, "Output")

    def _browse(self):
        d = QFileDialog.getExistingDirectory(self, "Quick-save folder", self.save_folder.text())
        if d:
            self.save_folder.setText(d)

    @staticmethod
    def _gid(it):
        v = it.data(Qt.UserRole)
        return v if v else it.text()

    def _add_group(self):
        name, ok = QInputDialog.getText(self, "Add group", "Name:")
        n = name.strip()
        if ok and n and not any(self.groups.item(i).text().lower() == n.lower()
                                for i in range(self.groups.count())):
            self.groups.addItem(n)

    def _ren_group(self):
        it = self.groups.currentItem()
        if not it:
            return
        if self._gid(it) == "Default":
            return
        name, ok = QInputDialog.getText(self, "Rename group", "Name:", text=it.text())
        n = name.strip()
        if n == it.text():
            return
        if ok and n and not any(self.groups.item(i) is not it
                                and self.groups.item(i).text().lower() == n.lower()
                                for i in range(self.groups.count())):
            it.setText(n); it.setData(Qt.UserRole, n)

    def _rm_group(self):
        row = self.groups.currentRow()
        if row < 0:
            return
        if self._gid(self.groups.item(row)) == "Default":
            QMessageBox.information(self, "PinShot", "The Default group cannot be removed.")
            return
        self.groups.takeItem(row)

    def _load(self):
        self.run_at_login.setChecked(_safe_autostart_enabled() or S.get("run_at_login", bool))
        di = self.snip_delay.findData(S.get("snip_delay", int))
        self.snip_delay.setCurrentIndex(max(0, di))
        self.abort_confirm.setChecked(not S.get("skip_abort_confirm", bool))
        idx = self.dblclick.findData(S.get("dblclick_pin_action"))
        self.dblclick.setCurrentIndex(max(0, idx))
        self.history_max.setValue(S.get("history_max", int))
        self.closed_max.setValue(S.get("closed_pins_max", int))
        self.groups.clear()
        from .i18n import group_label
        for g in S.get("groups"):
            it = QListWidgetItem(group_label(g)); it.setData(Qt.UserRole, g)
            self.groups.addItem(it)
        self.hk_snip.setText(S.get("hotkey_snip"))
        self.hk_pin.setText(S.get("hotkey_pin"))
        self.hk_hide.setText(S.get("hotkey_hide_all"))
        self.hk_ct.setText(S.get("hotkey_clickthrough") or "")
        self.hk_restore.setText(S.get("hotkey_restore") or "")
        self.hk_selection.setText(S.get("hotkey_selection_translate"))
        self.hk_translate.setText(S.get("hotkey_capture_translate"))
        self.language.setCurrentIndex(max(0, self.language.findData(S.get("language"))))
        self.theme.setCurrentIndex(max(0, self.theme.findData(S.get("theme"))))
        self.save_folder.setText(S.get("quick_save_folder"))
        self.quality.setValue(S.get("image_quality", int))

    def _apply(self):
        on = self.run_at_login.isChecked()
        try:
            _plat().autostart_set_enabled(on)
        except Exception as e:
            QMessageBox.warning(self, "PinShot", "Could not update autostart:\n" + str(e))
        S.set_("run_at_login", on)
        S.set_("snip_delay", int(self.snip_delay.currentData()))
        S.set_("skip_abort_confirm", not self.abort_confirm.isChecked())
        S.set_("dblclick_pin_action", self.dblclick.currentData())
        S.set_("history_max", self.history_max.value())
        S.set_("closed_pins_max", self.closed_max.value())
        S.set_("groups", [self._gid(self.groups.item(i)) for i in range(self.groups.count())] or ["Default"])
        S.set_("hotkey_snip", self.hk_snip.text().strip() or "F1")
        S.set_("hotkey_pin", self.hk_pin.text().strip() or "F3")
        S.set_("hotkey_hide_all", self.hk_hide.text().strip() or "Shift+F3")
        S.set_("hotkey_clickthrough", self.hk_ct.text().strip())
        S.set_("hotkey_restore", self.hk_restore.text().strip())
        S.set_("hotkey_selection_translate", self.hk_selection.text().strip() or "F5")
        S.set_("hotkey_capture_translate", self.hk_translate.text().strip() or "F6")
        S.set_("quick_save_folder", self.save_folder.text().strip())
        S.set_("image_quality", self.quality.value())
        S.set_("language", self.language.currentData())
        S.set_("theme", self.theme.currentData())
        self.applied.emit()

    def _ok(self):
        self._apply()
        self.accept()

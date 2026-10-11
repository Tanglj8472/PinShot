"""Regression tests for the hands-on test round on c47b1af."""
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
if sys.platform != "win32" and not os.environ.get("DISPLAY"):
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtCore import QCoreApplication, QStandardPaths, Qt
from PySide6.QtWidgets import QApplication

from pinshot import main as M
M.set_app_identity()
app = QApplication.instance() or QApplication([])

from pinshot import i18n
from pinshot.i18n import tr, group_label, STRINGS
from pinshot.overlay import HINTS


class AppIdentity(unittest.TestCase):
    def test_names(self):
        self.assertEqual(QCoreApplication.applicationName(), "PinShot")
        self.assertEqual(QCoreApplication.organizationName(), "PinShot")

    def test_data_dir(self):
        from pinshot.settings import data_dir
        d = data_dir()
        self.assertEqual(d.name, "PinShot")
        self.assertNotEqual(d.parent.name, "PinShot")       # not nested PinShot/PinShot
        self.assertNotIn("__main__", str(d))
        gen = QStandardPaths.writableLocation(QStandardPaths.GenericDataLocation)
        self.assertEqual(str(d.parent), gen)

    def test_legacy_dirs_listed(self):
        with tempfile.TemporaryDirectory() as t:
            new = Path(t, "PinShot")
            names = {str(p.relative_to(t)) for p in M.legacy_data_dirs(new)}
            self.assertIn("PinShot/PinShot", names)
            self.assertIn("__main__.py", names)

    def test_migrate_nested_into_parent(self):
        with tempfile.TemporaryDirectory() as t:
            new = Path(t, "PinShot"); old = new / "PinShot" / "ocr-models"; old.mkdir(parents=True)
            (old / "a.onnx").write_bytes(b"a")
            M.migrate_legacy_data(new / "PinShot", new)
            self.assertTrue((new / "ocr-models" / "a.onnx").exists())
            self.assertFalse((new / "PinShot").exists())

    def test_migrate_models(self):
        with tempfile.TemporaryDirectory() as t:
            old = Path(t, "__main__.py", "ocr-models"); old.mkdir(parents=True)
            (old / "m.onnx").write_bytes(b"x")
            new = Path(t, "PinShot")
            M.migrate_legacy_data(Path(t, "__main__.py"), new)
            self.assertTrue((new / "ocr-models" / "m.onnx").exists())
            self.assertFalse(Path(t, "__main__.py").exists())     # emptied legacy dir removed
            M.migrate_legacy_data(Path(t, "__main__.py"), new)   # idempotent


class Translations(unittest.TestCase):
    def tearDown(self):
        i18n.install(app, "en")

    def test_hint_keys_zh(self):
        i18n.install(app, "zh_CN")
        keys = {k for rows in HINTS.values() for k, _ in rows}
        for k in keys:
            self.assertNotRegex(tr(k), r"wheel|Right-click", k)
        self.assertEqual(tr("1 2 / wheel"), "1 2 / 滚轮")
        self.assertEqual(tr("Right-click"), "右键")
        i18n.install(app, "en")
        self.assertEqual(tr("1 2 / wheel"), "1 2 / wheel")

    def test_group_labels(self):
        i18n.install(app, "zh_CN")
        self.assertEqual(group_label("Default"), "默认")
        self.assertEqual(group_label("Group 3"), "分组 3")
        self.assertEqual(group_label("My stuff"), "My stuff")
        i18n.install(app, "en")
        self.assertEqual(group_label("Default"), "Default")
        self.assertEqual(group_label("Group 3"), "Group 3")

    def test_prefs_groups_keep_ids(self):
        from pinshot import settings as S
        from pinshot.prefs import PrefsDialog
        old = S.get("groups")
        try:
            S.set_("groups", ["Default", "Group 1", "Mine"])
            i18n.install(app, "zh_CN")
            d = PrefsDialog()
            shown = [d.groups.item(i).text() for i in range(d.groups.count())]
            self.assertEqual(shown, ["默认", "分组 1", "Mine"])
            d._apply()
            self.assertEqual(S.get("groups"), ["Default", "Group 1", "Mine"])
            d.deleteLater()
        finally:
            S.set_("groups", old)

    def test_ocr_window_title_and_top(self):
        from pinshot.translation import ResultWindow
        i18n.install(app, "zh_CN")
        w = ResultWindow()
        self.assertTrue(w.windowTitle())
        self.assertNotEqual(w.windowTitle(), "PinShot OCR")
        self.assertTrue(w.windowFlags() & Qt.WindowStaysOnTopHint)
        from PySide6.QtGui import QPixmap
        from pinshot.pin_window import PinWindow
        pm = QPixmap(40, 40); pm.fill(Qt.red); pin = PinWindow(pm); pin.show()
        w.show(); app.processEvents()
        w.raise_above_pins()   # must not raise
        pin.close(); w.close()
        i18n.install(app, "en")
        self.assertEqual(ResultWindow().windowTitle(), "PinShot OCR")


if __name__ == "__main__":
    unittest.main()

"""Walk i18n tables and scan live widgets for mixed-language leaks."""
import os
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
if sys.platform != "win32" and not os.environ.get("DISPLAY"):
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtWidgets import QApplication, QWidget
from PySide6.QtCore import QPointF, Qt
from PySide6.QtGui import QImage

from pinshot import i18n
from pinshot.i18n import ALLOWED_ZH_LATIN, STRINGS, table_entries, tr, visible_ui_strings
from pinshot.overlay import HINTS
from pinshot.toolbar import TIPS

CJK = re.compile(r"[\u4e00-\u9fff]")
LATIN = re.compile(r"[A-Za-z]+")
FKEY = re.compile(r"^F([1-9]|1[0-2])$")

app = QApplication.instance() or QApplication([])


def latin_words(text):
    return LATIN.findall(text or "")


def zh_latin_ok(word):
    return len(word) == 1 or word in ALLOWED_ZH_LATIN or bool(FKEY.match(word))


class I18nTables(unittest.TestCase):
    def test_every_key_has_both_languages(self):
        for key, langs in table_entries():
            self.assertIn("en", langs, key)
            self.assertIn("zh", langs, key)
            self.assertTrue(str(langs["en"]).strip(), key)
            self.assertTrue(str(langs["zh"]).strip(), key)

    def test_en_has_no_cjk(self):
        bad = []
        for key, langs in table_entries():
            if CJK.search(langs["en"]):
                bad.append((key, langs["en"]))
        self.assertEqual(bad, [])

    def test_zh_latin_only_allowed_tokens(self):
        bad = []
        for key, langs in table_entries():
            extra = [w for w in latin_words(langs["zh"]) if not zh_latin_ok(w)]
            if extra:
                bad.append((key, langs["zh"], extra))
        self.assertEqual(bad, [])

    def test_hints_and_tips_are_table_keys(self):
        missing = []
        for rows in HINTS.values():
            for _k, text in rows:
                if text not in STRINGS:
                    missing.append(text)
        for text in TIPS.values():
            if text not in STRINGS:
                missing.append(text)
        self.assertEqual(missing, [])

    def test_tr_switches(self):
        i18n.install(app, "en")
        self.assertEqual(tr("Snip"), "Snip")
        i18n.install(app, "zh_CN")
        self.assertEqual(tr("Snip"), "截图")
        self.assertNotRegex(tr("Snip"), r"[A-Za-z]")
        i18n.install(app, "en")
        self.assertEqual(tr("Snip"), "Snip")
        self.assertFalse(CJK.search(tr("Preferences…")))


class I18nRuntimeUI(unittest.TestCase):
    def setUp(self):
        i18n.install(app, "en")

    def tearDown(self):
        i18n.install(app, "en")
        for w in list(app.topLevelWidgets()):
            try:
                if w.objectName() == "i18n_probe" or w.windowTitle() in (
                    "PinShot Preferences", "PinShot 首选项", "PinShot OCR", "PinShot",
                ):
                    w.close()
            except RuntimeError:
                pass

    def _open_surfaces(self):
        from pinshot.prefs import PrefsDialog
        from pinshot.translation import ResultWindow
        from pinshot.toolbar import ToolDock
        from pinshot.annotate import Annotator
        from pinshot.pin_window import PinWindow
        from PySide6.QtGui import QPixmap, QColor

        prefs = PrefsDialog()
        prefs.advanced_btn.setChecked(True)
        prefs.show()
        result = ResultWindow()
        result.show()
        host = QWidget()
        host.setObjectName("i18n_probe")
        image = QImage(80, 60, QImage.Format_RGB32)
        image.fill(Qt.white)
        ann = Annotator(host, lambda: image, QPointF, QPointF)
        dock = ToolDock(ann, parent=host, for_pin=False)
        ann.set_tool("rect")
        host.show()
        dock.show()
        dock.panel.show()
        pm = QPixmap(80, 60)
        pm.fill(QColor("#4488ff"))
        pin = PinWindow(pm)
        pin.show()
        menu = pin.build_menu()
        return prefs, result, host, dock, pin, menu

    def _assert_lang(self, code, widgets):
        i18n.install(app, code)
        i18n.retranslate_all()
        app.processEvents()
        seen = []
        for w in widgets:
            seen.extend(visible_ui_strings(w))
        bad = []
        for kind, text in seen:
            if not text or text.strip() in {"…", "×", "%"}:
                continue
            if code.startswith("zh"):
                extra = [w for w in latin_words(text) if not zh_latin_ok(w)]
                if extra and CJK.search(text) is None and not any(zh_latin_ok(w) for w in latin_words(text)):
                    # purely Latin leftover that should have been translated
                    extra = [w for w in extra if w.lower() not in {"default", "group"}]
                extra = [w for w in extra if w.lower() not in {"default", "group"}]
                if extra:
                    bad.append((kind, text, extra))
            else:
                if CJK.search(text):
                    bad.append((kind, text, ["cjk"]))
        self.assertEqual(bad, [], bad[:12])

    def test_visible_widgets_match_language(self):
        prefs, result, host, dock, pin, menu = self._open_surfaces()
        widgets = [prefs, result, host, dock, dock.panel, pin, menu]
        try:
            self._assert_lang("en", widgets)
            self._assert_lang("zh_CN", widgets)
            self._assert_lang("en", widgets)
        finally:
            menu.close()
            pin.close()
            prefs.close()
            result.close()
            host.close()


if __name__ == "__main__":
    unittest.main()

"""Regression checks for OCR source pixels, async lifecycle and selection clipboard."""
import os
import sys
import unittest
from unittest.mock import patch
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from PySide6.QtWidgets import QApplication
from PySide6.QtCore import QRect, QPoint, QPointF, Qt, QMimeData, QEvent
from PySide6.QtGui import QImage, QPixmap, QColor, QMouseEvent
from pinshot.translation import TranslationController, read_translation
from pinshot.selection_text import SelectionReader
from pinshot.overlay import SnipOverlay

app = QApplication.instance() or QApplication([])


class TranslationTests(unittest.TestCase):
    def setUp(self):
        self.clipboard_backup = QMimeData()
        current = app.clipboard().mimeData()
        if current:
            for fmt in current.formats(): self.clipboard_backup.setData(fmt, current.data(fmt))
            if current.hasImage(): self.clipboard_backup.setImageData(current.imageData())

    def tearDown(self):
        app.clipboard().setMimeData(self.clipboard_backup)

    def test_translation_unicode_and_empty_response(self):
        self.assertEqual(read_translation('[{"translations":[{"text":"你好"}]}]'), '你好')
        with self.assertRaises(ValueError): read_translation('[]')

    def test_closed_window_ignores_worker_completion(self):
        controller = TranslationController(); window = controller.new_window()
        window.request = 1; window.close()
        controller._completed(window, 1, 'ocr', 'late text', '', True)
        self.assertEqual(window.original.toPlainText(), '')

    def test_stale_completion_does_not_replace_new_text(self):
        controller = TranslationController(); window = controller.new_window()
        window.request = 2; window.busy = True; window.original.setPlainText('new text')
        controller._completed(window, 1, 'ocr', 'old text', '', False)
        self.assertEqual(window.original.toPlainText(), 'new text')
        self.assertTrue(window.busy); window.close()

    def test_translation_failure_keeps_original_for_retry(self):
        controller = TranslationController(); window = controller.new_window()
        window.original.setPlainText('Hello'); window.request = 1
        controller._completed(window, 1, 'translate', '', 'timeout', False)
        self.assertEqual(window.original.toPlainText(), 'Hello')
        self.assertTrue(window.translate_button.isEnabled())
        self.assertFalse(window.copy_translated.isEnabled()); window.close()

    def test_ocr_uses_frozen_source_without_annotations_at_high_dpi(self):
        image = QImage(400, 200, QImage.Format_RGB32); image.fill(QColor('white'))
        overlay = SnipOverlay(QPixmap.fromImage(image), QRect(0, 0, 200, 100))
        overlay.sel = QRect(10, 10, 30, 20)
        overlay.ann.set_tool('rect')
        overlay.ann.press(QPointF(10, 10), Qt.LeftButton)
        overlay.ann.move(QPointF(40, 30)); overlay.ann.release(QPointF(40, 30))
        results = []; overlay.text_requested.connect(lambda im, auto: results.append((im, auto)))
        overlay._action('ocr')
        self.assertEqual((results[0][0].width(), results[0][0].height()), (60, 40))
        self.assertEqual(results[0][0].pixelColor(0, 0), QColor('white'))
        self.assertFalse(results[0][1])

    def test_selection_uses_release_position_after_missed_mouse_move(self):
        image = QImage(400, 200, QImage.Format_RGB32); image.fill(QColor('white'))
        overlay = SnipOverlay(QPixmap.fromImage(image), QRect(0, 0, 400, 200))
        overlay.state = 'creating'; overlay.press_pt = QPoint(10, 10)
        overlay.sel = QRect(10, 10, 5, 5)
        event = QMouseEvent(QEvent.MouseButtonRelease, QPointF(100, 50), QPointF(100, 50),
                            Qt.LeftButton, Qt.NoButton, Qt.NoModifier)
        overlay.mouseReleaseEvent(event)
        self.assertEqual(overlay.sel, QRect(10, 10, 90, 40)); overlay.close()

    @unittest.skipUnless(sys.platform == 'win32', 'Windows selection reader')
    def test_clipboard_restored_after_copy_failure(self):
        clipboard = app.clipboard(); clipboard.setText('clipboard backup')
        results = []; reader = SelectionReader()
        with patch('pinshot.selection_text.send_copy', side_effect=RuntimeError('injection failed')):
            reader.read(lambda text, error: results.append((text, error)))
        self.assertEqual(clipboard.text(), 'clipboard backup')
        self.assertFalse(reader.active)
        self.assertEqual(results, [('', 'injection failed')])

    @unittest.skipUnless(sys.platform == 'win32', 'Windows selection reader')
    def test_selection_text_returned_and_clipboard_restored(self):
        clipboard = app.clipboard(); clipboard.setText('clipboard backup')
        results = []; reader = SelectionReader()
        with patch('pinshot.selection_text.send_copy', side_effect=lambda: clipboard.setText('selected text')):
            reader.read(lambda text, error: results.append((text, error)))
        reader._poll()
        self.assertEqual(results, [('selected text', '')])
        self.assertEqual(clipboard.text(), 'clipboard backup')


if __name__ == '__main__': unittest.main()

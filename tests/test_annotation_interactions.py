"""Regression coverage for screenshot annotation editing."""
import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from PySide6.QtWidgets import QApplication, QWidget
from PySide6.QtCore import QPointF, Qt
from PySide6.QtGui import QImage
from pinshot.annotate import Annotator, Shape, Style

app = QApplication.instance() or QApplication([])


class AnnotationInteractions(unittest.TestCase):
    def setUp(self):
        self.widget = QWidget()
        self.image = QImage(600, 400, QImage.Format_RGB32)
        self.image.fill(Qt.white)
        self.ann = Annotator(self.widget, lambda: self.image, QPointF, QPointF)
        self.ann.set_tool('rect')
        self.rect = Shape('rect', [QPointF(40, 40), QPointF(200, 160)], Style())
        self.ann.add(self.rect)

    def test_hollow_hit(self):
        self.assertIsNone(self.ann.hit_test(QPointF(100, 100)))
        self.assertIs(self.ann.hit_test(QPointF(40, 80)), self.rect)
        self.rect.style.filled = True
        self.assertIs(self.ann.hit_test(QPointF(100, 100)), self.rect)

    def test_drag_with_drawing_tool(self):
        self.ann.hover(QPointF(40, 80))
        self.ann.press(QPointF(40, 80), Qt.LeftButton)
        self.ann.move(QPointF(70, 100))
        self.ann.release(QPointF(70, 100))
        self.assertEqual(len(self.ann.shapes), 1)
        self.assertEqual(self.rect.pts[0], QPointF(70, 60))
        self.ann.undo()
        self.assertEqual(self.ann.shapes[0].pts[0], QPointF(40, 40))
        self.ann.redo()
        self.assertEqual(self.ann.shapes[0].pts[0], QPointF(70, 60))

    def test_width_after_release(self):
        self.ann.adjust_width(2)
        self.assertEqual(self.rect.style.width, 5)
        self.assertEqual(self.ann.styles['rect'].width, 5)
        self.ann.undo()
        self.assertEqual(self.ann.shapes[0].style.width, 3)
        self.ann.undo()
        self.assertEqual(self.ann.shapes, [])

    def test_drag_without_tool(self):
        self.ann.set_tool(None)
        self.ann.hover(QPointF(40, 80))
        self.assertIs(self.ann.selected, self.rect)
        self.assertTrue(self.ann.press(QPointF(40, 80), Qt.LeftButton))
        self.ann.move(QPointF(60, 80))
        self.ann.release(QPointF(60, 80))
        self.assertEqual(self.rect.pts[0], QPointF(60, 40))

    def test_effect_tools_do_not_drag_annotations(self):
        for kind in ('eraser', 'mosaic'):
            self.ann.set_tool(kind)
            self.ann.hover(QPointF(40, 80))
            self.assertIsNone(self.ann.selected)
            self.ann.press(QPointF(40, 80), Qt.LeftButton)
            self.assertEqual(self.ann.live.kind, kind)
            self.assertIsNone(self.ann._sel_mode)
            self.ann.finish()

    def test_existing_style_edit_and_undo(self):
        self.ann.set_style_value('rect', 'filled', True)
        self.assertTrue(self.rect.style.filled)
        self.ann.undo()
        self.assertFalse(self.ann.shapes[0].style.filled)

    def test_erased_pixels_do_not_select_hidden_shape(self):
        self.rect.style.filled = True
        self.ann.add(Shape('eraser', [QPointF(40, 80), QPointF(200, 80)], Style(width=16, mode='brush')))
        self.assertIsNone(self.ann.hit_test(QPointF(100, 80)))
        self.assertIs(self.ann.hit_test(QPointF(100, 110)), self.rect)

    def test_ellipse_and_line_content_hit(self):
        self.rect.kind = 'ellipse'
        self.assertIsNone(self.ann.hit_test(QPointF(40, 40)))
        self.assertIsNone(self.ann.hit_test(QPointF(100, 100)))
        self.assertIs(self.ann.hit_test(QPointF(40, 100)), self.rect)
        self.rect.kind = 'line'
        self.assertIsNone(self.ann.hit_test(QPointF(50, 140)))
        self.assertIs(self.ann.hit_test(QPointF(120, 100)), self.rect)


if __name__ == '__main__': unittest.main()

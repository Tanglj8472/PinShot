"""Real local-model inference on a padded RGB image with English and Chinese."""
import os
import sys
import unittest
from pathlib import Path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from PySide6.QtWidgets import QApplication
from PySide6.QtGui import QImage, QPainter, QFont, QColor
from pinshot.translation import recognize

app = QApplication.instance() or QApplication([])


@unittest.skipUnless((Path(__file__).resolve().parents[1] / 'assets/ocr-models/ch_PP-OCRv5_rec_mobile.onnx').exists(),
                     'Run tools/prepare_ocr_models.py first')
class OcrSmoke(unittest.TestCase):
    def test_bilingual_local_inference(self):
        image = QImage(601, 200, QImage.Format_RGB32); image.fill(QColor('white'))
        painter = QPainter(image); painter.setPen(QColor('black'))
        painter.setFont(QFont('Microsoft YaHei', 28))
        painter.drawText(20, 60, 'Hello world')
        painter.drawText(20, 130, '屏幕文字识别测试'); painter.end()
        text = recognize(image)
        self.assertIn('hello', text.lower())
        self.assertIn('文字', text)

    def test_small_bilingual_region(self):
        image = QImage(283, 93, QImage.Format_RGB32); image.fill(QColor('white'))
        painter = QPainter(image); painter.setPen(QColor('black'))
        painter.setFont(QFont('Microsoft YaHei', 24))
        painter.drawText(5, 37, 'Hello world')
        painter.drawText(5, 80, '屏幕文字识别测试'); painter.end()
        text = recognize(image)
        self.assertIn('hello', text.lower())
        self.assertIn('文字', text)


if __name__ == '__main__': unittest.main()

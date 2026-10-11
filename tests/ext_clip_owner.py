"""Separate process that owns the X11 CLIPBOARD with ONLY image/png (like Chrome 'Copy image')."""
import sys, os
from PySide6.QtWidgets import QApplication
from PySide6.QtCore import QMimeData, QByteArray, QBuffer, QTimer
from PySide6.QtGui import QImage, QColor
app = QApplication(sys.argv)
img = QImage(int(sys.argv[1]), int(sys.argv[2]), QImage.Format_RGB32); img.fill(QColor(0, 170, 80))
if len(sys.argv) > 4:
    import random
    for y in range(img.height()):
        for x in range(0, img.width(), 3): img.setPixel(x, y, random.getrandbits(24))
buf = QBuffer(); buf.open(QBuffer.WriteOnly); img.save(buf, "PNG")
md = QMimeData(); md.setData("image/png", QByteArray(bytes(buf.data())))
app.clipboard().setMimeData(md); print("owner ready", flush=True)
QTimer.singleShot(int(sys.argv[3]) * 1000, app.quit); app.exec(); os._exit(0)

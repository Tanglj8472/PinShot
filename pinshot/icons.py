"""PinShot's own toolbar icons, drawn with QPainter (no third-party assets)."""
import math
from PySide6.QtCore import Qt, QPointF, QRectF
from PySide6.QtGui import QIcon, QPixmap, QPainter, QPen, QColor, QPolygonF, QFont, QPainterPath

FG = QColor(50, 50, 50)


def _arrow(p, a, b, head=5):
    p.drawLine(a, b)
    ang = math.atan2(b.y() - a.y(), b.x() - a.x())
    pts = [b, b - QPointF(head * math.cos(ang - .5), head * math.sin(ang - .5)),
           b - QPointF(head * math.cos(ang + .5), head * math.sin(ang + .5))]
    p.save(); p.setBrush(p.pen().color()); p.drawPolygon(QPolygonF(pts)); p.restore()


def icon(name: str, color: QColor = FG, size: int = 20) -> QIcon:
    pm = QPixmap(size * 2, size * 2); pm.fill(Qt.transparent); pm.setDevicePixelRatio(2)
    p = QPainter(pm); p.setRenderHint(QPainter.Antialiasing)
    pen = QPen(color, 1.6, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin); p.setPen(pen); p.setBrush(Qt.NoBrush)
    s = size
    if name == "rect": p.drawRect(QRectF(3.5, 5.5, 13, 9))
    elif name == "ellipse": p.drawEllipse(QRectF(3, 5, 14, 10))
    elif name == "line": _arrow(p, QPointF(4, 16), QPointF(16, 4))
    elif name == "pen":
        path = QPainterPath(QPointF(3, 14)); path.cubicTo(6, 4, 10, 18, 17, 5); p.drawPath(path)
    elif name == "marker":
        p.setPen(QPen(QColor(color.red(), color.green(), color.blue(), 110), 6, Qt.SolidLine, Qt.FlatCap))
        p.drawLine(QPointF(3, 12), QPointF(17, 12)); p.setPen(pen); p.drawLine(QPointF(5, 6), QPointF(14, 6))
    elif name == "mosaic":
        for i in range(3):
            for j in range(3):
                if (i + j) % 2 == 0: p.fillRect(QRectF(4 + i * 4, 4 + j * 4, 4, 4), color)
        p.drawRect(QRectF(4, 4, 12, 12))
    elif name == "text":
        f = QFont(); f.setPixelSize(15); f.setBold(True); p.setFont(f); p.drawText(QRectF(0, 0, s, s), Qt.AlignCenter, "T")
    elif name == "eraser":
        poly = QPolygonF([QPointF(4, 13), QPointF(11, 5), QPointF(16, 10), QPointF(10, 16), QPointF(6, 16)])
        p.drawPolygon(poly); p.drawLine(QPointF(7.5, 9), QPointF(13, 14)); p.drawLine(QPointF(10, 16), QPointF(17, 16))
    elif name in ("undo", "redo"):
        path = QPainterPath(); path.arcMoveTo(QRectF(4, 5, 12, 11), 160); path.arcTo(QRectF(4, 5, 12, 11), 160, -200)
        if name == "redo": p.translate(s, 0); p.scale(-1, 1)
        p.drawPath(path); _arrow(p, QPointF(5, 8), QPointF(4.2, 10.5), 4)
    elif name == "close":
        p.setPen(QPen(QColor(210, 50, 50), 1.8)); p.drawLine(QPointF(5, 5), QPointF(15, 15)); p.drawLine(QPointF(15, 5), QPointF(5, 15))
    elif name == "pin":
        p.drawLine(QPointF(10, 12), QPointF(10, 18)); p.drawRoundedRect(QRectF(6, 3, 8, 7), 2, 2); p.drawLine(QPointF(4, 11), QPointF(16, 11))
    elif name == "save":
        p.drawLine(QPointF(10, 3), QPointF(10, 12)); _arrow(p, QPointF(10, 3), QPointF(10, 13), 4)
        p.drawPolyline(QPolygonF([QPointF(4, 12), QPointF(4, 16), QPointF(16, 16), QPointF(16, 12)]))
    elif name == "copy":
        p.drawRoundedRect(QRectF(7, 3, 9, 11), 1.5, 1.5); p.drawPolyline(QPolygonF([QPointF(5, 6), QPointF(4, 6), QPointF(4, 17), QPointF(12, 17), QPointF(12, 16)]))
    elif name == "check":
        p.setPen(QPen(QColor(30, 150, 60), 2.2)); p.drawPolyline(QPolygonF([QPointF(4, 10), QPointF(8.5, 14.5), QPointF(16, 5)]))
    elif name == "select":
        p.drawRect(QRectF(4, 4, 10, 10)); p.drawLine(QPointF(14, 14), QPointF(17, 17))
    elif name == "number":
        p.setBrush(color); p.setPen(Qt.NoPen); p.drawEllipse(QRectF(3, 3, 14, 14))
        p.setPen(QColor(20, 20, 20) if color.lightness() > 140 else Qt.white); f = QFont(); f.setPixelSize(10); f.setBold(True); p.setFont(f)
        p.drawText(QRectF(3, 3, 14, 14), Qt.AlignCenter, "1")
    elif name == "app":
        p.setPen(Qt.NoPen); p.setBrush(QColor(30, 144, 255)); p.drawRoundedRect(QRectF(1, 1, 18, 18), 4, 4)
        p.setBrush(Qt.white); p.drawRect(QRectF(5, 5, 10, 7)); p.drawRect(QRectF(9, 12, 2, 4))
    p.end()
    return QIcon(pm)


def app_image(size: int = 256):
    """PinShot app icon (our own drawing): blue rounded tile, white crop corners + pin dot."""
    from PySide6.QtGui import QImage
    img = QImage(size, size, QImage.Format_ARGB32); img.fill(Qt.transparent)
    p = QPainter(img); p.setRenderHint(QPainter.Antialiasing)
    s = size / 32.0
    p.setPen(Qt.NoPen); p.setBrush(QColor(30, 144, 255)); p.drawRoundedRect(QRectF(1 * s, 1 * s, 30 * s, 30 * s), 7 * s, 7 * s)
    pen = QPen(Qt.white, max(1.0, 2.6 * s), Qt.SolidLine, Qt.SquareCap, Qt.MiterJoin); p.setPen(pen); p.setBrush(Qt.NoBrush)
    a, b, L = 7 * s, 25 * s, 6 * s
    for (x, y, dx, dy) in ((a, a, 1, 1), (b, a, -1, 1), (a, b, 1, -1), (b, b, -1, -1)):
        p.drawLine(QPointF(x, y), QPointF(x + dx * L, y)); p.drawLine(QPointF(x, y), QPointF(x, y + dy * L))
    p.setPen(Qt.NoPen); p.setBrush(QColor(255, 212, 0)); p.drawEllipse(QPointF(16 * s, 16 * s), 3.6 * s, 3.6 * s)
    p.end()
    return img

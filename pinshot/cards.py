"""Render clipboard text as a pin image: plain text card or color card."""
import colorsys, re
from PySide6.QtCore import Qt, QRect, QRectF
from PySide6.QtGui import QPixmap, QPainter, QColor, QFont, QFontMetrics, QPen

_HEX = re.compile(r"^\s*#?([0-9a-fA-F]{6}|[0-9a-fA-F]{3})\s*$")
_RGB = re.compile(r"^\s*(?:rgba?\s*\()?\s*(\d{1,3})\s*[, ]\s*(\d{1,3})\s*[, ]\s*(\d{1,3})\s*(?:[, ]\s*[\d.]+\s*)?\)?\s*$", re.I)


def parse_color(text: str):
    if not text or len(text) > 40: return None
    m = _HEX.match(text)
    if m and (text.strip().startswith("#") or len(m.group(1)) == 6):
        h = m.group(1)
        if len(h) == 3: h = "".join(c * 2 for c in h)
        return QColor("#" + h)
    m = _RGB.match(text)
    if m:
        v = [int(x) for x in m.groups()]
        if all(0 <= x <= 255 for x in v): return QColor(*v)
    return None


def color_lines(c: QColor):
    r, g, b = c.red(), c.green(), c.blue(); rf, gf, bf = r / 255, g / 255, b / 255
    h, s, v = colorsys.rgb_to_hsv(rf, gf, bf); hl, l, sl = colorsys.rgb_to_hls(rf, gf, bf)
    i = (rf + gf + bf) / 3; si = 0 if i == 0 else 1 - min(rf, gf, bf) / i
    return [("RGB", f"{r}, {g}, {b}"), ("RGB float", f"{rf:.3f}, {gf:.3f}, {bf:.3f}"),
            ("HSV", f"{h * 360:.0f}°, {s * 100:.0f}%, {v * 100:.0f}%"), ("HSL", f"{hl * 360:.0f}°, {sl * 100:.0f}%, {l * 100:.0f}%"),
            ("HSI", f"{h * 360:.0f}°, {si * 100:.0f}%, {i * 100:.0f}%"), ("HEX", c.name().upper())]


def _tr(s):
    from .i18n import tr
    return tr(s)


def color_card(c: QColor) -> QPixmap:
    rows = color_lines(c); W, sw, lh = 300, 96, 22
    H = max(sw + 20, 16 + lh * len(rows))
    pm = QPixmap(W, H); pm.fill(QColor("#ffffff"))
    p = QPainter(pm); p.setRenderHint(QPainter.Antialiasing)
    p.setPen(QPen(QColor("#999"), 1)); p.setBrush(c); p.drawRoundedRect(QRectF(10, 10, sw, H - 20), 4, 4)
    f = QFont(); f.setPixelSize(12)
    for i, (k, v) in enumerate(rows):
        y = 10 + i * lh
        f.setBold(True); p.setFont(f); p.setPen(QColor("#555")); p.drawText(QRect(sw + 22, y, 70, lh), Qt.AlignVCenter, _tr(k))
        f.setBold(False); p.setFont(f); p.setPen(QColor("#111")); p.drawText(QRect(sw + 92, y, W - sw - 96, lh), Qt.AlignVCenter, v)
    p.end(); return pm


def text_card(text: str, max_w=640) -> QPixmap:
    f = QFont(); f.setPixelSize(14); fm = QFontMetrics(f)
    text = text.replace("\t", "    ")[:20000]
    br = fm.boundingRect(QRect(0, 0, max_w, 100000), Qt.TextWordWrap | Qt.AlignLeft, text)
    W, H = min(max_w, br.width()) + 24, min(1200, br.height()) + 20
    pm = QPixmap(W, H); pm.fill(QColor("#fffef5"))
    p = QPainter(pm); p.setFont(f); p.setPen(QColor("#222"))
    p.drawText(QRect(12, 10, W - 24, H - 20), Qt.TextWordWrap | Qt.AlignLeft, text)
    p.setPen(QPen(QColor("#d8d2b0"), 1)); p.drawRect(0, 0, W - 1, H - 1); p.end(); return pm

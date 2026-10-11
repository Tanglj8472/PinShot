"""Generate assets/pinshot.ico (+ pinshot.png) from PinShot's own QPainter icon (no third-party art).
ICO is written by hand with PNG-compressed entries (Vista+ format), so no Pillow/Qt ICO plugin is needed."""
import os, struct, sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
from PySide6.QtCore import QBuffer, QByteArray, QIODevice
from PySide6.QtGui import QGuiApplication
from pinshot.icons import app_image

SIZES = (16, 24, 32, 48, 64, 128, 256)


def png_bytes(img):
    ba = QByteArray(); buf = QBuffer(ba); buf.open(QIODevice.WriteOnly); img.save(buf, "PNG"); buf.close()
    return bytes(ba)


def write_ico(path, sizes=SIZES):
    blobs = [png_bytes(app_image(n)) for n in sizes]
    header = struct.pack("<HHH", 0, 1, len(sizes))
    off = 6 + 16 * len(sizes); entries = b""
    for n, data in zip(sizes, blobs):
        d = 0 if n >= 256 else n
        entries += struct.pack("<BBBBHHII", d, d, 0, 0, 1, 32, len(data), off); off += len(data)
    with open(path, "wb") as f:
        f.write(header + entries + b"".join(blobs))


if __name__ == "__main__":
    app = QGuiApplication(sys.argv[:1])
    out = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "assets")
    os.makedirs(out, exist_ok=True)
    write_ico(os.path.join(out, "pinshot.ico"))
    app_image(256).save(os.path.join(out, "pinshot.png"))
    print("wrote", os.path.join(out, "pinshot.ico"))

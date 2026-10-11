"""Clipboard helpers. Reads images from imageData(), image/* bytes, file URLs, or <img> in HTML."""
import base64, re, urllib.request
from PySide6.QtCore import QUrl
from PySide6.QtGui import QGuiApplication, QPixmap, QImage

IMAGE_FORMATS = ["image/png", "image/jpeg", "image/bmp", "image/gif", "image/webp", "image/x-qt-image"]


def copy_pixmap(pm: QPixmap) -> None:
    QGuiApplication.clipboard().setPixmap(pm)


def _from_bytes(data) -> QPixmap:
    img = QImage()
    if data and img.loadFromData(bytes(data)):
        return QPixmap.fromImage(img.copy())
    return None


def log(msg):
    print("[pinshot]", msg, flush=True)


def clipboard_pixmap():
    """Read the clipboard fresh at call time (no caching).
    If another app owns it on X11, ask the X server directly first (Chrome serves image/png, often via INCR)."""
    cb = QGuiApplication.clipboard()
    owns = cb.ownsClipboard()
    if not owns:
        from .platform import current as _plat
        r = _plat().clipboard_read()
        if r:
            pm = _from_bytes(r[1])
            if pm: return pm
    md = cb.mimeData()
    if md is None:
        log("clipboard: no mime data"); return None
    log(f"clipboard: qt owns={owns} formats={md.formats()}")
    return _from_mime(md)


def _from_mime(md):
    if md.hasImage():
        v = md.imageData()
        img = v if isinstance(v, QImage) else (v.toImage() if isinstance(v, QPixmap) else None)
        if img is not None and not img.isNull():
            return QPixmap.fromImage(img.copy())
    for f in IMAGE_FORMATS + [f for f in md.formats() if f.startswith("image/")]:
        if md.hasFormat(f):
            pm = _from_bytes(md.data(f))
            if pm: return pm
    for u in md.urls() if md.hasUrls() else []:
        if u.isLocalFile():
            pm = QPixmap(u.toLocalFile())
            if not pm.isNull(): return pm
    if md.hasHtml():
        m = re.search(r'<img[^>]+src="([^"]+)"', md.html(), re.I)
        if m:
            src = m.group(1).replace("&amp;", "&")
            if src.startswith("data:image") and "," in src:
                return _from_bytes(base64.b64decode(src.split(",", 1)[1]))
            if src.startswith(("http://", "https://")):
                try:
                    with urllib.request.urlopen(src, timeout=5) as r:
                        return _from_bytes(r.read(20_000_000))
                except Exception:
                    return None
    return None


TEXT_TARGETS = ["UTF8_STRING", "text/plain;charset=utf-8", "STRING", "TEXT", "text/plain"]


def clipboard_text():
    """Fresh text read (X server first when another app owns the clipboard)."""
    cb = QGuiApplication.clipboard()
    if not cb.ownsClipboard():
        from .platform import current as _plat
        r = _plat().clipboard_read(prefer=TEXT_TARGETS)
        if r:
            try: return r[1].decode("utf-8")
            except UnicodeDecodeError: return r[1].decode("latin-1")
    md = cb.mimeData()
    return md.text() if md is not None and md.hasText() else ""

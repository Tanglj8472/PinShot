"""Pin groups + recoverable closed pins (Close vs Destroy)."""
from PySide6.QtCore import QPoint, QByteArray, QBuffer, QIODevice
from PySide6.QtGui import QImage, QPixmap
from .settings import get, set_, settings

from PySide6.QtCore import QObject, Signal


class _Bus(QObject):
    groups_changed = Signal()


BUS = _Bus()
CLOSED = []   # list of dicts: {image: QImage, pos: (x,y), scale, opacity, group, shadow}
ALL_GROUPS_VIS = {}  # group -> bool (default True)


def _dedupe(names):
    out, seen = [], set()
    for n in names:
        n = str(n).strip()
        if n and n.lower() not in seen: seen.add(n.lower()); out.append(n)
    if "default" not in seen: out.insert(0, "Default")
    return out


def group_names():
    raw = list(get("groups")); g = _dedupe(raw)
    if g != raw: set_("groups", g)       # heal stored duplicates
    return g


def find_group(name):
    for g in group_names():
        if g.lower() == (name or "").strip().lower(): return g
    return None


def ensure_group(name: str):
    name = (name or "").strip()
    if not name: return "Default"
    existing = find_group(name)
    if existing: return existing          # no duplicates (case-insensitive)
    set_("groups", group_names() + [name]); BUS.groups_changed.emit()
    return name


def rename_group(old: str, new: str):
    new = (new or "").strip()
    if not new or old == "Default": return False
    clash = find_group(new)
    if clash and clash.lower() != old.lower(): return False   # would duplicate
    g = group_names()
    if old in g:
        g[g.index(old)] = new; set_("groups", g)
    from .pin_window import ALL_PINS
    for p in ALL_PINS:
        if p.group == old: p.group = new
    if old in ALL_GROUPS_VIS:
        ALL_GROUPS_VIS[new] = ALL_GROUPS_VIS.pop(old)
    BUS.groups_changed.emit(); return True


def remove_group(name: str):
    if name == "Default": return
    g = [x for x in group_names() if x != name]
    if not g: g = ["Default"]
    set_("groups", g)
    from .pin_window import ALL_PINS
    for p in ALL_PINS:
        if p.group == name: p.group = "Default"
    ALL_GROUPS_VIS.pop(name, None); BUS.groups_changed.emit()


def set_group_visible(name: str, visible: bool):
    ALL_GROUPS_VIS[name] = visible
    from .pin_window import ALL_PINS
    for p in ALL_PINS:
        if p.group == name: p.setVisible(visible)
    BUS.groups_changed.emit()


def group_visible(name: str) -> bool:
    return ALL_GROUPS_VIS.get(name, True)


def push_closed(pin):
    """Snapshot a pin closed via Close (not Destroy)."""
    n = get("closed_pins_max", int)
    if n <= 0: return
    img = pin.ann.composed()
    CLOSED.append({
        "image": img.copy(),
        "pos": (pin.x(), pin.y()),
        "scale": pin.scale,
        "opacity": pin.windowOpacity(),
        "group": pin.group,
        "shadow": pin.shadow,
        "thumb": pin.thumb,
    })
    del CLOSED[:-n]


def restore_last_closed():
    if not CLOSED: return None
    from .pin_window import PinWindow
    data = CLOSED.pop()
    pm = QPixmap.fromImage(data["image"])
    w = PinWindow(pm, QPoint(data["pos"][0], data["pos"][1]))
    w.group = ensure_group(data.get("group", "Default"))
    w.scale = data.get("scale", 1.0); w.shadow = data.get("shadow", w.shadow)
    w.thumb = data.get("thumb", False); w.setWindowOpacity(data.get("opacity", 1.0))
    w._resize(); w.show(); w.raise_()
    if not group_visible(w.group): w.hide()
    return w


def persist_session():
    """Save open pins + groups to QSettings (lightweight PNG blobs)."""
    from .pin_window import ALL_PINS
    s = settings()
    s.setValue("session/groups", group_names())
    s.beginWriteArray("session/pins")
    i = 0
    for p in ALL_PINS:
        s.setArrayIndex(i)
        buf = QBuffer(); buf.open(QIODevice.WriteOnly)
        p.ann.composed().save(buf, "PNG")
        s.setValue("png", buf.data())
        s.setValue("x", p.x()); s.setValue("y", p.y())
        s.setValue("scale", p.scale); s.setValue("opacity", p.windowOpacity())
        s.setValue("group", p.group); s.setValue("shadow", p.shadow)
        i += 1
    s.endArray()
    s.setValue("session/count", i)


def restore_session():
    from .pin_window import PinWindow
    s = settings()
    g = s.value("session/groups")
    if isinstance(g, list) and g: set_("groups", [str(x) for x in g])
    n = int(s.value("session/count", 0))
    s.beginReadArray("session/pins")
    out = []
    for i in range(n):
        s.setArrayIndex(i)
        data = s.value("png")
        if not data: continue
        img = QImage(); img.loadFromData(bytes(data) if not isinstance(data, (bytes, bytearray, QByteArray)) else bytes(data))
        if img.isNull(): continue
        w = PinWindow(QPixmap.fromImage(img), QPoint(int(s.value("x", 100)), int(s.value("y", 100))))
        w.group = ensure_group(str(s.value("group", "Default")))
        w.scale = float(s.value("scale", 1.0)); w.setWindowOpacity(float(s.value("opacity", 1.0)))
        w.shadow = bool(s.value("shadow", True)); w._resize(); w.show()
        out.append(w)
    s.endArray()
    return out

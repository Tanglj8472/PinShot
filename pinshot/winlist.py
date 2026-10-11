"""Window and element rectangles for snip hover/click (X11 EWMH + child windows; AT-SPI optional)."""
from PySide6.QtCore import QRect


_PROBE = r"""
import gi; gi.require_version('Atspi','2.0'); from gi.repository import Atspi
Atspi.init(); d=Atspi.get_desktop(0); print('ATSPI_OK', d.get_child_count())
"""


def atspi_status():
    """(ok, detail). Probed in a subprocess: Atspi.init() aborts the process (g_error) when the
    accessibility bus is unreachable, so it must never be the first call in PinShot itself."""
    import subprocess, sys
    try:
        r = subprocess.run([sys.executable, "-c", _PROBE], capture_output=True, text=True, timeout=8)
    except Exception as e:
        return False, f"probe failed: {e}"
    for line in r.stdout.splitlines():
        if line.startswith("ATSPI_OK"):
            n = int(line.split()[1])
            return n >= 0, f"a11y bus ok, {n} accessible apps"
    err = (r.stderr.strip().splitlines() or ["no output"])[-1]
    return False, err[:200]


ATSPI_AVAILABLE = None
ATSPI_DETAIL = ""


def atspi_available():
    global ATSPI_AVAILABLE, ATSPI_DETAIL
    if ATSPI_AVAILABLE is None:
        ATSPI_AVAILABLE, ATSPI_DETAIL = atspi_status()
    return ATSPI_AVAILABLE


def window_rects(exclude_pids=()):
    """Return [(QRect, title)] topmost first, in root coordinates."""
    try:
        from Xlib import display, X
    except Exception:
        return []
    out = []
    try:
        d = display.Display(); root = d.screen().root
        A = d.intern_atom
        prop = root.get_full_property(A("_NET_CLIENT_LIST_STACKING"), X.AnyPropertyType) or \
            root.get_full_property(A("_NET_CLIENT_LIST"), X.AnyPropertyType)
        for wid in reversed(list(prop.value) if prop else []):
            w = d.create_resource_object("window", wid)
            try:
                pid = w.get_full_property(A("_NET_WM_PID"), X.AnyPropertyType)
                if pid and pid.value[0] in exclude_pids: continue
                st = w.get_full_property(A("_NET_WM_STATE"), X.AnyPropertyType)
                if st and A("_NET_WM_STATE_HIDDEN") in st.value: continue
                g = w.get_geometry(); t = w.translate_coords(root, 0, 0)
                name = w.get_full_property(A("_NET_WM_NAME"), X.AnyPropertyType)
                title = name.value.decode("utf-8", "replace") if name else ""
                ext = w.get_full_property(A("_NET_FRAME_EXTENTS"), X.AnyPropertyType)
                l, r, tp, b = (list(ext.value) + [0] * 4)[:4] if ext else (0, 0, 0, 0)
                out.append((QRect(-t.x - l, -t.y - tp, g.width + l + r, g.height + tp + b), title))
            except Exception:
                continue
        d.close()
    except Exception:
        return []
    return out


LAST_SOURCE = "X11 children"


def element_rects(exclude_pids=(), max_depth=4, min_size=8):
    """Finer rectangles for element mode.

    Prefer AT-SPI2 when available; otherwise walk mapped X11 child windows via XQueryTree
    (depth-limited) so hover can snap to buttons/panels inside a client.
    Returns [(QRect, title)] smallest/most-specific first among hits at a point — caller filters.
    Listed topmost-ish (children after parents in traversal order reversed for hit testing).
    """
    global LAST_SOURCE
    x11 = _x11_child_rects(exclude_pids=exclude_pids, max_depth=max_depth, min_size=min_size)
    if atspi_available():
        try:
            acc = _atspi_rects(exclude_pids=exclude_pids)
            if acc:
                LAST_SOURCE = "AT-SPI"
                # AT-SPI first (finer); X11 children still fill apps without a11y (e.g. Chrome
                # started before the a11y bus). hover_rect picks the smallest containing rect.
                return acc + x11
        except Exception:
            pass
    LAST_SOURCE = "X11 children"
    return x11


SKIP_ROLES = {"application", "redundant object", "unknown", "invalid"}


def _atspi_rects(exclude_pids=(), budget_s=0.6, max_depth=12):
    """Accessible bounds (screen coords) of showing objects, via AT-SPI2. Time-budgeted."""
    import time
    import gi
    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi
    Atspi.init()
    t_end = time.monotonic() + budget_s
    out = []
    SHOWING = Atspi.StateType.SHOWING

    def walk(acc, depth):
        if acc is None or depth > max_depth or time.monotonic() > t_end: return
        try:
            role = acc.get_role_name()
            if depth > 0 and not acc.get_state_set().contains(SHOWING): return
            if role not in SKIP_ROLES:
                comp = acc.get_component_iface()
                if comp:
                    e = comp.get_extents(Atspi.CoordType.SCREEN)
                    if e.width > 3 and e.height > 3 and e.width < 20000:
                        out.append((QRect(e.x, e.y, e.width, e.height), f"{role}:{acc.get_name() or ''}"))
            for i in range(min(acc.get_child_count(), 400)):
                walk(acc.get_child_at_index(i), depth + 1)
        except Exception:
            return

    desk = Atspi.get_desktop(0)
    for i in range(max(0, desk.get_child_count())):
        app = desk.get_child_at_index(i)
        try:
            if app.get_process_id() in exclude_pids: continue
        except Exception:
            pass
        walk(app, 0)
    return out


def _x11_child_rects(exclude_pids=(), max_depth=4, min_size=8):
    try:
        from Xlib import display, X
    except Exception:
        return []
    out = []
    try:
        d = display.Display(); root = d.screen().root
        A = d.intern_atom
        prop = root.get_full_property(A("_NET_CLIENT_LIST_STACKING"), X.AnyPropertyType) or \
            root.get_full_property(A("_NET_CLIENT_LIST"), X.AnyPropertyType)
        clients = list(prop.value) if prop else []
        for wid in reversed(clients):
            w = d.create_resource_object("window", wid)
            try:
                pid = w.get_full_property(A("_NET_WM_PID"), X.AnyPropertyType)
                if pid and pid.value[0] in exclude_pids: continue
                st = w.get_full_property(A("_NET_WM_STATE"), X.AnyPropertyType)
                if st and A("_NET_WM_STATE_HIDDEN") in st.value: continue
                _walk_x(d, root, w, out, 0, max_depth, min_size)
            except Exception:
                continue
        d.close()
    except Exception:
        return []
    # Prefer smaller rects first for hover hit-testing
    out.sort(key=lambda rt: rt[0].width() * rt[0].height())
    return out


def _walk_x(d, root, w, out, depth, max_depth, min_size):
    try:
        attrs = w.get_attributes()
        if attrs.map_state != 2:  # IsViewable
            return
        g = w.get_geometry()
        if g.width < min_size or g.height < min_size: return
        t = w.translate_coords(root, 0, 0)
        out.append((QRect(-t.x, -t.y, g.width, g.height), f"x11:{w.id:#x}"))
        if depth >= max_depth: return
        for cid in w.query_tree().children:
            _walk_x(d, root, d.create_resource_object("window", cid), out, depth + 1, max_depth, min_size)
    except Exception:
        return


def hover_rect(mode, cur, windows, elements, full):
    """Pick highlight rect for cursor `cur` (overlay-local) given translated lists."""
    if mode == "element" and elements:
        hits = [r for r, _ in elements if r.contains(cur)]
        if hits:
            return min(hits, key=lambda r: r.width() * r.height())
    hits = [r.intersected(full) for r, _ in windows if r.contains(cur)]
    return hits[0] if hits else full
